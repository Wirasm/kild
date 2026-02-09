use std::collections::HashMap;

use tokio::sync::broadcast;
use tracing::{debug, info, warn};

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
        self.next_client_id += 1;
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
        session.set_running(output_tx, pty_pid);

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
    pub fn stop_session(&mut self, session_id: &str) -> Result<(), DaemonError> {
        info!(
            event = "daemon.session.stop_started",
            session_id = session_id,
        );

        self.pty_manager.destroy(session_id)?;

        if let Some(session) = self.sessions.get_mut(session_id) {
            session.set_stopped();
        }

        info!(
            event = "daemon.session.stop_completed",
            session_id = session_id,
        );

        Ok(())
    }

    /// Destroy a session entirely.
    pub fn destroy_session(&mut self, session_id: &str) -> Result<(), DaemonError> {
        info!(
            event = "daemon.session.destroy_started",
            session_id = session_id,
        );

        // Kill PTY if it exists
        if self.pty_manager.get(session_id).is_some()
            && let Err(e) = self.pty_manager.destroy(session_id)
        {
            warn!(
                event = "daemon.session.destroy_pty_failed",
                session_id = session_id,
                error = %e,
            );
        }

        self.sessions.remove(session_id);

        info!(
            event = "daemon.session.destroy_completed",
            session_id = session_id,
        );

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
        info!(event = "daemon.session.pty_exited", session_id = session_id,);

        // Clean up PTY resources
        let _ = self.pty_manager.remove(session_id);

        // Transition session to Stopped
        if let Some(session) = self.sessions.get_mut(session_id) {
            let output_tx = session.output_tx();
            session.set_stopped();
            return output_tx;
        }

        None
    }

    /// Stop all running sessions (called during shutdown).
    pub fn stop_all(&mut self) {
        let session_ids: Vec<String> = self
            .sessions
            .values()
            .filter(|s| s.state() == SessionState::Running)
            .map(|s| s.id().to_string())
            .collect();

        for session_id in session_ids {
            if let Err(e) = self.stop_session(&session_id) {
                warn!(
                    event = "daemon.session.stop_failed",
                    session_id = session_id,
                    error = %e,
                );
            }
        }
    }
}
