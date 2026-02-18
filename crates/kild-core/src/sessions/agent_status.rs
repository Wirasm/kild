use tracing::info;

use crate::config::Config;
use crate::sessions::{errors::SessionError, persistence, types::*};

/// Result of a successful agent status update.
pub struct AgentStatusResult {
    pub branch: String,
    pub status: super::types::AgentStatus,
    pub updated_at: String,
}

/// Update agent status for a session via sidecar file.
///
/// Writes `updated_at` to the status sidecar; the health system reads this directly.
pub fn update_agent_status(
    name: &str,
    status: super::types::AgentStatus,
    notify: bool,
) -> Result<AgentStatusResult, SessionError> {
    info!(
        event = "core.session.agent_status_update_started",
        name = name,
        status = %status,
    );
    let config = Config::new();
    let session =
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

    // last_activity is tracked via the sidecar's updated_at (written above).
    // The health system reads agent_status_updated_at from the sidecar directly.
    // Only lifecycle events (open, stop, daemon sync) update last_activity in kild.json.

    info!(
        event = "core.session.agent_status_update_completed",
        session_id = %session.id,
        status = %status,
    );

    if crate::notify::should_notify(notify, status) {
        info!(
            event = "core.session.agent_status_notify_triggered",
            branch = %session.branch,
            status = %status,
        );
        let message =
            crate::notify::format_notification_message(&session.agent, &session.branch, status);
        crate::notify::send_notification("KILD", &message);
    }

    Ok(AgentStatusResult {
        branch: session.branch.to_string(),
        status,
        updated_at: now,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worktree_path_match_exact() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_dir = tmp.path().join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let worktree = tmp.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();

        let mut session = Session::new_for_test("feat".to_string(), worktree.clone());
        session.worktree_path = worktree.clone();
        persistence::save_session_to_file(&session, &sessions_dir).unwrap();

        let (sessions, _) = persistence::load_sessions_from_files(&sessions_dir).unwrap();
        let found = sessions
            .iter()
            .find(|s| worktree.starts_with(&s.worktree_path));
        assert!(found.is_some());
        assert_eq!(&*found.unwrap().branch, "feat");
    }

    #[test]
    fn test_worktree_path_match_subdirectory() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_dir = tmp.path().join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let worktree = tmp.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();

        let mut session = Session::new_for_test("feat".to_string(), worktree.clone());
        session.worktree_path = worktree.clone();
        persistence::save_session_to_file(&session, &sessions_dir).unwrap();

        let (sessions, _) = persistence::load_sessions_from_files(&sessions_dir).unwrap();

        let subdir = worktree.join("src").join("main.rs");
        let found = sessions
            .iter()
            .find(|s| subdir.starts_with(&s.worktree_path));
        assert!(found.is_some());
        assert_eq!(&*found.unwrap().branch, "feat");
    }

    #[test]
    fn test_worktree_path_no_match() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_dir = tmp.path().join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let worktree = tmp.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();

        let mut session = Session::new_for_test("feat".to_string(), worktree.clone());
        session.worktree_path = worktree;
        persistence::save_session_to_file(&session, &sessions_dir).unwrap();

        let (sessions, _) = persistence::load_sessions_from_files(&sessions_dir).unwrap();

        let other_path = tmp.path().join("other_project");
        let found = sessions
            .iter()
            .find(|s| other_path.starts_with(&s.worktree_path));
        assert!(found.is_none());
    }
}
