//! PID file management for reliable process tracking
//!
//! On macOS, sysinfo cannot read command line arguments or working directories
//! for other processes. This module provides PID file-based tracking as a reliable
//! alternative for identifying which process belongs to which shard.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::debug;

use crate::process::errors::ProcessError;

/// Directory name for storing PID files within the shards directory
const PID_DIR_NAME: &str = "pids";

/// Get the PID file path for a given session ID
///
/// PID files are stored at `~/.shards/pids/<session_id>.pid`
pub fn get_pid_file_path(shards_dir: &Path, session_id: &str) -> PathBuf {
    // Sanitize session_id to be safe for filenames (replace / with -)
    let safe_id = session_id.replace('/', "-");
    shards_dir
        .join(PID_DIR_NAME)
        .join(format!("{}.pid", safe_id))
}

/// Ensure the PID directory exists
pub fn ensure_pid_dir(shards_dir: &Path) -> Result<PathBuf, ProcessError> {
    let pid_dir = shards_dir.join(PID_DIR_NAME);
    if !pid_dir.exists() {
        fs::create_dir_all(&pid_dir).map_err(|e| ProcessError::PidFileError {
            path: pid_dir.clone(),
            message: format!("Failed to create PID directory: {}", e),
        })?;
        debug!(event = "pid_file.dir_created", path = %pid_dir.display());
    }
    Ok(pid_dir)
}

/// Read PID from a PID file with retry logic
///
/// The PID file may not exist immediately after spawning, so we retry
/// with exponential backoff.
pub fn read_pid_file_with_retry(
    pid_file: &Path,
    max_attempts: u32,
    initial_delay_ms: u64,
) -> Result<Option<u32>, ProcessError> {
    let mut delay = Duration::from_millis(initial_delay_ms);

    for attempt in 1..=max_attempts {
        debug!(
            event = "pid_file.read_attempt",
            attempt,
            max_attempts,
            path = %pid_file.display()
        );

        match read_pid_file(pid_file) {
            Ok(Some(pid)) => {
                debug!(
                    event = "pid_file.read_success",
                    attempt,
                    pid,
                    path = %pid_file.display()
                );
                return Ok(Some(pid));
            }
            Ok(None) => {
                // File doesn't exist yet, wait and retry
                if attempt < max_attempts {
                    debug!(
                        event = "pid_file.not_found_retry",
                        attempt,
                        next_delay_ms = delay.as_millis()
                    );
                    std::thread::sleep(delay);
                    delay = std::cmp::min(delay * 2, Duration::from_secs(8));
                }
            }
            Err(e) => {
                debug!(
                    event = "pid_file.read_error",
                    attempt,
                    error = %e
                );
                if attempt == max_attempts {
                    return Err(e);
                }
                std::thread::sleep(delay);
                delay = std::cmp::min(delay * 2, Duration::from_secs(8));
            }
        }
    }

    debug!(
        event = "pid_file.not_found_final",
        max_attempts,
        path = %pid_file.display()
    );
    Ok(None)
}

/// Read PID from a PID file (single attempt)
fn read_pid_file(pid_file: &Path) -> Result<Option<u32>, ProcessError> {
    if !pid_file.exists() {
        return Ok(None);
    }

    let mut file = fs::File::open(pid_file).map_err(|e| ProcessError::PidFileError {
        path: pid_file.to_path_buf(),
        message: format!("Failed to open PID file: {}", e),
    })?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| ProcessError::PidFileError {
            path: pid_file.to_path_buf(),
            message: format!("Failed to read PID file: {}", e),
        })?;

    let pid_str = contents.trim();
    if pid_str.is_empty() {
        return Ok(None);
    }

    let pid = pid_str
        .parse::<u32>()
        .map_err(|e| ProcessError::PidFileError {
            path: pid_file.to_path_buf(),
            message: format!("Invalid PID '{}': {}", pid_str, e),
        })?;

    Ok(Some(pid))
}

/// Delete a PID file
pub fn delete_pid_file(pid_file: &Path) -> Result<(), ProcessError> {
    if pid_file.exists() {
        fs::remove_file(pid_file).map_err(|e| ProcessError::PidFileError {
            path: pid_file.to_path_buf(),
            message: format!("Failed to delete PID file: {}", e),
        })?;
        debug!(event = "pid_file.deleted", path = %pid_file.display());
    }
    Ok(())
}

/// Generate a spawn command that captures the PID to a file
///
/// Uses the `exec` trick: write the shell's PID, then `exec` replaces
/// the shell with the target command, keeping the same PID.
///
/// # Example output
/// ```text
/// sh -c 'echo $$ > /path/to/pid && exec claude'
/// ```
pub fn wrap_command_with_pid_capture(command: &str, pid_file: &Path) -> String {
    // Escape single quotes in the command for safe shell embedding
    let escaped_command = command.replace('\'', "'\\''");
    let escaped_path = pid_file.to_string_lossy().replace('\'', "'\\''");

    format!(
        "sh -c 'echo $$ > '\\''{}'\\'' && exec {}'",
        escaped_path, escaped_command
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_get_pid_file_path() {
        let shards_dir = Path::new("/home/user/.shards");

        // Simple session ID
        let path = get_pid_file_path(shards_dir, "abc123");
        assert_eq!(path, PathBuf::from("/home/user/.shards/pids/abc123.pid"));

        // Session ID with slash (project/branch format)
        let path = get_pid_file_path(shards_dir, "project-id/feature-branch");
        assert_eq!(
            path,
            PathBuf::from("/home/user/.shards/pids/project-id-feature-branch.pid")
        );
    }

    #[test]
    fn test_ensure_pid_dir() {
        let temp_dir = TempDir::new().unwrap();
        let shards_dir = temp_dir.path();

        let pid_dir = ensure_pid_dir(shards_dir).unwrap();
        assert!(pid_dir.exists());
        assert_eq!(pid_dir, shards_dir.join("pids"));
    }

    #[test]
    fn test_read_pid_file_success() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("test.pid");

        // Write a valid PID
        let mut file = fs::File::create(&pid_file).unwrap();
        writeln!(file, "12345").unwrap();

        let result = read_pid_file(&pid_file).unwrap();
        assert_eq!(result, Some(12345));
    }

    #[test]
    fn test_read_pid_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("nonexistent.pid");

        let result = read_pid_file(&pid_file).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_read_pid_file_empty() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("empty.pid");

        fs::File::create(&pid_file).unwrap();

        let result = read_pid_file(&pid_file).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_read_pid_file_invalid() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("invalid.pid");

        let mut file = fs::File::create(&pid_file).unwrap();
        writeln!(file, "not-a-number").unwrap();

        let result = read_pid_file(&pid_file);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_pid_file_with_whitespace() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("whitespace.pid");

        let mut file = fs::File::create(&pid_file).unwrap();
        writeln!(file, "  42  \n").unwrap();

        let result = read_pid_file(&pid_file).unwrap();
        assert_eq!(result, Some(42));
    }

    #[test]
    fn test_delete_pid_file() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("to_delete.pid");

        fs::File::create(&pid_file).unwrap();
        assert!(pid_file.exists());

        delete_pid_file(&pid_file).unwrap();
        assert!(!pid_file.exists());
    }

    #[test]
    fn test_delete_pid_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("nonexistent.pid");

        // Should not error if file doesn't exist
        let result = delete_pid_file(&pid_file);
        assert!(result.is_ok());
    }

    #[test]
    fn test_wrap_command_with_pid_capture() {
        let pid_file = Path::new("/tmp/test.pid");

        let wrapped = wrap_command_with_pid_capture("claude", pid_file);
        assert!(wrapped.contains("echo $$"));
        assert!(wrapped.contains("/tmp/test.pid"));
        assert!(wrapped.contains("exec claude"));
    }

    #[test]
    fn test_wrap_command_with_pid_capture_special_chars() {
        let pid_file = Path::new("/tmp/test's file.pid");

        let wrapped = wrap_command_with_pid_capture("echo 'hello'", pid_file);
        // Should properly escape single quotes
        assert!(wrapped.contains("exec echo"));
    }

    #[test]
    fn test_read_pid_file_with_retry_immediate_success() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("immediate.pid");

        // Write PID before calling
        let mut file = fs::File::create(&pid_file).unwrap();
        writeln!(file, "99999").unwrap();

        let result = read_pid_file_with_retry(&pid_file, 3, 10).unwrap();
        assert_eq!(result, Some(99999));
    }
}
