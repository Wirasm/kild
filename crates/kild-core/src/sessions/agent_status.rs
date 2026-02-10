use tracing::info;

use crate::config::Config;
use crate::sessions::{errors::SessionError, persistence, types::*};

/// Update agent status for a session via sidecar file.
///
/// Also updates `last_activity` on the session JSON to feed the health monitoring system.
pub fn update_agent_status(
    name: &str,
    status: super::types::AgentStatus,
    notify: bool,
) -> Result<(), SessionError> {
    info!(
        event = "core.session.agent_status_update_started",
        name = name,
        status = %status,
    );
    let config = Config::new();
    let mut session =
        persistence::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    // Write sidecar file with current timestamp
    let now = chrono::Utc::now().to_rfc3339();
    let status_info = super::types::AgentStatusInfo {
        status,
        updated_at: now.clone(),
    };
    persistence::write_agent_status(&config.sessions_dir(), &session.id, &status_info)?;

    // Update last_activity on the session (heartbeat)
    session.last_activity = Some(now);
    persistence::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "core.session.agent_status_update_completed",
        session_id = session.id,
        status = %status,
    );

    if notify && matches!(status, AgentStatus::Waiting | AgentStatus::Error) {
        info!(
            event = "core.session.agent_status_notify_triggered",
            branch = session.branch,
            status = %status,
        );
        let message = format!(
            "Agent {} in {} needs input ({})",
            session.agent, session.branch, status
        );
        crate::notify::send_notification("KILD", &message);
    }

    Ok(())
}

/// Read agent status for a session from the sidecar file.
///
/// Returns `None` if no status has been reported yet.
pub fn read_agent_status(session_id: &str) -> Option<super::types::AgentStatusInfo> {
    let config = Config::new();
    persistence::read_agent_status(&config.sessions_dir(), session_id)
}

/// Resolve session from a worktree path (for --self flag).
///
/// Matches if the given path equals or is a subdirectory of a session's worktree path.
pub fn find_session_by_worktree_path(
    worktree_path: &std::path::Path,
) -> Result<Option<Session>, SessionError> {
    let config = Config::new();
    let (sessions, _) = persistence::load_sessions_from_files(&config.sessions_dir())?;

    Ok(sessions
        .into_iter()
        .find(|session| worktree_path.starts_with(&session.worktree_path)))
}
