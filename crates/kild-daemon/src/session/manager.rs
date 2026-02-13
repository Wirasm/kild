use std::collections::HashMap;

use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::errors::DaemonError;
use crate::pty::manager::PtyManager;
use crate::pty::output::{PtyExitEvent, spawn_pty_reader};
use crate::session::state::{ClientId, DaemonSession, SessionState};
use crate::types::{DaemonConfig, SessionInfo};

/// Orchestrates session lifecycle within the daemon.
///
/// Manages the map of `DaemonSession` instances, delegates to `PtyManager`
/// for PTY allocation, and handles client attach/detach tracking.
pub struct SessionManager {
    sessions: HashMap<String, DaemonSession>,
    pty_manager: PtyManager,
    config: DaemonConfig,
    next_client_id: ClientId,
    /// Sender for PTY exit notifications. Passed to each PTY reader task.
    pty_exit_tx: tokio::sync::mpsc::UnboundedSender<PtyExitEvent>,
}

impl SessionManager {
    pub fn new(
        config: DaemonConfig,
        pty_exit_tx: tokio::sync::mpsc::UnboundedSender<PtyExitEvent>,
    ) -> Self {
        Self {
            sessions: HashMap::new(),
            pty_manager: PtyManager::new(),
            config,
            next_client_id: 1,
            pty_exit_tx,
        }
    }

    /// Allocate a new client ID.
    pub fn next_client_id(&mut self) -> ClientId {
        let id = self.next_client_id;
        self.next_client_id = self.next_client_id.wrapping_add(1);
        id
    }

    /// Create a new session with a PTY.
    ///
    /// Creates the PTY, spawns the command, and sets up output broadcasting.
    /// Does NOT create git worktrees — that is kild-core's responsibility.
    /// The daemon is a pure PTY manager.
    #[allow(clippy::too_many_arguments)]
    pub fn create_session(
        &mut self,
        session_id: &str,
        working_directory: &str,
        command: &str,
        args: &[String],
        env_vars: &[(String, String)],
        rows: u16,
        cols: u16,
        use_login_shell: bool,
    ) -> Result<SessionInfo, DaemonError> {
        if self.sessions.contains_key(session_id) {
            return Err(DaemonError::SessionAlreadyExists(session_id.to_string()));
        }

        info!(
            event = "daemon.session.create_started",
            session_id = session_id,
            command = command,
            working_directory = working_directory,
        );

        let created_at = chrono::Utc::now().to_rfc3339();

        let mut session = DaemonSession::new(
            session_id.to_string(),
            working_directory.to_string(),
            command.to_string(),
            created_at,
            self.config.scrollback_buffer_size,
        );

        // Create the PTY and spawn the command
        let working_dir = std::path::Path::new(working_directory);
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let managed_pty = self.pty_manager.create(
            session_id,
            command,
            &args_refs,
            working_dir,
            rows,
            cols,
            env_vars,
            use_login_shell,
        )?;

        let pty_pid = managed_pty.child_process_id();

        // Clone the reader for the background read task
        let reader = managed_pty.try_clone_reader()?;

        // Create broadcast channel for output distribution
        let (output_tx, _) = broadcast::channel(64);
        let reader_tx = output_tx.clone();

        // Get shared scrollback buffer so PTY reader can feed it
        let shared_scrollback = session.shared_scrollback();

        // Spawn background task to read PTY output
        spawn_pty_reader(
            session_id.to_string(),
            reader,
            reader_tx,
            shared_scrollback,
            Some(self.pty_exit_tx.clone()),
        );

        // Transition session to Running
        session.set_running(output_tx, pty_pid)?;

        let info = session.to_session_info();
        self.sessions.insert(session_id.to_string(), session);

        info!(
            event = "daemon.session.create_completed",
            session_id = session_id,
            pid = ?pty_pid,
        );

        Ok(info)
    }

    /// Attach a client to a session. Returns a broadcast receiver for PTY output.
    pub fn attach_client(
        &mut self,
        session_id: &str,
        client_id: ClientId,
    ) -> Result<broadcast::Receiver<Vec<u8>>, DaemonError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| DaemonError::SessionNotFound(session_id.to_string()))?;

        if session.state() != SessionState::Running {
            return Err(DaemonError::SessionNotRunning(session_id.to_string()));
        }

        session.attach_client(client_id);

        let rx = session
            .subscribe_output()
            .ok_or_else(|| DaemonError::PtyError("no output channel available".to_string()))?;

        debug!(
            event = "daemon.session.client_attached",
            session_id = session_id,
            client_id = client_id,
            client_count = session.client_count(),
        );

        Ok(rx)
    }

    /// Detach a client from a session.
    pub fn detach_client(
        &mut self,
        session_id: &str,
        client_id: ClientId,
    ) -> Result<(), DaemonError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| DaemonError::SessionNotFound(session_id.to_string()))?;

        session.detach_client(client_id);

        debug!(
            event = "daemon.session.client_detached",
            session_id = session_id,
            client_id = client_id,
            client_count = session.client_count(),
        );

        Ok(())
    }

    /// Resize the PTY for a session.
    pub fn resize_pty(
        &mut self,
        session_id: &str,
        rows: u16,
        cols: u16,
    ) -> Result<(), DaemonError> {
        let pty = self
            .pty_manager
            .get_mut(session_id)
            .ok_or_else(|| DaemonError::SessionNotFound(session_id.to_string()))?;

        pty.resize(rows, cols)
    }

    /// Write data to a session's PTY stdin.
    pub fn write_stdin(&self, session_id: &str, data: &[u8]) -> Result<(), DaemonError> {
        let pty = self
            .pty_manager
            .get(session_id)
            .ok_or_else(|| DaemonError::SessionNotFound(session_id.to_string()))?;

        pty.write_stdin(data)
    }

    /// Stop a session's agent process.
    /// Idempotent: stopping an already-stopped session is a no-op.
    pub fn stop_session(&mut self, session_id: &str) -> Result<(), DaemonError> {
        info!(
            event = "daemon.session.stop_started",
            session_id = session_id,
        );

        // Check if session is already stopped (PTY may have exited on its own)
        if let Some(session) = self.sessions.get(session_id) {
            if session.state() == SessionState::Stopped {
                info!(
                    event = "daemon.session.stop_already_stopped",
                    session_id = session_id,
                );
                return Ok(());
            }
        } else {
            return Err(DaemonError::SessionNotFound(session_id.to_string()));
        }

        // Destroy PTY (may already be gone if process exited naturally)
        match self.pty_manager.destroy(session_id) {
            Ok(()) => {}
            Err(DaemonError::SessionNotFound(_)) => {
                // Expected: PTY already removed (process exited naturally)
            }
            Err(e) => {
                warn!(
                    event = "daemon.session.pty_destroy_warning",
                    session_id = session_id,
                    error = %e,
                );
            }
        }

        if let Some(session) = self.sessions.get_mut(session_id) {
            session.set_stopped()?;
        }

        info!(
            event = "daemon.session.stop_completed",
            session_id = session_id,
        );

        Ok(())
    }

    /// Destroy a session entirely.
    ///
    /// The session state is always removed, regardless of the force flag.
    /// When `force` is true, PTY kill failures are logged but the operation succeeds.
    /// When `force` is false, PTY kill failures return an error after session removal.
    pub fn destroy_session(&mut self, session_id: &str, force: bool) -> Result<(), DaemonError> {
        info!(
            event = "daemon.session.destroy_started",
            session_id = session_id,
            force = force,
        );

        // Kill PTY if it exists
        let pty_error = if self.pty_manager.get(session_id).is_some() {
            self.pty_manager.destroy(session_id).err()
        } else {
            None
        };

        if let Some(ref e) = pty_error {
            warn!(
                event = "daemon.session.destroy_pty_failed",
                session_id = session_id,
                error = %e,
            );
            warn!(
                event = "daemon.session.possible_orphaned_process",
                session_id = session_id,
                message = "PTY kill failed during destroy — child process may still be running",
            );
        }

        // Always remove the session state during destroy
        self.sessions.remove(session_id);

        info!(
            event = "daemon.session.destroy_completed",
            session_id = session_id,
        );

        // Propagate PTY kill error unless force mode
        if !force && let Some(e) = pty_error {
            return Err(e);
        }

        Ok(())
    }

    /// Get session info by ID.
    pub fn get_session(&self, session_id: &str) -> Option<SessionInfo> {
        self.sessions.get(session_id).map(|s| s.to_session_info())
    }

    /// List all sessions.
    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        self.sessions
            .values()
            .map(|s| s.to_session_info())
            .collect()
    }

    /// Get scrollback buffer contents for a session (for replay on attach).
    pub fn scrollback_contents(&self, session_id: &str) -> Option<Vec<u8>> {
        self.sessions
            .get(session_id)
            .map(|s| s.scrollback_contents())
    }

    /// Number of sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Number of active PTYs.
    pub fn active_pty_count(&self) -> usize {
        self.pty_manager.count()
    }

    /// Detach a client from all sessions (called on connection close).
    pub fn detach_client_from_all(&mut self, client_id: ClientId) {
        for session in self.sessions.values_mut() {
            session.detach_client(client_id);
        }
    }

    /// Handle a PTY exit event: transition the session to Stopped and clean up PTY.
    /// Returns the session_id and output_tx if the session had attached clients
    /// (so the caller can broadcast a session_event notification).
    pub fn handle_pty_exit(&mut self, session_id: &str) -> Option<broadcast::Sender<Vec<u8>>> {
        // Clean up PTY resources and capture exit code
        let exit_code = match self.pty_manager.remove(session_id) {
            Some(mut pty) => {
                // Child has already exited (reader got EOF), so wait() returns immediately
                let code = match pty.wait() {
                    Ok(status) => Some(status.exit_code() as i32),
                    Err(e) => {
                        warn!(
                            event = "daemon.session.exit_code_unavailable",
                            session_id = session_id,
                            error = %e,
                        );
                        None
                    }
                };
                debug!(
                    event = "daemon.session.pty_removed",
                    session_id = session_id,
                );
                code
            }
            None => {
                warn!(
                    event = "daemon.session.pty_already_removed",
                    session_id = session_id,
                    "PTY already removed (race with stop or natural exit)",
                );
                None
            }
        };

        info!(
            event = "daemon.session.pty_exited",
            session_id = session_id,
            exit_code = ?exit_code,
        );

        // Transition session to Stopped and record exit code
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.set_exit_code(exit_code);
            let output_tx = session.output_tx();
            if let Err(e) = session.set_stopped() {
                error!(
                    event = "daemon.session.stop_transition_failed",
                    session_id = session_id,
                    error = %e,
                );
                return None;
            }
            return output_tx;
        }

        None
    }

    /// Get client count for a session (test helper).
    #[cfg(test)]
    pub fn client_count(&self, session_id: &str) -> Option<usize> {
        self.sessions.get(session_id).map(|s| s.client_count())
    }

    /// Stop all running sessions (called during shutdown).
    pub fn stop_all(&mut self) {
        let session_ids: Vec<String> = self
            .sessions
            .values()
            .filter(|s| s.state() == SessionState::Running)
            .map(|s| s.id().to_string())
            .collect();

        let mut failed_stops: Vec<String> = Vec::with_capacity(session_ids.len());
        for session_id in session_ids {
            if let Err(e) = self.stop_session(&session_id) {
                warn!(
                    event = "daemon.session.stop_failed",
                    session_id = session_id,
                    error = %e,
                );
                failed_stops.push(session_id);
            }
        }

        if !failed_stops.is_empty() {
            error!(
                event = "daemon.session.shutdown_incomplete",
                failed_count = failed_stops.len(),
                failed_sessions = ?failed_stops,
                message = "Some sessions failed to stop — possible orphaned processes",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::output::PtyExitEvent;
    use crate::types::DaemonConfig;

    fn test_manager() -> (
        SessionManager,
        tokio::sync::mpsc::UnboundedReceiver<PtyExitEvent>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let config = DaemonConfig::default();
        (SessionManager::new(config, tx), rx)
    }

    #[tokio::test]
    async fn test_handle_pty_exit_transitions_to_stopped() {
        let (mut mgr, _rx) = test_manager();
        let tmpdir = tempfile::tempdir().unwrap();
        let wd = tmpdir.path().to_str().unwrap();

        // Create a session running "echo hello" (exits immediately)
        mgr.create_session("s1", wd, "echo", &["hello".to_string()], &[], 24, 80, false)
            .unwrap();

        // Verify it starts as Running
        let info = mgr.get_session("s1").unwrap();
        assert_eq!(info.status, kild_protocol::SessionStatus::Running);

        // Simulate PTY exit
        mgr.handle_pty_exit("s1");

        // Session should now be Stopped
        let info = mgr.get_session("s1").unwrap();
        assert_eq!(info.status, kild_protocol::SessionStatus::Stopped);
    }

    #[test]
    fn test_handle_pty_exit_nonexistent_session_returns_none() {
        let (mut mgr, _rx) = test_manager();

        // Should not panic, should return None
        let result = mgr.handle_pty_exit("nonexistent");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_handle_pty_exit_removes_pty() {
        let (mut mgr, _rx) = test_manager();
        let tmpdir = tempfile::tempdir().unwrap();
        let wd = tmpdir.path().to_str().unwrap();

        mgr.create_session("s1", wd, "echo", &["hi".to_string()], &[], 24, 80, false)
            .unwrap();

        assert_eq!(mgr.active_pty_count(), 1);

        mgr.handle_pty_exit("s1");

        // PTY should be removed
        assert_eq!(mgr.active_pty_count(), 0);
    }

    #[tokio::test]
    async fn test_attach_multiple_clients_tracks_count() {
        let (mut mgr, _rx) = test_manager();
        let tmpdir = tempfile::tempdir().unwrap();
        let wd = tmpdir.path().to_str().unwrap();

        // Use "sleep" to keep the session running during the test
        mgr.create_session("s1", wd, "sleep", &["10".to_string()], &[], 24, 80, false)
            .unwrap();

        assert_eq!(mgr.client_count("s1"), Some(0));

        let _rx1 = mgr.attach_client("s1", 1).unwrap();
        assert_eq!(mgr.client_count("s1"), Some(1));

        let _rx2 = mgr.attach_client("s1", 2).unwrap();
        assert_eq!(mgr.client_count("s1"), Some(2));

        let _rx3 = mgr.attach_client("s1", 3).unwrap();
        assert_eq!(mgr.client_count("s1"), Some(3));

        // Cleanup
        let _ = mgr.destroy_session("s1", true);
    }

    #[tokio::test]
    async fn test_detach_without_prior_attach_is_idempotent() {
        let (mut mgr, _rx) = test_manager();
        let tmpdir = tempfile::tempdir().unwrap();
        let wd = tmpdir.path().to_str().unwrap();

        mgr.create_session("s1", wd, "sleep", &["10".to_string()], &[], 24, 80, false)
            .unwrap();

        // Detaching a client that was never attached should succeed without error
        let result = mgr.detach_client("s1", 42);
        assert!(result.is_ok());
        assert_eq!(mgr.client_count("s1"), Some(0));

        // Cleanup
        let _ = mgr.destroy_session("s1", true);
    }

    #[tokio::test]
    async fn test_create_duplicate_session_fails() {
        let (mut mgr, _rx) = test_manager();
        let tmpdir = tempfile::tempdir().unwrap();
        let wd = tmpdir.path().to_str().unwrap();

        mgr.create_session("s1", wd, "sleep", &["10".to_string()], &[], 24, 80, false)
            .unwrap();

        let result = mgr.create_session("s1", wd, "sleep", &["10".to_string()], &[], 24, 80, false);
        assert!(result.is_err());
        match result.unwrap_err() {
            DaemonError::SessionAlreadyExists(id) => assert_eq!(id, "s1"),
            other => panic!("expected SessionAlreadyExists, got: {:?}", other),
        }

        // Cleanup
        let _ = mgr.destroy_session("s1", true);
    }

    #[test]
    fn test_next_client_id_increments() {
        let (mut mgr, _rx) = test_manager();
        assert_eq!(mgr.next_client_id(), 1);
        assert_eq!(mgr.next_client_id(), 2);
        assert_eq!(mgr.next_client_id(), 3);
    }
}
