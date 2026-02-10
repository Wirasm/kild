use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tracing::{debug, error, info};

use crate::errors::DaemonError;

/// Handle to a live PTY session.
pub struct ManagedPty {
    /// Master end of the PTY. Used for resize and cloning readers.
    master: Box<dyn MasterPty + Send>,
    /// Child process handle. Used for wait/kill.
    child: Box<dyn Child + Send + Sync>,
    /// Writer to PTY stdin. Wrapped in Arc<Mutex<>> for shared mutable access
    /// from multiple threads. Obtained once via take_writer() during creation.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Current PTY dimensions.
    size: PtySize,
}

impl std::fmt::Debug for ManagedPty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedPty")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl ManagedPty {
    pub fn size(&self) -> PtySize {
        self.size
    }

    /// Clone the PTY master reader for reading output in a background task.
    pub fn try_clone_reader(&self) -> Result<Box<dyn std::io::Read + Send>, DaemonError> {
        self.master
            .try_clone_reader()
            .map_err(|e| DaemonError::PtyError(format!("clone reader: {}", e)))
    }

    /// Write bytes to PTY stdin.
    pub fn write_stdin(&self, data: &[u8]) -> Result<(), DaemonError> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|e| DaemonError::PtyError(format!("lock writer: {}", e)))?;
        writer
            .write_all(data)
            .map_err(|e| DaemonError::PtyError(format!("write stdin: {}", e)))?;
        writer
            .flush()
            .map_err(|e| DaemonError::PtyError(format!("flush stdin: {}", e)))?;
        Ok(())
    }

    /// Resize the PTY.
    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<(), DaemonError> {
        let new_size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        self.master
            .resize(new_size)
            .map_err(|e| DaemonError::PtyError(format!("resize: {}", e)))?;
        self.size = new_size;
        debug!(
            event = "daemon.pty.resize_completed",
            rows = rows,
            cols = cols,
        );
        Ok(())
    }

    /// Get the child process ID, if available.
    pub fn child_process_id(&self) -> Option<u32> {
        self.child.process_id()
    }

    /// Wait for the child process to exit. Blocks until exit.
    pub fn wait(&mut self) -> Result<portable_pty::ExitStatus, DaemonError> {
        self.child
            .wait()
            .map_err(|e| DaemonError::PtyError(format!("wait: {}", e)))
    }

    /// Kill the child process.
    pub fn kill(&mut self) -> Result<(), DaemonError> {
        self.child
            .kill()
            .map_err(|e| DaemonError::PtyError(format!("kill: {}", e)))
    }
}

/// Manages all live PTY instances in the daemon.
pub struct PtyManager {
    ptys: HashMap<String, ManagedPty>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            ptys: HashMap::new(),
        }
    }

    /// Create a new PTY and spawn a command in it.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &mut self,
        session_id: &str,
        command: &str,
        args: &[&str],
        working_dir: &std::path::Path,
        rows: u16,
        cols: u16,
        env_vars: &[(String, String)],
        use_login_shell: bool,
    ) -> Result<&ManagedPty, DaemonError> {
        if self.ptys.contains_key(session_id) {
            return Err(DaemonError::SessionAlreadyExists(session_id.to_string()));
        }

        let pty_system = native_pty_system();
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(size)
            .map_err(|e| DaemonError::PtyError(format!("openpty: {}", e)))?;

        let mut cmd = if use_login_shell {
            // Use portable-pty's built-in login shell setup: $SHELL with argv[0] = "-zsh".
            // IMPORTANT: new_default_prog() panics if .arg() is called on it.
            let mut c = CommandBuilder::new_default_prog();
            c.cwd(working_dir);
            c
        } else {
            let mut c = CommandBuilder::new(command);
            c.args(args);
            c.cwd(working_dir);
            c
        };

        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        info!(
            event = "daemon.pty.create_started",
            session_id = session_id,
            command = command,
            rows = rows,
            cols = cols,
        );

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| DaemonError::PtyError(format!("spawn: {}", e)))?;

        let pid = child.process_id();

        // Take the writer once (portable-pty only allows one take_writer call)
        let writer = match pair.master.take_writer() {
            Ok(w) => w,
            Err(e) => {
                if let Err(kill_err) = child.kill() {
                    error!(
                        event = "daemon.pty.create_cleanup_failed",
                        session_id = session_id,
                        error = %kill_err,
                        "Failed to kill child process during PTY creation cleanup",
                    );
                }
                return Err(DaemonError::PtyError(format!("take writer: {}", e)));
            }
        };

        let managed = ManagedPty {
            master: pair.master,
            child,
            writer: Arc::new(Mutex::new(writer)),
            size,
        };

        self.ptys.insert(session_id.to_string(), managed);

        info!(
            event = "daemon.pty.create_completed",
            session_id = session_id,
            pid = ?pid,
        );

        self.ptys.get(session_id).ok_or_else(|| {
            DaemonError::PtyError("HashMap corruption: just-inserted PTY missing".to_string())
        })
    }

    /// Get a reference to a managed PTY.
    pub fn get(&self, session_id: &str) -> Option<&ManagedPty> {
        self.ptys.get(session_id)
    }

    /// Get a mutable reference to a managed PTY.
    pub fn get_mut(&mut self, session_id: &str) -> Option<&mut ManagedPty> {
        self.ptys.get_mut(session_id)
    }

    /// Remove and return a managed PTY.
    pub fn remove(&mut self, session_id: &str) -> Option<ManagedPty> {
        let pty = self.ptys.remove(session_id);
        if pty.is_some() {
            debug!(
                event = "daemon.pty.remove_completed",
                session_id = session_id
            );
        }
        pty
    }

    /// Destroy a PTY by killing the child process and removing it.
    pub fn destroy(&mut self, session_id: &str) -> Result<(), DaemonError> {
        if let Some(mut pty) = self.ptys.remove(session_id) {
            info!(
                event = "daemon.pty.destroy_started",
                session_id = session_id,
            );
            pty.kill()?;
            info!(
                event = "daemon.pty.destroy_completed",
                session_id = session_id,
            );
            Ok(())
        } else {
            Err(DaemonError::SessionNotFound(session_id.to_string()))
        }
    }

    /// Number of active PTYs.
    pub fn count(&self) -> usize {
        self.ptys.len()
    }

    /// All session IDs with active PTYs.
    pub fn session_ids(&self) -> Vec<String> {
        self.ptys.keys().cloned().collect()
    }
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_with_nonexistent_command_returns_error() {
        let mut mgr = PtyManager::new();
        let tmpdir = tempfile::tempdir().unwrap();
        let result = mgr.create(
            "s1",
            "/nonexistent/binary/that/does/not/exist",
            &[],
            tmpdir.path(),
            24,
            80,
            &[],
            false,
        );
        match result {
            Err(DaemonError::PtyError(msg)) => {
                assert!(msg.contains("spawn"), "expected spawn error, got: {}", msg)
            }
            Err(other) => panic!("expected PtyError, got: {:?}", other),
            Ok(_) => panic!("expected error, got Ok"),
        }
        // No PTY should be tracked after failure
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn test_create_with_duplicate_session_id_fails() {
        let mut mgr = PtyManager::new();
        let tmpdir = tempfile::tempdir().unwrap();

        mgr.create("s1", "sleep", &["10"], tmpdir.path(), 24, 80, &[], false)
            .unwrap();
        assert_eq!(mgr.count(), 1);

        let result = mgr.create("s1", "sleep", &["10"], tmpdir.path(), 24, 80, &[], false);
        match result {
            Err(DaemonError::SessionAlreadyExists(id)) => assert_eq!(id, "s1"),
            Err(other) => panic!("expected SessionAlreadyExists, got: {:?}", other),
            Ok(_) => panic!("expected error, got Ok"),
        }

        // Cleanup
        let _ = mgr.destroy("s1");
    }

    #[test]
    fn test_no_pty_tracked_after_failed_create() {
        let mut mgr = PtyManager::new();
        let tmpdir = tempfile::tempdir().unwrap();

        let _ = mgr.create(
            "fail-session",
            "/this/binary/does/not/exist",
            &[],
            tmpdir.path(),
            24,
            80,
            &[],
            false,
        );

        assert!(mgr.get("fail-session").is_none());
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn test_create_and_destroy_lifecycle() {
        let mut mgr = PtyManager::new();
        let tmpdir = tempfile::tempdir().unwrap();

        mgr.create("s1", "sleep", &["10"], tmpdir.path(), 24, 80, &[], false)
            .unwrap();
        assert_eq!(mgr.count(), 1);
        assert!(mgr.get("s1").is_some());

        mgr.destroy("s1").unwrap();
        assert_eq!(mgr.count(), 0);
        assert!(mgr.get("s1").is_none());
    }

    #[test]
    fn test_create_with_login_shell_uses_default_prog() {
        let mut mgr = PtyManager::new();
        let tmpdir = tempfile::tempdir().unwrap();
        // use_login_shell=true should succeed (uses $SHELL via new_default_prog)
        let result = mgr.create("s1", "", &[], tmpdir.path(), 24, 80, &[], true);
        assert!(result.is_ok(), "Login shell mode should succeed");
        mgr.destroy("s1").unwrap();
    }

    #[test]
    fn test_destroy_nonexistent_returns_error() {
        let mut mgr = PtyManager::new();
        let result = mgr.destroy("nonexistent");
        assert!(result.is_err());
        match result.unwrap_err() {
            DaemonError::SessionNotFound(id) => assert_eq!(id, "nonexistent"),
            other => panic!("expected SessionNotFound, got: {:?}", other),
        }
    }
}
