use std::path::Path;
use tracing::debug;

use crate::process::{delete_pid_file, get_pid_file_path};

/// Clean up PID files by key (best-effort).
///
/// Each key maps to a PID file via [`get_pid_file_path`]. Callers are responsible
/// for resolving which keys to clean up (e.g. spawn IDs for multi-agent sessions,
/// session IDs for legacy sessions). Failures are logged at debug level since
/// PID file cleanup is best-effort.
pub(crate) fn cleanup_pid_files(pid_keys: &[String], kild_dir: &Path, operation: &str) {
    for pid_key in pid_keys {
        let pid_file = get_pid_file_path(kild_dir, pid_key);
        match delete_pid_file(&pid_file) {
            Ok(()) => {
                debug!(
                    event = "core.process.pid_file_cleaned",
                    pid_key = pid_key,
                    operation = operation,
                    pid_file = %pid_file.display()
                );
            }
            Err(e) => {
                debug!(
                    event = "core.process.pid_file_cleanup_failed",
                    pid_key = pid_key,
                    operation = operation,
                    pid_file = %pid_file.display(),
                    error = %e
                );
            }
        }
    }
}
