//! Synchronous IPC client for communicating with the KILD daemon.
//!
//! Delegates JSONL framing to `kild_protocol::IpcConnection`.
//! This module provides domain-specific request helpers and error mapping.

use std::cell::RefCell;
use std::path::Path;
use std::time::Duration;

use kild_protocol::{
    ClientMessage, DaemonMessage, ErrorCode, IpcConnection, IpcError, SessionId, SessionStatus,
};
use tracing::{debug, info, warn};

thread_local! {
    static CACHED_CONNECTION: RefCell<Option<IpcConnection>> = const { RefCell::new(None) };
}

/// Get a connection to the daemon, reusing a cached one if available.
///
/// Uses thread-local storage to avoid lock contention. Each thread maintains
/// its own connection — for single-threaded CLI commands, this means one
/// connection is reused across sequential operations within the same invocation.
///
/// The connection is taken from the cache (exclusive ownership) and must be
/// returned with `return_connection()` after successful use.
fn get_connection() -> Result<IpcConnection, DaemonClientError> {
    let socket_path = crate::daemon::socket_path();

    CACHED_CONNECTION.with(|cell| {
        let mut cached = cell.borrow_mut();
        if let Some(conn) = cached.take() {
            if conn.is_alive() {
                debug!(event = "core.daemon.connection_reused");
                return Ok(conn);
            }
            debug!(event = "core.daemon.connection_stale");
        }
        let conn = IpcConnection::connect(&socket_path)?;
        debug!(event = "core.daemon.connection_created");
        Ok(conn)
    })
}

/// Return a connection to the cache for reuse by the next call.
///
/// Re-validates liveness before caching to prevent storing broken connections.
fn return_connection(conn: IpcConnection) {
    if !conn.is_alive() {
        debug!(event = "core.daemon.connection_dropped_on_return");
        return;
    }
    CACHED_CONNECTION.with(|cell| {
        debug!(event = "core.daemon.connection_cached");
        *cell.borrow_mut() = Some(conn);
    });
}

use crate::errors::KildError;

/// Result of creating a PTY session in the daemon.
#[derive(Debug, Clone)]
pub struct DaemonCreateResult {
    /// Daemon-assigned session identifier.
    pub daemon_session_id: String,
}

/// Error communicating with the daemon.
#[derive(Debug, thiserror::Error)]
pub enum DaemonClientError {
    #[error("Daemon is not running (socket not found at {path})")]
    NotRunning { path: String },

    #[error("Connection failed: {message}")]
    ConnectionFailed { message: String },

    #[error("Daemon returned error [{code}]: {message}")]
    DaemonError { code: ErrorCode, message: String },

    #[error("IPC protocol error: {message}")]
    ProtocolError { message: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl KildError for DaemonClientError {
    fn error_code(&self) -> &'static str {
        match self {
            DaemonClientError::NotRunning { .. } => "DAEMON_NOT_RUNNING",
            DaemonClientError::ConnectionFailed { .. } => "DAEMON_CONNECTION_FAILED",
            DaemonClientError::DaemonError { .. } => "DAEMON_ERROR",
            DaemonClientError::ProtocolError { .. } => "DAEMON_PROTOCOL_ERROR",
            DaemonClientError::Io(_) => "DAEMON_IO_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(self, DaemonClientError::NotRunning { .. })
    }
}

impl From<IpcError> for DaemonClientError {
    fn from(e: IpcError) -> Self {
        match e {
            IpcError::NotRunning { path } => DaemonClientError::NotRunning { path },
            IpcError::ConnectionFailed(io) => DaemonClientError::ConnectionFailed {
                message: io.to_string(),
            },
            IpcError::DaemonError { code, message } => {
                DaemonClientError::DaemonError { code, message }
            }
            IpcError::ProtocolError { message } => DaemonClientError::ProtocolError { message },
            IpcError::Io(io) => DaemonClientError::Io(io),
            other => DaemonClientError::ProtocolError {
                message: other.to_string(),
            },
        }
    }
}

/// Parameters for creating a daemon-managed PTY session.
///
/// The daemon is a pure PTY manager. It does NOT know about git worktrees,
/// agents, or kild sessions. The caller (kild-core session handler) is
/// responsible for worktree creation and session file persistence.
#[derive(Debug, Clone)]
pub struct DaemonCreateRequest<'a> {
    /// Unique request ID for response correlation.
    pub request_id: &'a str,
    /// Unique daemon session identifier (e.g. "myapp_feature-auth_0").
    /// Each agent spawn gets its own daemon session via `compute_spawn_id()`.
    pub session_id: &'a str,
    /// Working directory for the PTY process.
    pub working_directory: &'a Path,
    /// Command to execute in the PTY.
    pub command: &'a str,
    /// Arguments for the command.
    pub args: &'a [String],
    /// Environment variables to set for the PTY process.
    pub env_vars: &'a [(String, String)],
    /// Initial PTY rows.
    pub rows: u16,
    /// Initial PTY columns.
    pub cols: u16,
    /// When true, use native login shell (`CommandBuilder::new_default_prog()`)
    /// instead of executing the command directly. Used for bare shell sessions.
    pub use_login_shell: bool,
}

/// Create a new PTY session in the daemon.
///
/// Sends a `create_session` JSONL message to the daemon via unix socket.
/// Blocks until the daemon responds with session ID or error.
pub fn create_pty_session(
    request: &DaemonCreateRequest<'_>,
) -> Result<DaemonCreateResult, DaemonClientError> {
    info!(
        event = "core.daemon.create_pty_session_started",
        request_id = request.request_id,
        session_id = request.session_id,
        command = request.command,
    );

    let msg = ClientMessage::CreateSession {
        id: request.request_id.to_string(),
        session_id: SessionId::new(request.session_id),
        working_directory: request.working_directory.to_string_lossy().to_string(),
        command: request.command.to_string(),
        args: request.args.to_vec(),
        env_vars: request.env_vars.iter().cloned().collect(),
        rows: request.rows,
        cols: request.cols,
        use_login_shell: request.use_login_shell,
    };

    let mut conn = get_connection()?;
    let response = conn.send(&msg);

    match response {
        Ok(DaemonMessage::SessionCreated { session, .. }) => {
            return_connection(conn);
            info!(
                event = "core.daemon.create_pty_session_completed",
                daemon_session_id = %session.id
            );
            Ok(DaemonCreateResult {
                daemon_session_id: session.id.into_inner(),
            })
        }
        Ok(_) => Err(DaemonClientError::ProtocolError {
            message: "Expected SessionCreated response".to_string(),
        }),
        Err(IpcError::DaemonError { code, message }) => {
            return_connection(conn);
            Err(DaemonClientError::DaemonError { code, message })
        }
        Err(e) => {
            warn!(
                event = "core.daemon.create_pty_session_failed",
                request_id = request.request_id,
                error = %e,
            );
            Err(e.into())
        }
    }
}

/// Stop a daemon-managed session (kill the PTY process).
pub fn stop_daemon_session(daemon_session_id: &str) -> Result<(), DaemonClientError> {
    info!(
        event = "core.daemon.stop_session_started",
        daemon_session_id = daemon_session_id
    );

    let request = ClientMessage::StopSession {
        id: format!("stop-{}", daemon_session_id),
        session_id: SessionId::new(daemon_session_id),
    };

    let mut conn = get_connection()?;
    match conn.send(&request) {
        Ok(_) => {
            return_connection(conn);
            info!(
                event = "core.daemon.stop_session_completed",
                daemon_session_id = daemon_session_id
            );
            Ok(())
        }
        Err(IpcError::DaemonError { code, message }) => {
            return_connection(conn);
            Err(DaemonClientError::DaemonError { code, message })
        }
        Err(e) => {
            warn!(
                event = "core.daemon.stop_session_failed",
                daemon_session_id = daemon_session_id,
                error = %e,
            );
            Err(e.into())
        }
    }
}

/// Destroy a daemon-managed session (kill the PTY process and remove session state).
pub fn destroy_daemon_session(
    daemon_session_id: &str,
    force: bool,
) -> Result<(), DaemonClientError> {
    info!(
        event = "core.daemon.destroy_session_started",
        daemon_session_id = daemon_session_id,
        force = force,
    );

    let request = ClientMessage::DestroySession {
        id: format!("destroy-{}", daemon_session_id),
        session_id: SessionId::new(daemon_session_id),
        force,
    };

    let mut conn = get_connection()?;
    match conn.send(&request) {
        Ok(_) => {
            return_connection(conn);
            info!(
                event = "core.daemon.destroy_session_completed",
                daemon_session_id = daemon_session_id,
            );
            Ok(())
        }
        Err(IpcError::DaemonError { code, message }) => {
            return_connection(conn);
            Err(DaemonClientError::DaemonError { code, message })
        }
        Err(e) => {
            warn!(
                event = "core.daemon.destroy_session_failed",
                daemon_session_id = daemon_session_id,
                error = %e,
            );
            Err(e.into())
        }
    }
}

/// Check if the daemon is running and responsive.
pub fn ping_daemon() -> Result<bool, DaemonClientError> {
    debug!(event = "core.daemon.ping_started");

    let request = ClientMessage::Ping {
        id: "ping".to_string(),
    };

    let mut conn = match get_connection() {
        Ok(c) => c,
        Err(DaemonClientError::NotRunning { .. }) => return Ok(false),
        Err(e) => return Err(e),
    };

    conn.set_read_timeout(Some(Duration::from_secs(2)))?;

    match conn.send(&request) {
        Ok(_) => {
            return_connection(conn);
            debug!(event = "core.daemon.ping_completed", alive = true);
            Ok(true)
        }
        Err(e) => {
            warn!(event = "core.daemon.ping_failed", error = %e);
            debug!(event = "core.daemon.ping_completed", alive = false);
            Ok(false)
        }
    }
}

/// Query the daemon for a session's current status.
///
/// Returns `Ok(Some(SessionStatus))` if the daemon is reachable and knows
/// about this session. Returns `Ok(None)` if the daemon is not running or the
/// session is not found in the daemon.
/// Returns `Err(...)` for unexpected failures (connection errors, protocol errors).
pub fn get_session_status(
    daemon_session_id: &str,
) -> Result<Option<SessionStatus>, DaemonClientError> {
    debug!(
        event = "core.daemon.get_session_status_started",
        daemon_session_id = daemon_session_id
    );

    let request = ClientMessage::GetSession {
        id: format!("status-{}", daemon_session_id),
        session_id: SessionId::new(daemon_session_id),
    };

    let mut conn = match get_connection() {
        Ok(c) => c,
        Err(DaemonClientError::NotRunning { .. }) => {
            debug!(
                event = "core.daemon.get_session_status_completed",
                daemon_session_id = daemon_session_id,
                result = "daemon_not_running"
            );
            return Ok(None);
        }
        Err(e) => {
            return Err(e);
        }
    };

    conn.set_read_timeout(Some(Duration::from_secs(2)))?;

    match conn.send(&request) {
        Ok(DaemonMessage::SessionInfo { session, .. }) => {
            return_connection(conn);
            debug!(
                event = "core.daemon.get_session_status_completed",
                daemon_session_id = daemon_session_id,
                status = %session.status
            );
            Ok(Some(session.status))
        }
        Ok(unexpected) => {
            warn!(
                event = "core.daemon.get_session_status_failed",
                daemon_session_id = daemon_session_id,
                response = ?unexpected,
                "Unexpected response type from daemon"
            );
            Err(DaemonClientError::ProtocolError {
                message: "Expected SessionInfo response".to_string(),
            })
        }
        Err(IpcError::DaemonError { ref code, .. }) if *code == ErrorCode::SessionNotFound => {
            return_connection(conn);
            debug!(
                event = "core.daemon.get_session_status_completed",
                daemon_session_id = daemon_session_id,
                result = "session_not_found"
            );
            Ok(None)
        }
        Err(e) => {
            let err: DaemonClientError = e.into();
            warn!(
                event = "core.daemon.get_session_status_failed",
                daemon_session_id = daemon_session_id,
                error = %err
            );
            Err(err)
        }
    }
}

/// Query the daemon for a session's status and exit code.
///
/// Returns `(status, exit_code)` if the daemon knows about this session.
/// Returns `Ok(None)` if the daemon is not running or the session is not found.
pub fn get_session_info(
    daemon_session_id: &str,
) -> Result<Option<(SessionStatus, Option<i32>)>, DaemonClientError> {
    let request = ClientMessage::GetSession {
        id: format!("info-{}", daemon_session_id),
        session_id: SessionId::new(daemon_session_id),
    };

    let mut conn = match get_connection() {
        Ok(c) => c,
        Err(DaemonClientError::NotRunning { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };

    conn.set_read_timeout(Some(Duration::from_secs(2)))?;

    match conn.send(&request) {
        Ok(DaemonMessage::SessionInfo { session, .. }) => {
            return_connection(conn);
            Ok(Some((session.status, session.exit_code)))
        }
        Ok(unexpected) => {
            warn!(
                event = "core.daemon.get_session_info_failed",
                daemon_session_id = daemon_session_id,
                response = ?unexpected,
                "Unexpected response type from daemon"
            );
            Err(DaemonClientError::ProtocolError {
                message: "Expected SessionInfo response".to_string(),
            })
        }
        Err(IpcError::DaemonError { ref code, .. }) if *code == ErrorCode::SessionNotFound => {
            return_connection(conn);
            Ok(None)
        }
        Err(e) => {
            warn!(
                event = "core.daemon.get_session_info_failed",
                daemon_session_id = daemon_session_id,
                error = %e,
            );
            Err(e.into())
        }
    }
}

/// Read the scrollback buffer from a daemon session.
///
/// Returns the raw scrollback bytes (decoded from base64), or `None` if the
/// daemon is not running or the session is not found.
pub fn read_scrollback(daemon_session_id: &str) -> Result<Option<Vec<u8>>, DaemonClientError> {
    let request = ClientMessage::ReadScrollback {
        id: format!("scrollback-{}", daemon_session_id),
        session_id: SessionId::new(daemon_session_id),
    };

    let mut conn = match get_connection() {
        Ok(c) => c,
        Err(DaemonClientError::NotRunning { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };

    conn.set_read_timeout(Some(Duration::from_secs(2)))?;

    match conn.send(&request) {
        Ok(DaemonMessage::ScrollbackContents { data, .. }) => {
            return_connection(conn);
            use base64::Engine;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|e| DaemonClientError::ProtocolError {
                    message: format!("Invalid base64 in scrollback response: {}", e),
                })?;
            Ok(Some(decoded))
        }
        Ok(unexpected) => {
            warn!(
                event = "core.daemon.read_scrollback_failed",
                daemon_session_id = daemon_session_id,
                response = ?unexpected,
                "Unexpected response type from daemon"
            );
            Err(DaemonClientError::ProtocolError {
                message: "Expected ScrollbackContents response".to_string(),
            })
        }
        Err(IpcError::DaemonError { ref code, .. }) if *code == ErrorCode::SessionNotFound => {
            return_connection(conn);
            Ok(None)
        }
        Err(e) => {
            warn!(
                event = "core.daemon.read_scrollback_failed",
                daemon_session_id = daemon_session_id,
                error = %e,
            );
            Err(e.into())
        }
    }
}

/// List all daemon sessions.
///
/// Returns all sessions from the daemon. The caller can filter by prefix
/// to find sessions belonging to a specific kild (e.g., UI-created shells).
pub fn list_daemon_sessions() -> Result<Vec<kild_protocol::SessionInfo>, DaemonClientError> {
    debug!(event = "core.daemon.list_sessions_started");

    let request = ClientMessage::ListSessions {
        id: "list-sessions".to_string(),
        project_id: None,
    };

    let mut conn = get_connection()?;

    match conn.send(&request) {
        Ok(DaemonMessage::SessionList { sessions, .. }) => {
            return_connection(conn);
            debug!(
                event = "core.daemon.list_sessions_completed",
                count = sessions.len()
            );
            Ok(sessions)
        }
        Ok(_) => Err(DaemonClientError::ProtocolError {
            message: "Expected SessionList response".to_string(),
        }),
        Err(IpcError::DaemonError { code, message }) => {
            return_connection(conn);
            Err(DaemonClientError::DaemonError { code, message })
        }
        Err(e) => {
            warn!(
                event = "core.daemon.list_sessions_failed",
                error = %e,
            );
            Err(e.into())
        }
    }
}

/// Request the daemon to shut down gracefully.
pub fn request_shutdown() -> Result<(), DaemonClientError> {
    info!(event = "core.daemon.shutdown_started");

    let request = ClientMessage::DaemonStop {
        id: "shutdown".to_string(),
    };

    let mut conn = get_connection()?;
    match conn.send(&request) {
        Ok(_) => {
            // Don't return connection — daemon is shutting down
            info!(event = "core.daemon.shutdown_completed");
            Ok(())
        }
        Err(IpcError::DaemonError { code, message }) => {
            Err(DaemonClientError::DaemonError { code, message })
        }
        Err(e) => {
            warn!(event = "core.daemon.shutdown_failed", error = %e);
            Err(e.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_returns_not_running_for_missing_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("daemon.sock");

        let result = IpcConnection::connect(&socket_path);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), IpcError::NotRunning { .. }),
            "Should return NotRunning when daemon socket doesn't exist"
        );
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(
            DaemonClientError::NotRunning {
                path: "/tmp/test.sock".to_string()
            }
            .error_code(),
            "DAEMON_NOT_RUNNING"
        );
        assert_eq!(
            DaemonClientError::ConnectionFailed {
                message: "refused".to_string()
            }
            .error_code(),
            "DAEMON_CONNECTION_FAILED"
        );
        assert_eq!(
            DaemonClientError::DaemonError {
                code: ErrorCode::SessionNotFound,
                message: "internal".to_string()
            }
            .error_code(),
            "DAEMON_ERROR"
        );
        assert_eq!(
            DaemonClientError::ProtocolError {
                message: "bad json".to_string()
            }
            .error_code(),
            "DAEMON_PROTOCOL_ERROR"
        );
        assert_eq!(
            DaemonClientError::Io(std::io::Error::new(std::io::ErrorKind::Other, "test"))
                .error_code(),
            "DAEMON_IO_ERROR"
        );
    }

    #[test]
    fn test_is_user_error() {
        assert!(
            DaemonClientError::NotRunning {
                path: "/tmp/test.sock".to_string()
            }
            .is_user_error()
        );

        assert!(
            !DaemonClientError::ConnectionFailed {
                message: "refused".to_string()
            }
            .is_user_error()
        );
        assert!(
            !DaemonClientError::DaemonError {
                code: ErrorCode::SessionNotFound,
                message: "internal".to_string()
            }
            .is_user_error()
        );
        assert!(
            !DaemonClientError::ProtocolError {
                message: "bad json".to_string()
            }
            .is_user_error()
        );
        assert!(
            !DaemonClientError::Io(std::io::Error::new(std::io::ErrorKind::Other, "test"))
                .is_user_error()
        );
    }

    #[test]
    fn test_from_ipc_error_not_running() {
        let ipc_err = IpcError::NotRunning {
            path: "/tmp/test.sock".to_string(),
        };
        let daemon_err: DaemonClientError = ipc_err.into();
        assert!(
            matches!(daemon_err, DaemonClientError::NotRunning { path } if path == "/tmp/test.sock")
        );
    }

    #[test]
    fn test_from_ipc_error_daemon_error() {
        let ipc_err = IpcError::DaemonError {
            code: ErrorCode::SessionNotFound,
            message: "not found".to_string(),
        };
        let daemon_err: DaemonClientError = ipc_err.into();
        assert!(
            matches!(daemon_err, DaemonClientError::DaemonError { code, message }
            if code == ErrorCode::SessionNotFound && message == "not found")
        );
    }

    #[test]
    fn test_from_ipc_error_connection_failed() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied");
        let ipc_err = IpcError::ConnectionFailed(io_err);
        let daemon_err: DaemonClientError = ipc_err.into();
        assert!(
            matches!(daemon_err, DaemonClientError::ConnectionFailed { message }
            if message.contains("permission denied"))
        );
    }

    #[test]
    fn test_from_ipc_error_protocol_error() {
        let ipc_err = IpcError::ProtocolError {
            message: "bad format".to_string(),
        };
        let daemon_err: DaemonClientError = ipc_err.into();
        assert!(
            matches!(daemon_err, DaemonClientError::ProtocolError { message }
            if message == "bad format")
        );
    }

    #[test]
    fn test_from_ipc_error_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");
        let ipc_err = IpcError::Io(io_err);
        let daemon_err: DaemonClientError = ipc_err.into();
        assert!(matches!(daemon_err, DaemonClientError::Io(_)));
    }
}
