use kild_paths::KildPaths;
use tracing::{info, warn};

use crate::agents;
use crate::sessions::errors::SessionError;

/// Result of building a daemon PTY create request.
#[derive(Debug)]
pub(super) struct DaemonSpawnParams {
    pub cmd: String,
    pub cmd_args: Vec<String>,
    pub env_vars: Vec<(String, String)>,
    pub use_login_shell: bool,
}

/// Deliver an initial prompt to a daemon session's PTY stdin (best-effort).
///
/// Waits for the agent's TUI to fully settle before injecting — both text and Enter
/// are written after the scrollback stabilizes, not before startup. This is necessary
/// because most agents flush PTY stdin during TUI initialization, and some (gemini, amp)
/// continue loading after the first render before their input loop is truly ready.
///
/// Detection: scrollback must exceed 50 bytes AND stop growing for 500ms.
/// Write order: text → 50ms pause → `\r` (same cadence as `kild inject`).
/// Never blocks the caller beyond 20s. Never fails — logs and returns on any error.
pub(super) fn deliver_initial_prompt(daemon_session_id: &str, prompt: &str) {
    let timeout = std::time::Duration::from_secs(20);
    let poll_interval = std::time::Duration::from_millis(200);
    let stable_window = std::time::Duration::from_millis(500);
    let start = std::time::Instant::now();
    let mut last_scrollback_len: usize = 0;
    let mut last_change = std::time::Instant::now();
    let mut tui_ready = false;

    while start.elapsed() < timeout {
        std::thread::sleep(poll_interval);

        match crate::daemon::client::get_session_info(daemon_session_id) {
            Ok(Some((kild_protocol::SessionStatus::Stopped, _))) => break,
            Err(e) => {
                warn!(
                    event = "core.session.initial_prompt_session_info_failed",
                    daemon_session_id = daemon_session_id,
                    error = %e,
                );
                break;
            }
            _ => {}
        }

        let scrollback_len = match crate::daemon::client::read_scrollback(daemon_session_id) {
            Ok(Some(bytes)) => bytes.len(),
            Ok(None) => 0,
            Err(e) => {
                warn!(
                    event = "core.session.initial_prompt_scrollback_failed",
                    daemon_session_id = daemon_session_id,
                    error = %e,
                );
                break;
            }
        };

        if scrollback_len > 50 {
            if scrollback_len != last_scrollback_len {
                last_scrollback_len = scrollback_len;
                last_change = std::time::Instant::now();
            } else if last_change.elapsed() >= stable_window {
                tui_ready = true;
                break;
            }
        }
    }

    if !tui_ready {
        warn!(
            event = "core.session.initial_prompt_tui_timeout",
            daemon_session_id = daemon_session_id,
            elapsed_ms = start.elapsed().as_millis(),
        );
    }

    let text_ok = match crate::daemon::client::write_stdin(daemon_session_id, prompt.as_bytes()) {
        Ok(()) => true,
        Err(e) => {
            warn!(
                event = "core.session.initial_prompt_failed",
                daemon_session_id = daemon_session_id,
                phase = "text",
                error = %e,
            );
            false
        }
    };

    if text_ok {
        std::thread::sleep(std::time::Duration::from_millis(50));

        match crate::daemon::client::write_stdin(daemon_session_id, b"\r") {
            Ok(()) => {
                info!(
                    event = "core.session.initial_prompt_sent",
                    daemon_session_id = daemon_session_id,
                    bytes = prompt.len(),
                    tui_ready = tui_ready,
                    wait_ms = start.elapsed().as_millis(),
                );
            }
            Err(e) => {
                warn!(
                    event = "core.session.initial_prompt_failed",
                    daemon_session_id = daemon_session_id,
                    phase = "enter",
                    error = %e,
                );
            }
        }
    }
}

/// Compute a unique spawn ID for a given session and spawn index.
///
/// Each agent spawn within a session gets its own spawn ID, which is used for
/// per-agent PID file paths and window titles. This prevents race conditions
/// where `kild open` on a running kild would read the wrong PID.
pub(super) fn compute_spawn_id(session_id: &str, spawn_index: usize) -> String {
    format!("{}_{}", session_id, spawn_index)
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
pub(super) fn build_daemon_create_request(
    agent_command: &str,
    agent_name: &str,
    session_id: &str,
    task_list_id: Option<&str>,
    branch: &str,
) -> Result<DaemonSpawnParams, SessionError> {
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
    let paths = KildPaths::resolve().map_err(|e| SessionError::DaemonError {
        message: format!("{} — cannot configure tmux shim PATH", e),
    })?;
    let shim_bin_dir = paths.bin_dir();

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
    let zdotdir = paths.shim_zdotdir(session_id);
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

    // $KILD_SESSION_BRANCH for Claude Code status hook reporting
    let claude_env = agents::resume::claude_env_vars(agent_name, branch);
    env_vars.extend(claude_env);

    Ok(DaemonSpawnParams {
        cmd,
        cmd_args,
        env_vars,
        use_login_shell,
    })
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
        let DaemonSpawnParams {
            cmd,
            cmd_args: args,
            use_login_shell,
            ..
        } = build_daemon_create_request(
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
        let DaemonSpawnParams {
            cmd,
            cmd_args: args,
            use_login_shell,
            ..
        } = build_daemon_create_request("claude", "claude", "test-session", None, "test-branch")
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
        let DaemonSpawnParams {
            cmd_args: args,
            use_login_shell,
            ..
        } = build_daemon_create_request("/bin/zsh", "shell", "test-session", None, "test-branch")
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
        let params = result.unwrap();
        assert!(params.use_login_shell);
    }

    #[test]
    fn test_build_daemon_request_agent_escapes_single_quotes() {
        let DaemonSpawnParams { cmd_args: args, .. } = build_daemon_create_request(
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
        let DaemonSpawnParams { env_vars, .. } =
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
        let DaemonSpawnParams { env_vars, .. } =
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
        let DaemonSpawnParams { env_vars, .. } = build_daemon_create_request(
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
            let DaemonSpawnParams { env_vars, .. } = build_daemon_create_request(
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
        let DaemonSpawnParams { env_vars, .. } =
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
        let DaemonSpawnParams { env_vars, .. } =
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
    fn test_build_daemon_request_includes_claude_env_vars() {
        let DaemonSpawnParams { env_vars, .. } =
            build_daemon_create_request("claude", "claude", "test-session", None, "my-feature")
                .unwrap();

        let branch_val = env_vars
            .iter()
            .find(|(k, _)| k == "KILD_SESSION_BRANCH")
            .map(|(_, v)| v.as_str());
        assert_eq!(
            branch_val,
            Some("my-feature"),
            "KILD_SESSION_BRANCH should be set for claude agent"
        );
    }
}
