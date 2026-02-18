use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use bytes::Bytes;
use tokio::sync::broadcast;
use tracing::error;

use crate::errors::DaemonError;
use crate::pty::output::ScrollbackBuffer;
use crate::types::{SessionInfo, SessionStatus};

/// Unique identifier for a connected client.
pub type ClientId = u64;

/// Daemon-internal session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Creating,
    Running,
    Stopped,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Creating => write!(f, "creating"),
            SessionState::Running => write!(f, "running"),
            SessionState::Stopped => write!(f, "stopped"),
        }
    }
}

/// A session managed by the daemon, combining metadata with PTY runtime state.
///
/// The daemon is PTY-centric: it knows about commands and working directories,
/// not about git branches or agents. Those concepts live in kild-core.
pub struct DaemonSession {
    id: String,
    working_directory: String,
    command: String,
    created_at: String,
    state: SessionState,
    /// Broadcast sender for PTY output distribution to attached clients.
    /// Only present when Running.
    output_tx: Option<broadcast::Sender<Bytes>>,
    /// Ring buffer of recent PTY output for replay on attach.
    /// Shared with the PTY reader task so it can feed output into the buffer.
    scrollback: Arc<RwLock<ScrollbackBuffer>>,
    /// Set of attached client IDs.
    attached_clients: HashSet<ClientId>,
    /// Child process PID (only when Running).
    pty_pid: Option<u32>,
    /// Exit code of the PTY child process. Set when the process exits.
    exit_code: Option<i32>,
    /// Parent session ID for child sessions spawned via the pane backend protocol.
    /// `None` for top-level leader sessions.
    parent_session_id: Option<String>,
}

impl DaemonSession {
    /// Create a new session in Creating state.
    pub fn new(
        id: String,
        working_directory: String,
        command: String,
        created_at: String,
        scrollback_capacity: usize,
        parent_session_id: Option<String>,
    ) -> Self {
        Self {
            id,
            working_directory,
            command,
            created_at,
            state: SessionState::Creating,
            output_tx: None,
            scrollback: Arc::new(RwLock::new(ScrollbackBuffer::new(scrollback_capacity))),
            attached_clients: HashSet::new(),
            pty_pid: None,
            exit_code: None,
            parent_session_id,
        }
    }

    // --- Getters ---

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn pty_pid(&self) -> Option<u32> {
        self.pty_pid
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    pub fn set_exit_code(&mut self, code: Option<i32>) {
        self.exit_code = code;
    }

    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn working_directory(&self) -> &str {
        &self.working_directory
    }

    pub fn has_output(&self) -> bool {
        self.output_tx.is_some()
    }

    /// Clone the output broadcast sender (for notification after state transitions).
    pub fn output_tx(&self) -> Option<broadcast::Sender<Bytes>> {
        self.output_tx.clone()
    }

    /// Parent session ID for child sessions spawned via the pane backend protocol.
    pub fn parent_session_id(&self) -> Option<&str> {
        self.parent_session_id.as_deref()
    }

    // --- State transitions ---

    /// Transition to Running state with a broadcast sender for PTY output.
    ///
    /// # Errors
    /// Returns `InvalidStateTransition` if the session is not in Creating state.
    pub fn set_running(
        &mut self,
        output_tx: broadcast::Sender<Bytes>,
        pty_pid: Option<u32>,
    ) -> Result<(), DaemonError> {
        if !matches!(self.state, SessionState::Creating) {
            return Err(DaemonError::InvalidStateTransition(format!(
                "set_running requires Creating state, got {}",
                self.state
            )));
        }
        self.state = SessionState::Running;
        self.output_tx = Some(output_tx);
        self.pty_pid = pty_pid;
        Ok(())
    }

    /// Transition to Stopped state, clearing PTY resources.
    /// Idempotent: calling on an already-stopped session is a no-op.
    ///
    /// Valid from Running (normal stop) or Creating (early termination during
    /// session creation failure, e.g. PTY spawn fails after session object is created).
    ///
    /// # Errors
    /// Returns `InvalidStateTransition` if called from an unexpected state.
    pub fn set_stopped(&mut self) -> Result<(), DaemonError> {
        if self.state == SessionState::Stopped {
            return Ok(());
        }
        if !matches!(self.state, SessionState::Running | SessionState::Creating) {
            return Err(DaemonError::InvalidStateTransition(format!(
                "set_stopped requires Running or Creating state, got {}",
                self.state
            )));
        }
        self.state = SessionState::Stopped;
        self.output_tx = None;
        self.pty_pid = None;
        Ok(())
    }

    /// Attach a client to this session.
    pub fn attach_client(&mut self, client_id: ClientId) {
        self.attached_clients.insert(client_id);
    }

    /// Detach a client from this session.
    pub fn detach_client(&mut self, client_id: ClientId) {
        self.attached_clients.remove(&client_id);
    }

    /// Number of currently attached clients.
    pub fn client_count(&self) -> usize {
        self.attached_clients.len()
    }

    /// Subscribe to PTY output. Returns `None` if not running.
    pub fn subscribe_output(&self) -> Option<broadcast::Receiver<Bytes>> {
        self.output_tx.as_ref().map(|tx| tx.subscribe())
    }

    /// Subscribe to PTY output without registering as an attached client.
    ///
    /// For observers (e.g. pane backend connection) that want output events
    /// but should not appear in the session's client count. Returns `None`
    /// if the session is not running.
    pub fn subscribe_output_passive(&self) -> Option<broadcast::Receiver<Bytes>> {
        self.output_tx.as_ref().map(|tx| tx.subscribe())
    }

    /// Get scrollback buffer contents for replay on attach.
    pub fn scrollback_contents(&self) -> Vec<u8> {
        match self.scrollback.read() {
            Ok(sb) => sb.contents(),
            Err(poisoned) => {
                // A poisoned read lock means another thread panicked while holding
                // the write lock. Return whatever was in the buffer — partial scrollback
                // is better than nothing for an attaching client.
                error!(
                    event = "daemon.session.scrollback_lock_poisoned",
                    session_id = %self.id,
                    error = %poisoned,
                    "RwLock poisoned on read, returning partial scrollback contents",
                );
                poisoned.into_inner().contents()
            }
        }
    }

    /// Get a clone of the shared scrollback buffer for the PTY reader task.
    pub fn shared_scrollback(&self) -> Arc<RwLock<ScrollbackBuffer>> {
        self.scrollback.clone()
    }

    /// Convert to wire format `SessionInfo`.
    pub fn to_session_info(&self) -> SessionInfo {
        let status = match self.state {
            SessionState::Creating => SessionStatus::Creating,
            SessionState::Running => SessionStatus::Running,
            SessionState::Stopped => SessionStatus::Stopped,
        };
        SessionInfo {
            id: self.id.clone().into(),
            working_directory: self.working_directory.clone(),
            command: self.command.clone(),
            status,
            created_at: self.created_at.clone(),
            client_count: Some(self.client_count()),
            pty_pid: self.pty_pid,
            exit_code: self.exit_code,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session() -> DaemonSession {
        DaemonSession::new(
            "myapp_feature".to_string(),
            "/tmp/wt".to_string(),
            "claude".to_string(),
            "2026-02-09T14:30:00Z".to_string(),
            1024,
            None,
        )
    }

    #[test]
    fn test_new_session_starts_creating() {
        let session = test_session();
        assert_eq!(session.state(), SessionState::Creating);
        assert!(!session.has_output());
        assert_eq!(session.client_count(), 0);
        assert!(session.pty_pid().is_none());
    }

    #[test]
    fn test_set_running() {
        let mut session = test_session();
        let (tx, _) = broadcast::channel(16);
        session.set_running(tx, Some(12345)).unwrap();
        assert_eq!(session.state(), SessionState::Running);
        assert!(session.has_output());
        assert_eq!(session.pty_pid(), Some(12345));
    }

    #[test]
    fn test_set_stopped() {
        let mut session = test_session();
        let (tx, _) = broadcast::channel(16);
        session.set_running(tx, Some(12345)).unwrap();
        session.set_stopped().unwrap();
        assert_eq!(session.state(), SessionState::Stopped);
        assert!(!session.has_output());
        assert!(session.pty_pid().is_none());
    }

    #[test]
    fn test_client_tracking() {
        let mut session = test_session();
        assert_eq!(session.client_count(), 0);

        session.attach_client(1);
        assert_eq!(session.client_count(), 1);

        session.attach_client(2);
        assert_eq!(session.client_count(), 2);

        // Duplicate attach is idempotent
        session.attach_client(1);
        assert_eq!(session.client_count(), 2);

        session.detach_client(1);
        assert_eq!(session.client_count(), 1);

        session.detach_client(2);
        assert_eq!(session.client_count(), 0);
    }

    #[test]
    fn test_subscribe_output_when_running() {
        let mut session = test_session();
        assert!(session.subscribe_output().is_none());

        let (tx, _) = broadcast::channel(16);
        session.set_running(tx, None).unwrap();
        assert!(session.subscribe_output().is_some());
    }

    #[test]
    fn test_scrollback_empty_initially() {
        let session = test_session();
        assert!(session.scrollback_contents().is_empty());
    }

    #[test]
    fn test_to_session_info() {
        let mut session = test_session();
        session.attach_client(1);
        session.attach_client(2);

        let info = session.to_session_info();
        assert_eq!(&*info.id, "myapp_feature");
        assert_eq!(info.working_directory, "/tmp/wt");
        assert_eq!(info.command, "claude");
        assert_eq!(info.status, SessionStatus::Creating);
        assert_eq!(info.client_count, Some(2));
    }

    #[test]
    fn test_session_state_display() {
        assert_eq!(SessionState::Creating.to_string(), "creating");
        assert_eq!(SessionState::Running.to_string(), "running");
        assert_eq!(SessionState::Stopped.to_string(), "stopped");
    }

    #[test]
    fn test_set_running_on_running_returns_error() {
        let mut session = test_session();
        let (tx, _) = broadcast::channel(16);
        session.set_running(tx.clone(), Some(12345)).unwrap();
        let err = session.set_running(tx, Some(12345)).unwrap_err();
        assert!(err.to_string().contains("set_running requires Creating"));
    }

    #[test]
    fn test_set_running_on_stopped_returns_error() {
        let mut session = test_session();
        session.set_stopped().unwrap();
        let (tx, _) = broadcast::channel(16);
        let err = session.set_running(tx, Some(12345)).unwrap_err();
        assert!(err.to_string().contains("set_running requires Creating"));
    }

    #[test]
    fn test_set_stopped_on_creating_succeeds() {
        let mut session = test_session();
        assert_eq!(session.state(), SessionState::Creating);
        session.set_stopped().unwrap();
        assert_eq!(session.state(), SessionState::Stopped);
    }

    #[test]
    fn test_set_stopped_idempotent() {
        let mut session = test_session();
        let (tx, _) = broadcast::channel(16);
        session.set_running(tx, Some(12345)).unwrap();
        session.set_stopped().unwrap();
        session.set_stopped().unwrap(); // second call is no-op
        assert_eq!(session.state(), SessionState::Stopped);
    }

    #[test]
    fn test_exit_code_none_initially() {
        let session = test_session();
        assert_eq!(session.exit_code(), None);
    }

    #[test]
    fn test_exit_code_stored_after_set() {
        let mut session = test_session();
        let (tx, _) = broadcast::channel(16);
        session.set_running(tx, Some(123)).unwrap();
        session.set_exit_code(Some(42));
        session.set_stopped().unwrap();
        assert_eq!(session.exit_code(), Some(42));
    }

    #[test]
    fn test_exit_code_in_session_info() {
        let mut session = test_session();
        let (tx, _) = broadcast::channel(16);
        session.set_running(tx, Some(123)).unwrap();
        session.set_exit_code(Some(1));

        let info = session.to_session_info();
        assert_eq!(info.exit_code, Some(1));
    }

    #[test]
    fn test_exit_code_none_in_session_info() {
        let session = test_session();
        let info = session.to_session_info();
        assert_eq!(info.exit_code, None);
    }

    #[test]
    fn test_parent_session_id_defaults_to_none() {
        let session = test_session();
        assert_eq!(session.parent_session_id(), None);
    }

    #[test]
    fn test_parent_session_id_stored_and_returned() {
        let session = DaemonSession::new(
            "myapp_feature_ctx_1".to_string(),
            "/tmp/wt".to_string(),
            "claude".to_string(),
            "2026-02-09T14:30:00Z".to_string(),
            1024,
            Some("myapp_feature".to_string()),
        );
        assert_eq!(session.parent_session_id(), Some("myapp_feature"));
    }

    #[test]
    fn test_subscribe_output_passive_none_when_not_running() {
        let session = test_session();
        assert!(session.subscribe_output_passive().is_none());
    }

    #[test]
    fn test_subscribe_output_passive_some_when_running() {
        let mut session = test_session();
        let (tx, _) = broadcast::channel(16);
        session.set_running(tx, None).unwrap();
        assert!(session.subscribe_output_passive().is_some());
    }
}
