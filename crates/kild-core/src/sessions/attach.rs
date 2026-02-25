use std::path::Path;

use tracing::{info, warn};

use crate::sessions::errors::SessionError;
use crate::terminal;
use crate::terminal::types::TerminalType;
use kild_config::KildConfig;

/// Spawn a terminal attach window for a daemon session (best-effort).
///
/// After a daemon PTY is created, this spawns a terminal window running
/// `kild attach <branch>` so the CLI user gets immediate visual feedback.
/// The terminal backend is selected via user config or auto-detection
/// (Ghostty > iTerm > Terminal.app on macOS).
/// The attach process is ephemeral — Ctrl+C detaches without killing the agent.
///
/// Returns `Some((terminal_type, window_id))` on success for storage in
/// `AgentProcess`, enabling cleanup during destroy. Returns `None` on failure.
/// Failures are logged as warnings but never block session creation.
fn spawn_attach_window(
    branch: &str,
    spawn_id: &str,
    worktree_path: &Path,
    kild_config: &KildConfig,
) -> Option<(TerminalType, String)> {
    info!(event = "core.session.auto_attach_started", branch = branch);

    let kild_binary = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            warn!(
                event = "core.session.auto_attach_failed",
                branch = branch,
                error = %e,
                "Could not resolve kild binary path for auto-attach"
            );
            eprintln!("Warning: Could not auto-attach to daemon session: {}", e);
            eprintln!("         Use `kild attach {}` to connect manually.", branch);
            return None;
        }
    };

    let attach_command = format!("{} attach '{}'", kild_binary.display(), branch);

    // Pass None for kild_dir to skip PID file creation — the attach process is ephemeral
    match terminal::handler::spawn_terminal(
        worktree_path,
        &attach_command,
        kild_config,
        Some(spawn_id),
        None,
    ) {
        Ok(result) => {
            info!(
                event = "core.session.auto_attach_completed",
                branch = branch,
                window_id = ?result.terminal_window_id
            );
            result
                .terminal_window_id
                .map(|wid| (result.terminal_type, wid))
        }
        Err(e) => {
            warn!(
                event = "core.session.auto_attach_failed",
                branch = branch,
                error = %e,
                "Could not spawn attach window for daemon session"
            );
            eprintln!("Warning: Could not auto-attach to daemon session: {}", e);
            eprintln!("         Use `kild attach {}` to connect manually.", branch);
            None
        }
    }
}

/// Spawn attach window and update session with terminal info.
///
/// Only runs for daemon sessions. Calls `spawn_attach_window()` and updates
/// the session's latest agent with terminal type and window ID. Saves the
/// updated session to disk.
///
/// Returns `Ok(true)` on success, `Ok(false)` if attach window fails (best-effort).
/// Returns `Err` only if session save fails.
pub fn spawn_and_save_attach_window(
    session: &mut super::types::Session,
    branch: &str,
    kild_config: &KildConfig,
    sessions_dir: &Path,
) -> Result<bool, SessionError> {
    use super::persistence;

    let attach_spawn_id = session
        .latest_agent()
        .map(|a| a.spawn_id().to_string())
        .unwrap_or_default();

    let Some((tt, wid)) = spawn_attach_window(
        branch,
        &attach_spawn_id,
        &session.worktree_path,
        kild_config,
    ) else {
        return Ok(false);
    };

    let Some(agent) = session.latest_agent_mut() else {
        return Ok(false);
    };

    agent.set_attach_info(tt, wid);
    persistence::save_session_to_file(session, sessions_dir)?;
    Ok(true)
}
