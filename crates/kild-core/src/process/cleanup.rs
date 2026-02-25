use std::path::Path;
use tracing::{debug, warn};

use crate::process::{delete_pid_file, get_pid_file_path};
use crate::sessions::types::Session;

/// Clean up PID files for a session (best-effort).
///
/// Handles both multi-agent sessions (per-agent spawn ID PID files) and
/// legacy sessions (session-level PID file). Failures are logged at debug
/// level since PID file cleanup is best-effort.
pub(crate) fn cleanup_session_pid_files(session: &Session, kild_dir: &Path, operation: &str) {
    // Collect PID file keys to clean up.
    // Legacy sessions (pre-multi-agent) have no tracked agents — fall back to the session ID.
    let pid_keys: Vec<String> = if session.has_agents() {
        session
            .agents()
            .iter()
            .map(|agent| {
                // Use spawn_id if present; fall back to session ID for old sessions without one
                if agent.spawn_id().is_empty() {
                    session.id.to_string()
                } else {
                    agent.spawn_id().to_string()
                }
            })
            .collect()
    } else {
        warn!(
            event = "core.session.pid_cleanup_no_agents",
            session_id = %session.id,
            operation = operation,
            "Session has no tracked agents, attempting session-level PID file cleanup"
        );
        vec![session.id.to_string()]
    };

    for pid_key in pid_keys {
        let pid_file = get_pid_file_path(kild_dir, &pid_key);
        match delete_pid_file(&pid_file) {
            Ok(()) => {
                debug!(
                    event = "core.session.pid_file_cleaned",
                    session_id = %session.id,
                    operation = operation,
                    pid_file = %pid_file.display()
                );
            }
            Err(e) => {
                debug!(
                    event = "core.session.pid_file_cleanup_failed",
                    session_id = %session.id,
                    operation = operation,
                    pid_file = %pid_file.display(),
                    error = %e
                );
            }
        }
    }
}
