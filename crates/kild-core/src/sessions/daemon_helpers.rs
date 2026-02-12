use tracing::{info, warn};

use crate::agents;
use crate::sessions::errors::SessionError;

/// Compute a unique spawn ID for a given session and spawn index.
///
/// Each agent spawn within a session gets its own spawn ID, which is used for
/// per-agent PID file paths and window titles. This prevents race conditions
/// where `kild open` on a running kild would read the wrong PID.
pub(super) fn compute_spawn_id(session_id: &str, spawn_index: usize) -> String {
    format!("{}_{}", session_id, spawn_index)
}

/// Ensure the tmux shim binary is installed at `~/.kild/bin/tmux`.
///
/// Looks for `kild-tmux-shim` next to the running `kild` binary and symlinks
/// it as `tmux` in `~/.kild/bin/`. Agent teams require this binary.
pub(crate) fn ensure_shim_binary() -> Result<(), String> {
    let shim_bin_dir = dirs::home_dir()
        .ok_or("HOME not set — cannot install tmux shim")?
        .join(".kild")
        .join("bin");
    let shim_link = shim_bin_dir.join("tmux");

    if shim_link.exists() {
        return Ok(());
    }

    let shim_binary = crate::daemon::find_sibling_binary("kild-tmux-shim")?;

    std::fs::create_dir_all(&shim_bin_dir)
        .map_err(|e| format!("failed to create {}: {}", shim_bin_dir.display(), e))?;

    #[cfg(unix)]
    std::os::unix::fs::symlink(&shim_binary, &shim_link).map_err(|e| {
        format!(
            "failed to symlink {} -> {}: {}",
            shim_binary.display(),
            shim_link.display(),
            e
        )
    })?;

    info!(
        event = "core.session.shim_binary_installed",
        path = %shim_link.display()
    );

    Ok(())
}

/// Build the command, args, env vars, and login shell flag for a daemon PTY create request.
///
/// Both `create_session` and `open_session` need to parse the agent command string
/// and collect environment variables for the daemon. This helper centralises that logic.
///
/// Two strategies based on agent type:
/// - **Bare shell** (`agent_name == "shell"`): Sets `use_login_shell = true` so the daemon
///   uses `CommandBuilder::new_default_prog()` for a native login shell with profile sourcing.
/// - **Agents**: Wraps in `$SHELL -lc 'exec <command>'` so profile files are sourced
///   before the agent starts, providing full PATH and environment. The `exec` replaces
///   the wrapper shell with the agent for clean process tracking.
///
/// The `session_id` is used to set up tmux shim environment variables so that agents
/// running inside daemon PTYs see a `$TMUX` environment and can use pane-based workflows.
#[allow(clippy::type_complexity)]
pub(super) fn build_daemon_create_request(
    agent_command: &str,
    agent_name: &str,
    session_id: &str,
    task_list_id: Option<&str>,
) -> Result<(String, Vec<String>, Vec<(String, String)>, bool), SessionError> {
    let use_login_shell = agent_name == "shell";

    let (cmd, cmd_args) = if use_login_shell {
        // For bare shell: command/args are ignored by new_default_prog(),
        // but we still pass them for logging purposes.
        (agent_command.to_string(), vec![])
    } else {
        // For agents: validate command is non-empty, then wrap in login shell.
        // sh -lc 'exec claude --flags' ensures profile files are sourced.
        if agent_command.split_whitespace().next().is_none() {
            return Err(SessionError::DaemonError {
                message: format!(
                    "Empty command string for agent '{}'. Check agent configuration.",
                    agent_name
                ),
            });
        }
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let escaped = agent_command.replace('\'', "'\\''");
        (shell, vec!["-lc".to_string(), format!("exec {}", escaped)])
    };

    let mut env_vars = Vec::new();
    for key in &["PATH", "HOME", "SHELL", "USER", "LANG", "TERM"] {
        if let Ok(val) = std::env::var(key) {
            env_vars.push((key.to_string(), val));
        }
    }

    // tmux shim environment for daemon sessions
    let home_dir = dirs::home_dir().ok_or_else(|| SessionError::DaemonError {
        message: "HOME not set — cannot configure tmux shim PATH".to_string(),
    })?;
    let shim_bin_dir = home_dir.join(".kild").join("bin");

    // Prepend shim dir to PATH so our tmux shim is found first.
    // NOTE: For login shells on macOS, /etc/zprofile runs path_helper which
    // reconstructs PATH and may push this to the end. The ZDOTDIR wrapper
    // below re-prepends it after all profile scripts have run.
    if let Some(path_entry) = env_vars.iter_mut().find(|(k, _)| k == "PATH") {
        path_entry.1 = format!("{}:{}", shim_bin_dir.display(), path_entry.1);
    } else if let Ok(system_path) = std::env::var("PATH") {
        env_vars.push((
            "PATH".to_string(),
            format!("{}:{}", shim_bin_dir.display(), system_path),
        ));
    }

    // Create a ZDOTDIR wrapper so that ~/.kild/bin is prepended to PATH
    // AFTER login shell profile scripts run (macOS path_helper in /etc/zprofile
    // reconstructs PATH and drops our prepended entry).
    let zdotdir = home_dir
        .join(".kild")
        .join("shim")
        .join(session_id)
        .join("zdotdir");
    if let Err(e) = create_zdotdir_wrapper(&zdotdir, &shim_bin_dir) {
        warn!(
            event = "core.session.zdotdir_setup_failed",
            session_id = session_id,
            error = %e,
        );
        eprintln!(
            "Warning: Failed to set up shell PATH wrapper: {}. \
             The tmux shim may not be found by agents (macOS path_helper can reorder PATH).",
            e
        );
    } else {
        env_vars.push(("ZDOTDIR".to_string(), zdotdir.display().to_string()));
    }

    // $TMUX triggers Claude Code's tmux pane backend (auto mode)
    let daemon_sock = crate::daemon::socket_path();
    env_vars.push((
        "TMUX".to_string(),
        format!("{},{},0", daemon_sock.display(), std::process::id()),
    ));

    // $TMUX_PANE identifies the leader's own pane
    env_vars.push(("TMUX_PANE".to_string(), "%0".to_string()));

    // $KILD_SHIM_SESSION tells the shim where to find its state
    env_vars.push(("KILD_SHIM_SESSION".to_string(), session_id.to_string()));

    // $CLAUDE_CODE_TASK_LIST_ID for task list persistence across sessions
    if let Some(tlid) = task_list_id {
        let task_env = agents::resume::task_list_env_vars(agent_name, tlid);
        env_vars.extend(task_env);
    }

    Ok((cmd, cmd_args, env_vars, use_login_shell))
}

/// Create a ZDOTDIR wrapper that re-prepends `~/.kild/bin` to PATH.
///
/// On macOS, login shells source `/etc/zprofile` which runs `path_helper`,
/// reconstructing PATH from `/etc/paths` and dropping any prepended entries.
/// This wrapper sources the user's real `~/.zshrc` then prepends our shim dir,
/// ensuring `~/.kild/bin/tmux` is always found first.
fn create_zdotdir_wrapper(
    zdotdir: &std::path::Path,
    shim_bin_dir: &std::path::Path,
) -> Result<(), String> {
    std::fs::create_dir_all(zdotdir).map_err(|e| format!("failed to create zdotdir: {}", e))?;

    // .zshenv runs before .zprofile — we need .zshrc which runs after.
    // But we also need .zshenv to reset ZDOTDIR so the user's own .zshenv
    // and .zshrc are sourced from their real home directory.
    // zsh dotfile load order: .zshenv → .zprofile (login) → .zshrc (interactive)
    // ZDOTDIR must stay set throughout so zsh reads ALL our wrappers.
    // Each wrapper sources the user's real file from $HOME.
    // .zshrc (last) unsets ZDOTDIR so nested/child shells behave normally.

    let zshenv_content = r#"# KILD shim — auto-generated, do not edit.
# Source user's real .zshenv if it exists.
[[ -f "$HOME/.zshenv" ]] && source "$HOME/.zshenv"
"#;

    let zprofile_content = r#"# KILD shim — auto-generated, do not edit.
# Source user's real .zprofile if it exists.
[[ -f "$HOME/.zprofile" ]] && source "$HOME/.zprofile"
"#;

    let zshrc_content = format!(
        r#"# KILD shim — auto-generated, do not edit.
# Source user's real .zshrc if it exists.
[[ -f "$HOME/.zshrc" ]] && source "$HOME/.zshrc"

# Re-prepend shim bin dir to PATH (macOS path_helper may have reordered it).
export PATH="{shim_bin}:$PATH"

# Reset ZDOTDIR so child shells use the user's real dotfiles.
unset ZDOTDIR
"#,
        shim_bin = shim_bin_dir.display(),
    );

    std::fs::write(zdotdir.join(".zshenv"), zshenv_content)
        .map_err(|e| format!("failed to write .zshenv: {}", e))?;
    std::fs::write(zdotdir.join(".zprofile"), zprofile_content)
        .map_err(|e| format!("failed to write .zprofile: {}", e))?;
    std::fs::write(zdotdir.join(".zshrc"), zshrc_content)
        .map_err(|e| format!("failed to write .zshrc: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_daemon_request_agent_wraps_in_login_shell() {
        let (cmd, args, _env, use_login_shell) =
            build_daemon_create_request("claude --agent --verbose", "claude", "test-session", None)
                .unwrap();
        assert!(!use_login_shell, "Agent should not use login shell mode");
        // Agent commands are wrapped in $SHELL -lc 'exec <command>'
        assert!(
            cmd.ends_with("sh") || cmd.ends_with("zsh") || cmd.ends_with("bash"),
            "Command should be a shell, got: {}",
            cmd
        );
        assert_eq!(args.len(), 2, "Should have -lc and the exec command");
        assert_eq!(args[0], "-lc");
        assert!(
            args[1].contains("exec claude --agent --verbose"),
            "Should wrap command with exec, got: {}",
            args[1]
        );
    }

    #[test]
    fn test_build_daemon_request_single_word_agent_wraps_in_login_shell() {
        let (cmd, args, _env, use_login_shell) =
            build_daemon_create_request("claude", "claude", "test-session", None).unwrap();
        assert!(!use_login_shell);
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "-lc");
        assert!(args[1].contains("exec claude"), "got: {}", args[1]);
        assert!(
            cmd.ends_with("sh") || cmd.ends_with("zsh") || cmd.ends_with("bash"),
            "got: {}",
            cmd
        );
    }

    #[test]
    fn test_build_daemon_request_bare_shell_uses_login_shell() {
        let (_cmd, args, _env, use_login_shell) =
            build_daemon_create_request("/bin/zsh", "shell", "test-session", None).unwrap();
        assert!(use_login_shell, "Bare shell should use login shell mode");
        assert!(args.is_empty(), "Login shell mode should have no args");
    }

    #[test]
    fn test_build_daemon_request_empty_command_returns_error() {
        let result = build_daemon_create_request("", "claude", "test-session", None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            SessionError::DaemonError { message } => {
                assert!(
                    message.contains("claude"),
                    "Error should mention agent name, got: {}",
                    message
                );
                assert!(
                    message.contains("Empty command"),
                    "Error should mention empty command, got: {}",
                    message
                );
            }
            other => panic!("Expected DaemonError, got: {:?}", other),
        }
    }

    #[test]
    fn test_build_daemon_request_whitespace_only_command_returns_error() {
        let result = build_daemon_create_request("   ", "kiro", "test-session", None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            SessionError::DaemonError { message } => {
                assert!(message.contains("kiro"));
            }
            other => panic!("Expected DaemonError, got: {:?}", other),
        }
    }

    #[test]
    fn test_build_daemon_request_bare_shell_empty_command_still_works() {
        // Bare shell with empty-ish command: since use_login_shell=true,
        // the command is passed through for logging only (daemon ignores it)
        let result = build_daemon_create_request("", "shell", "test-session", None);
        assert!(result.is_ok(), "Bare shell should accept empty command");
        let (_cmd, _args, _env, use_login_shell) = result.unwrap();
        assert!(use_login_shell);
    }

    #[test]
    fn test_build_daemon_request_agent_escapes_single_quotes() {
        let (_, args, _, _) = build_daemon_create_request(
            "claude --note 'hello world'",
            "claude",
            "test-session",
            None,
        )
        .unwrap();
        assert!(
            args[1].contains("exec claude --note"),
            "Should contain the command, got: {}",
            args[1]
        );
    }

    #[test]
    fn test_build_daemon_request_collects_env_vars() {
        let (_cmd, _args, env_vars, _) =
            build_daemon_create_request("claude", "claude", "test-session", None).unwrap();

        // PATH and HOME should always be present in the environment
        let keys: Vec<&str> = env_vars.iter().map(|(k, _)| k.as_str()).collect();
        assert!(
            keys.contains(&"PATH"),
            "Should collect PATH env var, got keys: {:?}",
            keys
        );
        assert!(
            keys.contains(&"HOME"),
            "Should collect HOME env var, got keys: {:?}",
            keys
        );
    }

    #[test]
    fn test_build_daemon_request_includes_shim_env_vars() {
        let (_cmd, _args, env_vars, _) =
            build_daemon_create_request("claude", "claude", "proj_my-branch", None).unwrap();

        let keys: Vec<&str> = env_vars.iter().map(|(k, _)| k.as_str()).collect();

        // Should include tmux shim environment variables
        assert!(
            keys.contains(&"TMUX"),
            "Should set TMUX env var, got keys: {:?}",
            keys
        );
        assert!(
            keys.contains(&"TMUX_PANE"),
            "Should set TMUX_PANE env var, got keys: {:?}",
            keys
        );
        assert!(
            keys.contains(&"KILD_SHIM_SESSION"),
            "Should set KILD_SHIM_SESSION env var, got keys: {:?}",
            keys
        );

        // KILD_SHIM_SESSION should contain the session_id
        let shim_session = env_vars
            .iter()
            .find(|(k, _)| k == "KILD_SHIM_SESSION")
            .map(|(_, v)| v.as_str());
        assert_eq!(shim_session, Some("proj_my-branch"));

        // TMUX_PANE should be %0
        let tmux_pane = env_vars
            .iter()
            .find(|(k, _)| k == "TMUX_PANE")
            .map(|(_, v)| v.as_str());
        assert_eq!(tmux_pane, Some("%0"));

        // PATH should be prepended with shim bin dir
        let path_val = env_vars
            .iter()
            .find(|(k, _)| k == "PATH")
            .map(|(_, v)| v.as_str())
            .unwrap();
        assert!(
            path_val.contains(".kild/bin"),
            "PATH should contain .kild/bin shim dir, got: {}",
            path_val
        );
    }

    #[test]
    fn test_build_daemon_request_includes_task_list_env_var_for_claude() {
        let (_cmd, _args, env_vars, _) = build_daemon_create_request(
            "claude",
            "claude",
            "myproject_my-branch",
            Some("kild-myproject_my-branch"),
        )
        .unwrap();

        let task_list_val = env_vars
            .iter()
            .find(|(k, _)| k == "CLAUDE_CODE_TASK_LIST_ID")
            .map(|(_, v)| v.as_str());
        assert_eq!(
            task_list_val,
            Some("kild-myproject_my-branch"),
            "CLAUDE_CODE_TASK_LIST_ID should be set for claude agent"
        );
    }

    #[test]
    fn test_build_daemon_request_no_task_list_env_var_for_non_claude() {
        for (agent_cmd, agent_name) in &[
            ("kiro", "kiro"),
            ("gemini", "gemini"),
            ("amp", "amp"),
            ("opencode", "opencode"),
        ] {
            let (_cmd, _args, env_vars, _) = build_daemon_create_request(
                agent_cmd,
                agent_name,
                "test-session",
                Some("kild-test"),
            )
            .unwrap();

            let has_task_list = env_vars
                .iter()
                .any(|(k, _)| k == "CLAUDE_CODE_TASK_LIST_ID");
            assert!(
                !has_task_list,
                "CLAUDE_CODE_TASK_LIST_ID should not be set for agent '{}'",
                agent_name
            );
        }
    }

    #[test]
    fn test_compute_spawn_id_produces_unique_ids() {
        let session_id = "myproject_feature-auth";
        let id_0 = compute_spawn_id(session_id, 0);
        let id_1 = compute_spawn_id(session_id, 1);
        let id_2 = compute_spawn_id(session_id, 2);
        assert_eq!(id_0, "myproject_feature-auth_0");
        assert_eq!(id_1, "myproject_feature-auth_1");
        assert_eq!(id_2, "myproject_feature-auth_2");
        assert_ne!(id_0, id_1);
        assert_ne!(id_1, id_2);
    }

    #[test]
    fn test_build_daemon_request_no_task_list_env_var_when_none() {
        let (_cmd, _args, env_vars, _) =
            build_daemon_create_request("claude", "claude", "test-session", None).unwrap();

        let has_task_list = env_vars
            .iter()
            .any(|(k, _)| k == "CLAUDE_CODE_TASK_LIST_ID");
        assert!(
            !has_task_list,
            "CLAUDE_CODE_TASK_LIST_ID should not be set when task_list_id is None"
        );
    }
}
