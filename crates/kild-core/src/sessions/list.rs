use tracing::{error, info, warn};

use crate::daemon::client::DaemonClientError;
use crate::sessions::{errors::SessionError, persistence, types::*};
use kild_config::Config;

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
/// Returns `true` if the session status was changed to `Stopped`.
/// Returns `false` if the session was not modified:
///   - Session is already stopped or has no daemon session ID
///   - Daemon reports the session is still running
///   - Daemon returned an unexpected structured error (skip to avoid false positives)
///
/// Daemon unreachable (connection failed, broken pipe, empty response) is treated
/// as "daemon down" and causes the session to be marked `Stopped`.
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
            if !is_daemon_unreachable(&e) {
                // Daemon sent a structured error (DaemonError) — it's alive but
                // returned something unexpected. Don't sync to avoid false positives.
                warn!(
                    event = "core.session.daemon_status_sync_failed",
                    session_id = %session.id,
                    daemon_session_id = daemon_sid,
                    error = %e,
                    "Daemon returned unexpected error, skipping sync"
                );
                return false;
            }
            // Connection failed, broken pipe, empty response, etc. — daemon is
            // not running or died mid-request. Treat as daemon down → sync to Stopped.
            warn!(
                event = "core.session.daemon_status_sync_unreachable",
                session_id = %session.id,
                daemon_session_id = daemon_sid,
                error = %e,
                "Daemon unreachable — marking session as stopped"
            );
            None
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

/// Check if a daemon client error indicates the daemon is unreachable.
///
/// Connection failures, IO errors, and protocol errors (e.g., empty response from
/// a dying daemon) all indicate the daemon process is not functional. Only
/// `DaemonError` (a structured error response) proves the daemon is alive.
///
/// Note: `get_session_status()` converts `NotRunning` into `Ok(None)` before
/// returning, so `NotRunning` cannot reach this helper in practice.
fn is_daemon_unreachable(e: &DaemonClientError) -> bool {
    !matches!(e, DaemonClientError::DaemonError { .. })
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

    #[test]
    fn test_is_daemon_unreachable_connection_failed() {
        let err = DaemonClientError::ConnectionFailed {
            message: "connection reset".to_string(),
        };
        assert!(
            is_daemon_unreachable(&err),
            "ConnectionFailed should be treated as unreachable"
        );
    }

    #[test]
    fn test_is_daemon_unreachable_io_error() {
        let err = DaemonClientError::Io(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "broken",
        ));
        assert!(
            is_daemon_unreachable(&err),
            "IO errors should be treated as unreachable"
        );
    }

    #[test]
    fn test_is_daemon_unreachable_protocol_error() {
        let err = DaemonClientError::ProtocolError {
            message: "Empty response from daemon".to_string(),
        };
        assert!(
            is_daemon_unreachable(&err),
            "ProtocolError (empty response from dying daemon) should be treated as unreachable"
        );
    }

    #[test]
    fn test_is_daemon_unreachable_not_running() {
        // NotRunning cannot reach is_daemon_unreachable in practice (absorbed by
        // get_session_status into Ok(None)), but the helper classifies it correctly.
        let err = DaemonClientError::NotRunning {
            path: "/tmp/test.sock".to_string(),
        };
        assert!(
            is_daemon_unreachable(&err),
            "NotRunning should be treated as unreachable"
        );
    }

    #[test]
    fn test_is_daemon_unreachable_daemon_error_is_reachable() {
        let err = DaemonClientError::DaemonError {
            code: kild_protocol::ErrorCode::SessionNotFound,
            message: "not found".to_string(),
        };
        assert!(
            !is_daemon_unreachable(&err),
            "DaemonError means daemon is alive — should NOT be treated as unreachable"
        );
    }

    #[test]
    fn test_sync_daemon_marks_stopped_when_daemon_not_running() {
        // Active session with a daemon_session_id pointing to a nonexistent daemon.
        // Tests run without a daemon, so get_session_status returns Ok(None)
        // (NotRunning early-exit path), which should trigger the Stopped transition.
        let agent = AgentProcess::new(
            "claude".to_string(),
            "proj_stale_0".to_string(),
            None,
            None,
            None,
            None,
            None,
            "claude-code".to_string(),
            chrono::Utc::now().to_rfc3339(),
            Some("proj_stale_0".to_string()), // daemon_session_id set
        )
        .unwrap();

        let mut session = Session::new(
            "test-project_stale-daemon".into(),
            "test-project".into(),
            "stale-daemon".into(),
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

        assert_eq!(session.status, SessionStatus::Active);

        // With no daemon running, get_session_status returns Ok(None),
        // and sync should flip the in-memory status to Stopped.
        // The file persist will fail (no session file on disk) but the
        // in-memory mutation is the behavior under test.
        let changed = sync_daemon_session_status(&mut session);

        assert!(changed, "should return true when status changes");
        assert_eq!(
            session.status,
            SessionStatus::Stopped,
            "session must flip to Stopped when daemon is not running"
        );
    }
}
