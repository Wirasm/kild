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
    if !session.has_agents() {
        // Legacy session (pre-multi-agent) — attempt session-level PID file cleanup
        warn!(
            event = "core.session.pid_cleanup_no_agents",
            session_id = %session.id,
            operation = operation,
            "Session has no tracked agents, attempting session-level PID file cleanup"
        );
        let pid_file = get_pid_file_path(kild_dir, &session.id);
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
        return;
    }

    for agent_proc in session.agents() {
        // Determine PID file key: use spawn_id if available, otherwise fall back to session ID
        let pid_key = if agent_proc.spawn_id().is_empty() {
            session.id.to_string() // Backward compat: old sessions without spawn_id
        } else {
            agent_proc.spawn_id().to_string()
        };
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
