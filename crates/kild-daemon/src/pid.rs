use std::fs;
use std::path::{Path, PathBuf};

use kild_paths::KildPaths;
use tracing::{debug, warn};

use crate::errors::DaemonError;

/// Returns the default PID file path: `~/.kild/daemon.pid`.
pub fn pid_file_path() -> PathBuf {
    KildPaths::resolve()
        .unwrap_or_else(|e| {
            warn!(
                event = "daemon.pid.home_dir_fallback",
                error = %e,
                fallback = "/tmp/.kild",
            );
            KildPaths::from_dir(PathBuf::from("/tmp/.kild"))
        })
        .daemon_pid_file()
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

/// Write the current binary's path and mtime to the bin file.
///
/// Stores two lines: the canonical binary path and its mtime as epoch seconds.
/// Used to detect stale daemons after binary upgrades.
pub fn write_bin_file(path: &Path) -> Result<(), DaemonError> {
    let exe = std::env::current_exe().map_err(|e| {
        DaemonError::Io(std::io::Error::other(format!(
            "cannot determine current binary path: {e}"
        )))
    })?;

    let canonical = exe.canonicalize().unwrap_or(exe);

    let mtime = fs::metadata(&canonical)
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        })
        .unwrap_or(0);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{}\n{}\n", canonical.display(), mtime))?;
    debug!(
        event = "daemon.bin.write_completed",
        path = %canonical.display(),
        mtime = mtime,
    );
    Ok(())
}

/// Read the binary path and mtime from the bin file.
///
/// Returns `(path, mtime_epoch_secs)` or `None` if the file is missing or malformed.
pub fn read_bin_file(path: &Path) -> Option<(PathBuf, u64)> {
    let content = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            warn!(
                event = "daemon.bin.read_failed",
                path = %path.display(),
                error = %e,
            );
            return None;
        }
    };

    let mut lines = content.lines();
    let bin_path = lines.next()?;
    let mtime_str = lines.next()?;
    let mtime = mtime_str.parse::<u64>().ok()?;

    Some((PathBuf::from(bin_path), mtime))
}

/// Remove the bin file.
pub fn remove_bin_file(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => debug!(event = "daemon.bin.remove_completed", path = %path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => warn!(
            event = "daemon.bin.remove_failed",
            path = %path.display(),
            error = %e,
        ),
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

    #[test]
    fn test_write_and_read_bin_file() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("daemon.bin");

        write_bin_file(&bin_path).unwrap();

        let result = read_bin_file(&bin_path);
        assert!(result.is_some());
        let (path, mtime) = result.unwrap();
        // Should contain the current executable path
        assert!(!path.as_os_str().is_empty());
        // Mtime should be non-zero for a real binary
        assert!(mtime > 0);
    }

    #[test]
    fn test_read_nonexistent_bin_file() {
        let path = Path::new("/tmp/kild_test_nonexistent_daemon.bin");
        assert!(read_bin_file(path).is_none());
    }

    #[test]
    fn test_read_corrupt_bin_file() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("daemon.bin");
        fs::write(&bin_path, "only_one_line\n").unwrap();

        // Missing mtime line → None
        assert!(read_bin_file(&bin_path).is_none());
    }

    #[test]
    fn test_read_bin_file_bad_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("daemon.bin");
        fs::write(&bin_path, "/usr/bin/test\nnot_a_number\n").unwrap();

        assert!(read_bin_file(&bin_path).is_none());
    }

    #[test]
    fn test_remove_bin_file() {
        let dir = tempfile::tempdir().unwrap();
        let bin_path = dir.path().join("daemon.bin");
        fs::write(&bin_path, "/usr/bin/test\n12345\n").unwrap();

        remove_bin_file(&bin_path);
        assert!(!bin_path.exists());
    }

    #[test]
    fn test_remove_nonexistent_bin_file() {
        let path = Path::new("/tmp/kild_test_remove_nonexistent.bin");
        // Should not panic on missing file
        remove_bin_file(path);
    }
}
