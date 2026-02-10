use std::fs;
use std::path::{Path, PathBuf};

use tracing::{debug, warn};

use crate::errors::DaemonError;

/// Returns the default PID file path: `~/.kild/daemon.pid`.
pub fn pid_file_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".kild")
        .join("daemon.pid")
}

/// Write the current process PID to the PID file.
pub fn write_pid_file(path: &Path) -> Result<(), DaemonError> {
    let pid = std::process::id();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{}\n", pid))?;
    debug!(event = "daemon.pid.write_completed", pid = pid, path = %path.display());
    Ok(())
}

/// Read the PID from the PID file. Returns `None` if the file doesn't exist
/// or contains invalid content.
pub fn read_pid_file(path: &Path) -> Option<u32> {
    let content = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            warn!(
                event = "daemon.pid.read_failed",
                path = %path.display(),
                error = %e,
            );
            return None;
        }
    };
    match content.trim().parse::<u32>() {
        Ok(pid) => Some(pid),
        Err(_) => {
            warn!(
                event = "daemon.pid.parse_failed",
                path = %path.display(),
                content = %content.trim(),
            );
            None
        }
    }
}

/// Remove the PID file.
pub fn remove_pid_file(path: &Path) -> Result<(), DaemonError> {
    match fs::remove_file(path) {
        Ok(()) => {
            debug!(event = "daemon.pid.remove_completed", path = %path.display());
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(DaemonError::Io(e)),
    }
}

/// Check whether a process with the given PID is alive.
///
/// Uses `kill(pid, 0)` which checks existence without sending a signal.
pub fn is_process_alive(pid: u32) -> bool {
    use nix::sys::signal;
    use nix::unistd::Pid;

    match signal::kill(Pid::from_raw(pid as i32), None) {
        Ok(()) => true,
        Err(nix::errno::Errno::ESRCH) => false,
        // EPERM means process exists but we lack permission — still alive
        Err(nix::errno::Errno::EPERM) => true,
        Err(_) => false,
    }
}

/// Check if the daemon is running by reading the PID file and verifying the process.
///
/// Returns `Some(pid)` if a daemon is running, `None` otherwise.
/// If the PID file exists but the process is dead, the stale PID file is removed.
pub fn check_daemon_running(pid_path: &Path) -> Option<u32> {
    let pid = read_pid_file(pid_path)?;

    if is_process_alive(pid) {
        Some(pid)
    } else {
        warn!(
            event = "daemon.pid.stale_detected",
            pid = pid,
            path = %pid_path.display(),
        );
        if let Err(e) = remove_pid_file(pid_path) {
            warn!(
                event = "daemon.pid.stale_remove_failed",
                pid = pid,
                path = %pid_path.display(),
                error = %e,
            );
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_and_read_pid_file() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("daemon.pid");

        write_pid_file(&pid_path).unwrap();

        let pid = read_pid_file(&pid_path);
        assert!(pid.is_some());
        assert_eq!(pid.unwrap(), std::process::id());
    }

    #[test]
    fn test_read_nonexistent_pid_file() {
        let path = Path::new("/tmp/kild_test_nonexistent_pid_file.pid");
        assert!(read_pid_file(path).is_none());
    }

    #[test]
    fn test_read_corrupt_pid_file() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("daemon.pid");
        fs::write(&pid_path, "not_a_number\n").unwrap();

        assert!(read_pid_file(&pid_path).is_none());
    }

    #[test]
    fn test_remove_pid_file() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("daemon.pid");
        fs::write(&pid_path, "12345\n").unwrap();

        remove_pid_file(&pid_path).unwrap();
        assert!(!pid_path.exists());
    }

    #[test]
    fn test_remove_nonexistent_pid_file() {
        let path = Path::new("/tmp/kild_test_remove_nonexistent.pid");
        // Should not error on missing file
        remove_pid_file(path).unwrap();
    }

    #[test]
    fn test_is_process_alive_current() {
        // Current process should be alive
        assert!(is_process_alive(std::process::id()));
    }

    #[test]
    fn test_is_process_alive_dead() {
        // PID 0 is the kernel's idle process, and PID 4294967 is unlikely to exist
        // Use an unlikely high PID
        assert!(!is_process_alive(4_294_967));
    }

    #[test]
    fn test_check_daemon_running_current_process() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("daemon.pid");

        // Write current PID — should be detected as running
        write_pid_file(&pid_path).unwrap();
        let result = check_daemon_running(&pid_path);
        assert_eq!(result, Some(std::process::id()));
    }

    #[test]
    fn test_check_daemon_running_stale() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("daemon.pid");

        // Write a dead PID
        fs::write(&pid_path, "4294967\n").unwrap();
        let result = check_daemon_running(&pid_path);
        assert!(result.is_none());
        // Stale PID file should be cleaned up
        assert!(!pid_path.exists());
    }

    #[test]
    fn test_check_daemon_running_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("daemon.pid");
        let result = check_daemon_running(&pid_path);
        assert!(result.is_none());
    }
}
