use tracing::{error, info, warn};

use crate::config::Config;
use crate::sessions::{errors::SessionError, persistence, types::*};

pub fn list_sessions() -> Result<Vec<Session>, SessionError> {
    info!(event = "core.session.list_started");

    let config = Config::new();
    let (sessions, skipped_count) = persistence::load_sessions_from_files(&config.sessions_dir())?;

    if skipped_count > 0 {
        tracing::warn!(
            event = "core.session.list_skipped_sessions",
            skipped_count = skipped_count,
            message = "Some session files were skipped due to errors"
        );
    }

    info!(
        event = "core.session.list_completed",
        count = sessions.len()
    );

    Ok(sessions)
}

pub fn get_session(name: &str) -> Result<Session, SessionError> {
    info!(event = "core.session.get_started", name = name);

    let config = Config::new();
    let session = persistence::load_session_from_file(name, &config.sessions_dir())?;

    info!(
        event = "core.session.get_completed",
        name = name,
        session_id = %session.id
    );

    Ok(session)
}

/// Sync a session's status with the daemon if it has a daemon-managed agent.
///
/// When a daemon PTY exits naturally (or the daemon crashes), the kild-core session
/// JSON still says Active. This function queries the daemon for the real status and
/// updates the session file if stale.
///
/// Returns `true` if the session was updated (status changed to Stopped).
/// This is a best-effort operation — daemon unreachable is treated as "stopped".
pub fn sync_daemon_session_status(session: &mut Session) -> bool {
    // Only sync Active sessions with daemon_session_id
    if session.status != SessionStatus::Active {
        return false;
    }

    let daemon_sid = match session.latest_agent().and_then(|a| a.daemon_session_id()) {
        Some(id) => id.to_string(),
        None => return false,
    };

    let status = match crate::daemon::client::get_session_status(&daemon_sid) {
        Ok(s) => s,
        Err(e) => {
            warn!(
                event = "core.session.daemon_status_sync_failed",
                session_id = %session.id,
                daemon_session_id = daemon_sid,
                error = %e,
                "Failed to query daemon for session status"
            );
            // Treat unexpected errors as "unknown" — don't sync to Stopped
            // since we can't confirm the session actually exited.
            return false;
        }
    };

    // If daemon reports Running, the session is still active — no sync needed.
    if status == Some(kild_protocol::SessionStatus::Running) {
        return false;
    }

    // Daemon reports "stopped", session not found, or daemon not running — mark as Stopped.
    info!(
        event = "core.session.daemon_status_sync",
        session_id = %session.id,
        daemon_session_id = daemon_sid,
        daemon_status = ?status,
        "Syncing stale session status to Stopped"
    );

    let now = chrono::Utc::now().to_rfc3339();

    // Update in-memory session for callers (list/status display)
    session.status = SessionStatus::Stopped;
    session.last_activity = Some(now.clone());

    // Patch status and last_activity via targeted JSON update to preserve unknown fields.
    // Using patch instead of full save prevents older binaries from dropping new fields
    // (e.g., installed kild binary dropping task_list_id added by a newer version).
    let config = Config::new();
    if let Err(e) = persistence::patch_session_json_fields(
        &config.sessions_dir(),
        &session.id,
        &[
            ("status", serde_json::json!("Stopped")),
            ("last_activity", serde_json::Value::String(now)),
        ],
    ) {
        error!(
            event = "core.session.daemon_status_sync_save_failed",
            session_id = %session.id,
            error = %e,
            "Failed to persist synced status"
        );
        eprintln!(
            "Warning: kild '{}' status is stale (daemon stopped but save failed: {}). Check disk space/permissions in ~/.kild/sessions/",
            session.branch, e
        );
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_list_sessions_empty() {
        // Create a temporary directory for this test
        let temp_dir = std::env::temp_dir().join("kild_test_empty_sessions");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        // Test with empty directory
        let (sessions, skipped) = persistence::load_sessions_from_files(&temp_dir).unwrap();
        assert_eq!(sessions.len(), 0);
        assert_eq!(skipped, 0);

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_sync_daemon_skips_stopped_sessions() {
        let mut session = Session::new(
            "test-project_sync-stopped".into(),
            "test-project".into(),
            "sync-stopped".into(),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Stopped,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            None,
            None,
            vec![],
            None,
            None,
            None,
        );

        let changed = sync_daemon_session_status(&mut session);
        assert!(!changed, "Stopped sessions should be skipped");
        assert_eq!(session.status, SessionStatus::Stopped);
    }

    #[test]
    fn test_sync_daemon_skips_sessions_without_daemon_session_id() {
        // Active session with a terminal-managed agent (no daemon_session_id)
        let agent = AgentProcess::new(
            "claude".to_string(),
            "test_0".to_string(),
            Some(12345),
            Some("claude-code".to_string()),
            Some(1234567890),
            None,
            None,
            "claude-code".to_string(),
            chrono::Utc::now().to_rfc3339(),
            None, // No daemon_session_id
        )
        .unwrap();

        let mut session = Session::new(
            "test-project_sync-terminal".into(),
            "test-project".into(),
            "sync-terminal".into(),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            None,
            None,
            vec![agent],
            None,
            None,
            None,
        );

        let changed = sync_daemon_session_status(&mut session);
        assert!(!changed, "Terminal-managed sessions should be skipped");
        assert_eq!(session.status, SessionStatus::Active);
    }

    #[test]
    fn test_sync_daemon_skips_active_session_without_agents() {
        // Active session with no agents at all (empty agents vec)
        let mut session = Session::new(
            "test-project_sync-no-agents".into(),
            "test-project".into(),
            "sync-no-agents".into(),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            None,
            None,
            vec![], // No agents
            None,
            None,
            None,
        );

        let changed = sync_daemon_session_status(&mut session);
        assert!(!changed, "Sessions with no agents should be skipped");
        assert_eq!(session.status, SessionStatus::Active);
    }
}
