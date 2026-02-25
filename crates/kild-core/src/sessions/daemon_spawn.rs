//! Shared agent spawn sequences for daemon and terminal paths.
//!
//! Both `create_session` and `open_session` execute nearly identical daemon/terminal
//! spawn sequences. This module extracts the shared logic so each lifecycle file
//! stays focused on its unique orchestration (worktree creation, resume flags, etc.).

use std::path::Path;

use tracing::{debug, warn};

use crate::agents;
use crate::sessions::errors::SessionError;
use crate::sessions::types::AgentProcess;
use crate::terminal;
use kild_config::{Config, KildConfig};

use super::daemon_request::build_daemon_create_request;
use super::integrations::{
    setup_claude_integration, setup_codex_integration, setup_opencode_integration,
};
use super::{dropbox, fleet};

/// Everything needed to spawn an agent in either a daemon PTY or an external terminal.
pub(super) struct AgentSpawnParams<'a> {
    pub branch: &'a str,
    pub agent: &'a str,
    pub agent_command: &'a str,
    pub worktree_path: &'a Path,
    pub session_id: &'a str,
    pub spawn_id: &'a str,
    pub task_list_id: Option<&'a str>,
    pub project_id: &'a str,
    pub kild_config: &'a KildConfig,
}

/// Spawn an agent in a daemon-managed PTY.
///
/// Covers the shared daemon spawn sequence:
/// 1. Ensure daemon is running
/// 2. Agent integration setup (codex, opencode, claude hooks)
/// 3. Fleet member + dropbox setup
/// 4. Fleet agent flags → command augmentation
/// 5. Build daemon create request (env vars, shim, login shell)
/// 6. Inject dropbox env vars
/// 7. Create PTY session via daemon IPC
/// 8. Early exit detection with exponential backoff
/// 9. Scrollback capture on early exit
/// 10. Construct `AgentProcess` result
///
/// Create-only steps (shim binary, pre-emptive cleanup, pane registry init)
/// and open-only steps (initial prompt delivery) remain in their respective callers.
pub(super) fn spawn_daemon_agent(
    params: &AgentSpawnParams<'_>,
) -> Result<AgentProcess, SessionError> {
    let now = chrono::Utc::now().to_rfc3339();

    // 1. Auto-start daemon if not running
    crate::daemon::ensure_daemon_running(params.kild_config)?;

    // 2. Agent integration setup (hooks, config patching)
    setup_codex_integration(params.agent);
    setup_opencode_integration(params.agent, params.worktree_path);
    setup_claude_integration(params.agent);

    // 3. Fleet member + dropbox setup
    fleet::ensure_fleet_member(params.branch, params.worktree_path, params.agent);
    dropbox::ensure_dropbox(params.project_id, params.branch, params.agent);

    // 4. Fleet agent flags → augmented command
    let fleet_command = match fleet::fleet_agent_flags(params.branch, params.agent) {
        Some(flags) => format!("{} {}", params.agent_command, flags),
        None => params.agent_command.to_string(),
    };

    // 5. Build daemon create request
    let mut req_params = build_daemon_create_request(
        &fleet_command,
        params.agent,
        params.session_id,
        params.task_list_id,
        params.branch,
    )?;

    // 6. Inject dropbox env vars
    dropbox::inject_dropbox_env_vars(
        &mut req_params.env_vars,
        params.project_id,
        params.branch,
        params.agent,
    );

    // 7. Create PTY session via daemon IPC
    let daemon_request = crate::daemon::client::DaemonCreateRequest {
        request_id: params.spawn_id,
        session_id: params.spawn_id,
        working_directory: params.worktree_path,
        command: &req_params.cmd,
        args: &req_params.cmd_args,
        env_vars: &req_params.env_vars,
        rows: 24,
        cols: 80,
        use_login_shell: req_params.use_login_shell,
    };
    let daemon_result =
        crate::daemon::client::create_pty_session(&daemon_request).map_err(|e| {
            SessionError::DaemonError {
                message: e.to_string(),
            }
        })?;

    // 8. Early exit detection: poll with exponential backoff until Running or Stopped.
    // Fast-failing processes (bad resume session, missing binary, env issues)
    // typically exit within 50ms of spawn. Exit early on Running confirmation.
    // Worst-case window: 350ms (50+100+200) before falling through with None (assume alive).
    let maybe_early_exit: Option<Option<i32>> = {
        let mut result = None;
        for delay_ms in [50u64, 100, 200] {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            match crate::daemon::client::get_session_info(&daemon_result.daemon_session_id) {
                Ok(Some((kild_protocol::SessionStatus::Stopped, exit_code))) => {
                    result = Some(exit_code);
                    break;
                }
                Ok(Some((kild_protocol::SessionStatus::Running, _))) => break,
                _ => {}
            }
        }
        result
    };

    // 9. Handle early exit: read scrollback, clean up, return error
    if let Some(exit_code) = maybe_early_exit {
        let scrollback_tail = read_scrollback_tail(&daemon_result.daemon_session_id);

        if let Err(e) =
            crate::daemon::client::destroy_daemon_session(&daemon_result.daemon_session_id, true)
        {
            warn!(
                event = "core.session.daemon_spawn_cleanup_failed",
                daemon_session_id = %daemon_result.daemon_session_id,
                error = %e,
            );
        }

        return Err(SessionError::DaemonPtyExitedEarly {
            exit_code,
            scrollback_tail,
        });
    }

    // 10. Construct AgentProcess result
    AgentProcess::new(
        params.agent.to_string(),
        params.spawn_id.to_string(),
        None,
        None,
        None,
        None,
        None,
        params.agent_command.to_string(),
        now,
        Some(daemon_result.daemon_session_id),
    )
}

/// Spawn an agent in an external terminal window.
///
/// Covers the shared terminal spawn sequence:
/// 1. Agent integration setup (codex, opencode, claude hooks)
/// 2. Build env prefix (task list, codex, claude env vars)
/// 3. Construct terminal command with env cleanup
/// 4. Spawn terminal window
/// 5. Capture process metadata
/// 6. Construct `AgentProcess` result
pub(super) fn spawn_terminal_agent(
    params: &AgentSpawnParams<'_>,
) -> Result<AgentProcess, SessionError> {
    let now = chrono::Utc::now().to_rfc3339();

    // 1. Agent integration setup
    setup_codex_integration(params.agent);
    setup_opencode_integration(params.agent, params.worktree_path);
    setup_claude_integration(params.agent);

    // 2–3. Build env prefix and terminal command
    let terminal_command = {
        let mut env_prefix: Vec<(String, String)> = Vec::new();
        if let Some(tlid) = params.task_list_id {
            env_prefix.extend(agents::resume::task_list_env_vars(params.agent, tlid));
        }
        env_prefix.extend(agents::resume::codex_env_vars(params.agent, params.branch));
        env_prefix.extend(agents::resume::claude_env_vars(params.agent, params.branch));
        super::env_cleanup::build_env_command(&env_prefix, params.agent_command)
    };
    debug!(
        event = "core.session.terminal_command_constructed",
        command = %terminal_command,
    );

    // 4. Spawn terminal window
    let base_config = Config::new();
    let spawn_result = terminal::handler::spawn_terminal(
        params.worktree_path,
        &terminal_command,
        params.kild_config,
        Some(params.spawn_id),
        Some(base_config.kild_dir()),
    )
    .map_err(|e| SessionError::TerminalError { source: e })?;

    // 5. Capture process metadata (fresh from OS for PID reuse protection)
    let (process_name, process_start_time) = capture_process_metadata(&spawn_result);

    // 6. Construct AgentProcess result
    let command = if spawn_result.command_executed.trim().is_empty() {
        format!("{} (command not captured)", params.agent)
    } else {
        spawn_result.command_executed.clone()
    };

    AgentProcess::new(
        params.agent.to_string(),
        params.spawn_id.to_string(),
        spawn_result.process_id,
        process_name,
        process_start_time,
        Some(spawn_result.terminal_type.clone()),
        spawn_result.terminal_window_id.clone(),
        command,
        now,
        None,
    )
}

/// Read the last 20 lines of scrollback from a daemon session (best-effort).
fn read_scrollback_tail(daemon_session_id: &str) -> String {
    match crate::daemon::client::read_scrollback(daemon_session_id) {
        Ok(Some(bytes)) => {
            let text = String::from_utf8_lossy(&bytes);
            let lines: Vec<&str> = text.lines().collect();
            let start = lines.len().saturating_sub(20);
            lines[start..].join("\n")
        }
        Ok(None) => {
            debug!(
                event = "core.session.scrollback_empty",
                daemon_session_id = daemon_session_id,
            );
            String::new()
        }
        Err(e) => {
            debug!(
                event = "core.session.scrollback_read_failed",
                daemon_session_id = daemon_session_id,
                error = %e,
            );
            String::new()
        }
    }
}

/// Capture process metadata from a terminal spawn result.
///
/// Attempts to get fresh process info from the OS for PID reuse protection.
/// Falls back to spawn result metadata if process info retrieval fails.
fn capture_process_metadata(
    spawn_result: &terminal::types::SpawnResult,
) -> (Option<String>, Option<u64>) {
    let Some(pid) = spawn_result.process_id else {
        return (
            spawn_result.process_name.clone(),
            spawn_result.process_start_time,
        );
    };

    match crate::process::get_process_info(pid) {
        Ok(info) => (Some(info.name), Some(info.start_time)),
        Err(e) => {
            warn!(
                event = "core.session.process_info_failed",
                pid = pid,
                error = %e,
                "Failed to get process metadata after spawn - using spawn result metadata"
            );
            (
                spawn_result.process_name.clone(),
                spawn_result.process_start_time,
            )
        }
    }
}
