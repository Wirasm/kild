//! PID file management for reliable process tracking
//!
//! On macOS, sysinfo cannot read command line arguments or working directories
//! for other processes. This module provides PID file-based tracking as a reliable
//! alternative for identifying which process belongs to which kild.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, warn};

use crate::process::errors::ProcessError;

/// Directory name for storing PID files within the kild directory
const PID_DIR_NAME: &str = "pids";

/// Get the PID file path for a given session ID
///
/// PID files are stored at `~/.kild/pids/<session_id>.pid`
pub fn get_pid_file_path(kild_dir: &Path, session_id: &str) -> PathBuf {
    // Sanitize session_id to be safe for filenames (replace / with -)
    let safe_id = session_id.replace('/', "-");
    kild_dir.join(PID_DIR_NAME).join(format!("{}.pid", safe_id))
}

/// Ensure the PID directory exists
pub fn ensure_pid_dir(kild_dir: &Path) -> Result<PathBuf, ProcessError> {
    let pid_dir = kild_dir.join(PID_DIR_NAME);
    if !pid_dir.exists() {
        fs::create_dir_all(&pid_dir).map_err(|e| ProcessError::PidFileError {
            path: pid_dir.clone(),
            message: format!("Failed to create PID directory: {}", e),
        })?;
        debug!(event = "core.pid_file.dir_created", path = %pid_dir.display());
    }
    Ok(pid_dir)
}

/// Read PID from a PID file with fast polling.
///
/// The PID file is written by `echo $$ > file && exec cmd` before the
/// agent process starts, so it typically appears within milliseconds.
/// Polls at ~100ms intervals (with +/-20% PID-based jitter to decorrelate
/// simultaneous `kild create` launches) with a 3s timeout.
pub fn read_pid_file_with_retry(pid_file: &Path) -> Result<Option<u32>, ProcessError> {
    const BASE_INTERVAL_MS: u64 = 100;
    const MAX_WAIT: Duration = Duration::from_secs(3);

    // Compute jitter once — deterministic per-process, varies across concurrent launches
    let jitter_range = BASE_INTERVAL_MS / 5; // 20ms
    let pid_jitter = (std::process::id() as u64) % (jitter_range * 2 + 1);
    let jitter = pid_jitter as i64 - jitter_range as i64;
    // poll_interval is in [80, 120] ms — cannot underflow
    let poll_interval = Duration::from_millis((BASE_INTERVAL_MS as i64 + jitter) as u64);

    let start = std::time::Instant::now();
    let mut last_error: Option<ProcessError> = None;

    while start.elapsed() <= MAX_WAIT {
        match read_pid_file(pid_file) {
            Ok(Some(pid)) => {
                debug!(
                    event = "core.pid_file.read_success",
                    pid,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    path = %pid_file.display()
                );
                return Ok(Some(pid));
            }
            Ok(None) => {
                // File doesn't exist yet, continue polling
            }
            Err(e) => {
                last_error = Some(e);
            }
        }

        if let Some(ref error) = last_error {
            debug!(
                event = "core.pid_file.polling_with_error",
                elapsed_ms = start.elapsed().as_millis() as u64,
                error = %error,
                path = %pid_file.display()
            );
        } else {
            debug!(
                event = "core.pid_file.polling",
                elapsed_ms = start.elapsed().as_millis() as u64,
                path = %pid_file.display()
            );
        }

        std::thread::sleep(poll_interval);
    }

    // Timeout reached — surface errors encountered during polling
    if let Some(error) = last_error {
        warn!(
            event = "core.pid_file.timeout_with_errors",
            elapsed_ms = start.elapsed().as_millis() as u64,
            error = %error,
            path = %pid_file.display(),
            message = "PID file polling timed out after encountering errors"
        );
        return Err(error);
    }

    debug!(
        event = "core.pid_file.not_found_timeout",
        elapsed_ms = start.elapsed().as_millis() as u64,
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
        debug!(event = "core.pid_file.deleted", path = %pid_file.display());
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
        let kild_dir = Path::new("/home/user/.kild");

        // Simple session ID
        let path = get_pid_file_path(kild_dir, "abc123");
        assert_eq!(path, PathBuf::from("/home/user/.kild/pids/abc123.pid"));

        // Session ID with slash (project/branch format)
        let path = get_pid_file_path(kild_dir, "project-id/feature-branch");
        assert_eq!(
            path,
            PathBuf::from("/home/user/.kild/pids/project-id-feature-branch.pid")
        );
    }

    #[test]
    fn test_ensure_pid_dir() {
        let temp_dir = TempDir::new().unwrap();
        let kild_dir = temp_dir.path();

        let pid_dir = ensure_pid_dir(kild_dir).unwrap();
        assert!(pid_dir.exists());
        assert_eq!(pid_dir, kild_dir.join("pids"));
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

        let result = read_pid_file_with_retry(&pid_file).unwrap();
        assert_eq!(result, Some(99999));
    }

    #[test]
    fn test_read_pid_file_with_retry_not_found_times_out() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("never_created.pid");

        let start = std::time::Instant::now();
        let result = read_pid_file_with_retry(&pid_file).unwrap();
        let elapsed_ms = start.elapsed().as_millis();

        assert_eq!(result, None);
        // Should timeout at ~3000ms. Allow 500ms tolerance for CI variance.
        assert!(
            elapsed_ms >= 3000 && elapsed_ms < 3500,
            "Should timeout at ~3s (3000-3500ms), took {}ms",
            elapsed_ms
        );
    }

    #[test]
    fn test_read_pid_file_with_retry_success_after_delay() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("delayed.pid");

        // Simulate PID file written after 250ms (2-3 poll iterations)
        let pid_file_clone = pid_file.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            let mut file = fs::File::create(&pid_file_clone).unwrap();
            writeln!(file, "77777").unwrap();
        });

        let start = std::time::Instant::now();
        let result = read_pid_file_with_retry(&pid_file).unwrap();
        let elapsed_ms = start.elapsed().as_millis();

        assert_eq!(result, Some(77777));
        assert!(
            elapsed_ms >= 250 && elapsed_ms < 1000,
            "Should find PID within 1s after 250ms delay, took {}ms",
            elapsed_ms
        );
    }

    #[test]
    fn test_read_pid_file_with_retry_persistent_error_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let pid_file = temp_dir.path().join("always_invalid.pid");

        // Create file with invalid content that persists
        let mut file = fs::File::create(&pid_file).unwrap();
        writeln!(file, "not-a-number").unwrap();

        let start = std::time::Instant::now();
        let result = read_pid_file_with_retry(&pid_file);
        let elapsed = start.elapsed();

        assert!(
            result.is_err(),
            "Should return error for persistent invalid PID"
        );
        assert!(
            elapsed.as_secs() >= 3 && elapsed.as_secs() < 5,
            "Should timeout after ~3s, took {:?}",
            elapsed
        );
    }
}
