use std::path::Path;

use tracing::{debug, info, warn};

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

/// Ensure the Codex notify hook script is installed at `<home>/.kild/hooks/codex-notify`.
///
/// This script is called by Codex CLI's `notify` config. It reads JSON from stdin,
/// maps event types to KILD agent statuses, and calls `kild agent-status`.
/// Event mappings: `agent-turn-complete` → `idle`, `approval-requested` → `waiting`.
/// Idempotent: skips if script already exists.
fn ensure_codex_notify_hook_with_home(home: &Path) -> Result<(), String> {
    let hooks_dir = home.join(".kild").join("hooks");
    let hook_path = hooks_dir.join("codex-notify");

    if hook_path.exists() {
        debug!(
            event = "core.session.codex_notify_hook_already_exists",
            path = %hook_path.display()
        );
        return Ok(());
    }

    std::fs::create_dir_all(&hooks_dir)
        .map_err(|e| format!("failed to create {}: {}", hooks_dir.display(), e))?;

    let script = r#"#!/bin/sh
# KILD Codex notify hook — auto-generated, do not edit.
# Called by Codex CLI via notify config with JSON on stdin.
# Maps Codex events to KILD agent statuses.
INPUT=$(cat)
EVENT_TYPE=$(echo "$INPUT" | grep -o '"type":"[^"]*"' | head -1 | sed 's/"type":"//;s/"//')
case "$EVENT_TYPE" in
  agent-turn-complete) kild agent-status --self idle --notify ;;
  approval-requested)  kild agent-status --self waiting --notify ;;
esac
"#;

    std::fs::write(&hook_path, script)
        .map_err(|e| format!("failed to write {}: {}", hook_path.display(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("failed to chmod {}: {}", hook_path.display(), e))?;
    }

    info!(
        event = "core.session.codex_notify_hook_installed",
        path = %hook_path.display()
    );

    Ok(())
}

pub(crate) fn ensure_codex_notify_hook() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("HOME not set — cannot install Codex notify hook")?;
    ensure_codex_notify_hook_with_home(&home)
}

/// Ensure Codex CLI config has the KILD notify hook configured.
///
/// Patches `<home>/.codex/config.toml` to add `notify = ["<path>"]` if the notify
/// field is missing or empty. Respects existing user configuration — if notify
/// is already set to a non-empty array, it is left unchanged and this function
/// returns Ok without modifying the file.
fn ensure_codex_config_with_home(home: &Path) -> Result<(), String> {
    let codex_dir = home.join(".codex");
    let config_path = codex_dir.join("config.toml");
    let hook_path = home.join(".kild").join("hooks").join("codex-notify");
    let hook_path_str = hook_path.display().to_string();

    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("failed to read {}: {}", config_path.display(), e))?;

        // Parse to check if notify is already configured with a non-empty array.
        // Propagate parse errors so we don't blindly append to a malformed file.
        let parsed = content.parse::<toml::Value>().map_err(|e| {
            format!(
                "failed to parse {}: {} — fix TOML syntax or remove the file to reset",
                config_path.display(),
                e
            )
        })?;

        if let Some(toml::Value::Array(arr)) = parsed.get("notify")
            && !arr.is_empty()
        {
            info!(event = "core.session.codex_config_already_configured");
            return Ok(());
        }

        // notify is missing or empty — append it, preserving existing content
        let mut new_content = content;
        if !new_content.ends_with('\n') && !new_content.is_empty() {
            new_content.push('\n');
        }
        new_content.push_str(&format!("notify = [\"{}\"]\n", hook_path_str));

        std::fs::write(&config_path, new_content)
            .map_err(|e| format!("failed to write {}: {}", config_path.display(), e))?;
    } else {
        // Config doesn't exist — create it with just the notify line
        std::fs::create_dir_all(&codex_dir)
            .map_err(|e| format!("failed to create {}: {}", codex_dir.display(), e))?;
        std::fs::write(&config_path, format!("notify = [\"{}\"]\n", hook_path_str))
            .map_err(|e| format!("failed to write {}: {}", config_path.display(), e))?;
    }

    info!(
        event = "core.session.codex_config_patched",
        path = %config_path.display()
    );

    Ok(())
}

pub(crate) fn ensure_codex_config() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("HOME not set — cannot patch Codex config")?;
    ensure_codex_config_with_home(&home)
}

/// Install Codex notify hook and patch config if needed.
///
/// Best-effort: warns on failure but doesn't block session creation.
/// No-op for non-Codex agents.
pub(crate) fn setup_codex_integration(agent: &str) {
    if agent != "codex" {
        return;
    }

    if let Err(msg) = ensure_codex_notify_hook() {
        warn!(event = "core.session.codex_notify_hook_failed", error = %msg);
        eprintln!("Warning: {msg}");
        eprintln!("Codex status reporting may not work.");
    }

    if let Err(msg) = ensure_codex_config() {
        warn!(event = "core.session.codex_config_patch_failed", error = %msg);
        eprintln!("Warning: {msg}");
        let hook_path = dirs::home_dir()
            .map(|h| h.join(".kild/hooks/codex-notify").display().to_string())
            .unwrap_or_else(|| "<HOME>/.kild/hooks/codex-notify".to_string());
        let config_path = dirs::home_dir()
            .map(|h| h.join(".codex/config.toml").display().to_string())
            .unwrap_or_else(|| "<HOME>/.codex/config.toml".to_string());
        eprintln!("Add notify = [\"{hook_path}\"] to {config_path} manually.");
    }
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
///
/// The `branch` is used to inject `KILD_SESSION_BRANCH` for agents like Codex that need
/// to report their status back to KILD via notify hooks.
#[allow(clippy::type_complexity)]
pub(super) fn build_daemon_create_request(
    agent_command: &str,
    agent_name: &str,
    session_id: &str,
    task_list_id: Option<&str>,
    branch: &str,
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

    // $KILD_SESSION_BRANCH for Codex notify hook status reporting
    let codex_env = agents::resume::codex_env_vars(agent_name, branch);
    env_vars.extend(codex_env);

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
        let (cmd, args, _env, use_login_shell) = build_daemon_create_request(
            "claude --agent --verbose",
            "claude",
            "test-session",
            None,
            "test-branch",
        )
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
            build_daemon_create_request("claude", "claude", "test-session", None, "test-branch")
                .unwrap();
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
            build_daemon_create_request("/bin/zsh", "shell", "test-session", None, "test-branch")
                .unwrap();
        assert!(use_login_shell, "Bare shell should use login shell mode");
        assert!(args.is_empty(), "Login shell mode should have no args");
    }

    #[test]
    fn test_build_daemon_request_empty_command_returns_error() {
        let result = build_daemon_create_request("", "claude", "test-session", None, "test-branch");
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
        let result =
            build_daemon_create_request("   ", "kiro", "test-session", None, "test-branch");
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
        let result = build_daemon_create_request("", "shell", "test-session", None, "test-branch");
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
            "test-branch",
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
            build_daemon_create_request("claude", "claude", "test-session", None, "test-branch")
                .unwrap();

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
            build_daemon_create_request("claude", "claude", "proj_my-branch", None, "my-branch")
                .unwrap();

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
            "my-branch",
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
                "test-branch",
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
            build_daemon_create_request("claude", "claude", "test-session", None, "test-branch")
                .unwrap();

        let has_task_list = env_vars
            .iter()
            .any(|(k, _)| k == "CLAUDE_CODE_TASK_LIST_ID");
        assert!(
            !has_task_list,
            "CLAUDE_CODE_TASK_LIST_ID should not be set when task_list_id is None"
        );
    }

    #[test]
    fn test_build_daemon_request_includes_codex_env_vars() {
        let (_cmd, _args, env_vars, _) =
            build_daemon_create_request("codex", "codex", "test-session", None, "my-feature")
                .unwrap();

        let branch_val = env_vars
            .iter()
            .find(|(k, _)| k == "KILD_SESSION_BRANCH")
            .map(|(_, v)| v.as_str());
        assert_eq!(
            branch_val,
            Some("my-feature"),
            "KILD_SESSION_BRANCH should be set for codex agent"
        );
    }

    #[test]
    fn test_build_daemon_request_no_codex_env_for_claude() {
        let (_cmd, _args, env_vars, _) =
            build_daemon_create_request("claude", "claude", "test-session", None, "my-feature")
                .unwrap();

        let has_branch = env_vars.iter().any(|(k, _)| k == "KILD_SESSION_BRANCH");
        assert!(
            !has_branch,
            "KILD_SESSION_BRANCH should not be set for claude agent"
        );
    }

    #[test]
    fn test_ensure_codex_notify_hook_creates_script() {
        use std::fs;

        let temp_home =
            std::env::temp_dir().join(format!("kild_test_codex_hook_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_home);
        let hook_path = temp_home.join(".kild").join("hooks").join("codex-notify");

        let result = ensure_codex_notify_hook_with_home(&temp_home);
        assert!(result.is_ok(), "Hook install should succeed: {:?}", result);
        assert!(hook_path.exists(), "Hook script should exist");

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(
            content.starts_with("#!/bin/sh"),
            "Script should have shebang"
        );
        assert!(
            content.contains("agent-turn-complete"),
            "Script should handle agent-turn-complete"
        );
        assert!(
            content.contains("approval-requested"),
            "Script should handle approval-requested"
        );
        assert!(
            content.contains("kild agent-status --self idle --notify"),
            "Script should call kild agent-status for idle"
        );
        assert!(
            content.contains("kild agent-status --self waiting --notify"),
            "Script should call kild agent-status for waiting"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&hook_path).unwrap().permissions().mode();
            assert!(
                mode & 0o111 != 0,
                "Script should be executable, mode: {:o}",
                mode
            );
        }

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_codex_notify_hook_idempotent() {
        use std::fs;

        let temp_home =
            std::env::temp_dir().join(format!("kild_test_codex_hook_idem_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_home);
        let hook_path = temp_home.join(".kild").join("hooks").join("codex-notify");

        // First call creates the script
        let result = ensure_codex_notify_hook_with_home(&temp_home);
        assert!(result.is_ok());
        let content1 = fs::read_to_string(&hook_path).unwrap();

        // Second call should succeed without changing content
        let result = ensure_codex_notify_hook_with_home(&temp_home);
        assert!(result.is_ok());
        let content2 = fs::read_to_string(&hook_path).unwrap();
        assert_eq!(
            content1, content2,
            "Content should not change on second call"
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_codex_config_patches_empty_config() {
        use std::fs;

        let temp_home =
            std::env::temp_dir().join(format!("kild_test_codex_cfg_empty_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_home);
        let codex_dir = temp_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("config.toml"), "").unwrap();

        let result = ensure_codex_config_with_home(&temp_home);
        assert!(result.is_ok(), "Config patch should succeed: {:?}", result);

        let content = fs::read_to_string(codex_dir.join("config.toml")).unwrap();
        assert!(
            content.contains("notify = [\""),
            "Config should contain notify setting, got: {}",
            content
        );
        assert!(
            content.contains("codex-notify"),
            "Config should reference codex-notify hook, got: {}",
            content
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_codex_config_preserves_existing_notify() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_codex_cfg_existing_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);
        let codex_dir = temp_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(
            codex_dir.join("config.toml"),
            "notify = [\"my-custom-program\"]\n",
        )
        .unwrap();

        let result = ensure_codex_config_with_home(&temp_home);
        assert!(result.is_ok());

        let content = fs::read_to_string(codex_dir.join("config.toml")).unwrap();
        assert!(
            content.contains("my-custom-program"),
            "Custom notify should be preserved"
        );
        assert!(
            !content.contains("codex-notify"),
            "Should NOT overwrite user's custom notify config"
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_codex_config_patches_empty_notify_array() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_codex_cfg_empty_arr_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);
        let codex_dir = temp_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("config.toml"), "notify = []\n").unwrap();

        let result = ensure_codex_config_with_home(&temp_home);
        assert!(result.is_ok());

        let content = fs::read_to_string(codex_dir.join("config.toml")).unwrap();
        assert!(
            content.contains("codex-notify"),
            "Empty notify array should be patched, got: {}",
            content
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_codex_config_creates_new_config() {
        use std::fs;

        let temp_home =
            std::env::temp_dir().join(format!("kild_test_codex_cfg_new_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_home);
        // Don't create .codex dir — it shouldn't exist yet

        let result = ensure_codex_config_with_home(&temp_home);
        assert!(
            result.is_ok(),
            "Should create config from scratch: {:?}",
            result
        );

        let config_path = temp_home.join(".codex").join("config.toml");
        assert!(config_path.exists(), "Config file should be created");

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(
            content.contains("notify = [\""),
            "New config should contain notify, got: {}",
            content
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_codex_config_preserves_existing_content() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_codex_cfg_preserve_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);
        let codex_dir = temp_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(
            codex_dir.join("config.toml"),
            "[model]\nprovider = \"openai\"\n",
        )
        .unwrap();

        let result = ensure_codex_config_with_home(&temp_home);
        assert!(result.is_ok());

        let content = fs::read_to_string(codex_dir.join("config.toml")).unwrap();
        assert!(
            content.contains("[model]"),
            "Existing content should be preserved"
        );
        assert!(
            content.contains("provider = \"openai\""),
            "Existing settings should be preserved"
        );
        assert!(content.contains("codex-notify"), "notify should be added");

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_codex_config_rejects_malformed_toml() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_codex_cfg_malformed_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);
        let codex_dir = temp_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("config.toml"), "[invalid toml syntax\n").unwrap();

        let result = ensure_codex_config_with_home(&temp_home);
        assert!(result.is_err(), "Should fail on malformed TOML");

        let err = result.unwrap_err();
        assert!(
            err.contains("failed to parse"),
            "Error should mention parse failure, got: {}",
            err
        );
        assert!(
            err.contains("fix TOML syntax"),
            "Error should suggest fixing TOML syntax, got: {}",
            err
        );

        // Verify the file was NOT modified
        let content = fs::read_to_string(codex_dir.join("config.toml")).unwrap();
        assert_eq!(
            content, "[invalid toml syntax\n",
            "Malformed file should not be modified"
        );

        let _ = fs::remove_dir_all(&temp_home);
    }
}
