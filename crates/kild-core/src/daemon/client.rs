//! Synchronous IPC client for communicating with the KILD daemon.
//!
//! Uses `std::os::unix::net::UnixStream` — no tokio dependency.
//! Uses typed `ClientMessage`/`DaemonMessage` enums from `kild-protocol`
//! for compile-checked protocol correctness.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use kild_protocol::{ClientMessage, DaemonMessage, ErrorCode, SessionStatus};
use tracing::{debug, info, warn};

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

/// Connect to the daemon socket with a timeout.
fn connect(socket_path: &Path) -> Result<UnixStream, DaemonClientError> {
    if !socket_path.exists() {
        return Err(DaemonClientError::NotRunning {
            path: socket_path.display().to_string(),
        });
    }

    let stream = UnixStream::connect(socket_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::ConnectionRefused {
            DaemonClientError::NotRunning {
                path: socket_path.display().to_string(),
            }
        } else {
            DaemonClientError::ConnectionFailed {
                message: e.to_string(),
            }
        }
    })?;

    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    Ok(stream)
}

/// Send a typed request and read one typed response over a JSONL connection.
fn send_request(
    stream: &mut UnixStream,
    request: &ClientMessage,
) -> Result<DaemonMessage, DaemonClientError> {
    let msg = serde_json::to_string(request).map_err(|e| DaemonClientError::ProtocolError {
        message: e.to_string(),
    })?;

    writeln!(stream, "{}", msg)?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    if line.is_empty() {
        return Err(DaemonClientError::ProtocolError {
            message: "Empty response from daemon".to_string(),
        });
    }

    let response: DaemonMessage =
        serde_json::from_str(&line).map_err(|e| DaemonClientError::ProtocolError {
            message: format!("Invalid JSON response: {}", e),
        })?;

    // Check for error responses
    if let DaemonMessage::Error { code, message, .. } = response {
        return Err(DaemonClientError::DaemonError { code, message });
    }

    Ok(response)
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
    let socket_path = crate::daemon::socket_path();

    info!(
        event = "core.daemon.create_pty_session_started",
        request_id = request.request_id,
        session_id = request.session_id,
        command = request.command,
    );

    let msg = ClientMessage::CreateSession {
        id: request.request_id.to_string(),
        session_id: request.session_id.to_string(),
        working_directory: request.working_directory.to_string_lossy().to_string(),
        command: request.command.to_string(),
        args: request.args.to_vec(),
        env_vars: request.env_vars.iter().cloned().collect(),
        rows: request.rows,
        cols: request.cols,
        use_login_shell: request.use_login_shell,
    };

    let mut stream = connect(&socket_path)?;
    let response = send_request(&mut stream, &msg)?;

    let session_id = match response {
        DaemonMessage::SessionCreated { session, .. } => session.id,
        _ => {
            return Err(DaemonClientError::ProtocolError {
                message: "Expected SessionCreated response".to_string(),
            });
        }
    };

    info!(
        event = "core.daemon.create_pty_session_completed",
        daemon_session_id = session_id
    );

    Ok(DaemonCreateResult {
        daemon_session_id: session_id,
    })
}

/// Stop a daemon-managed session (kill the PTY process).
pub fn stop_daemon_session(daemon_session_id: &str) -> Result<(), DaemonClientError> {
    let socket_path = crate::daemon::socket_path();

    info!(
        event = "core.daemon.stop_session_started",
        daemon_session_id = daemon_session_id
    );

    let request = ClientMessage::StopSession {
        id: format!("stop-{}", daemon_session_id),
        session_id: daemon_session_id.to_string(),
    };

    let mut stream = connect(&socket_path)?;
    send_request(&mut stream, &request)?;

    info!(
        event = "core.daemon.stop_session_completed",
        daemon_session_id = daemon_session_id
    );

    Ok(())
}

/// Destroy a daemon-managed session (kill the PTY process and remove session state).
pub fn destroy_daemon_session(
    daemon_session_id: &str,
    force: bool,
) -> Result<(), DaemonClientError> {
    let socket_path = crate::daemon::socket_path();

    info!(
        event = "core.daemon.destroy_session_started",
        daemon_session_id = daemon_session_id,
        force = force,
    );

    let request = ClientMessage::DestroySession {
        id: format!("destroy-{}", daemon_session_id),
        session_id: daemon_session_id.to_string(),
        force,
    };

    let mut stream = connect(&socket_path)?;
    send_request(&mut stream, &request)?;

    info!(
        event = "core.daemon.destroy_session_completed",
        daemon_session_id = daemon_session_id,
    );

    Ok(())
}

/// Check if the daemon is running and responsive.
pub fn ping_daemon() -> Result<bool, DaemonClientError> {
    let socket_path = crate::daemon::socket_path();

    debug!(event = "core.daemon.ping_started");

    let request = ClientMessage::Ping {
        id: "ping".to_string(),
    };

    let mut stream = match connect(&socket_path) {
        Ok(s) => s,
        Err(DaemonClientError::NotRunning { .. }) => return Ok(false),
        Err(e) => return Err(e),
    };

    // Use a short timeout for ping
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;

    match send_request(&mut stream, &request) {
        Ok(_) => {
            debug!(event = "core.daemon.ping_completed", alive = true);
            Ok(true)
        }
        Err(_) => {
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
    let socket_path = crate::daemon::socket_path();

    debug!(
        event = "core.daemon.get_session_status_started",
        daemon_session_id = daemon_session_id
    );

    let request = ClientMessage::GetSession {
        id: format!("status-{}", daemon_session_id),
        session_id: daemon_session_id.to_string(),
    };

    let mut stream = match connect(&socket_path) {
        Ok(s) => s,
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

    // Use a short timeout for status queries
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;

    match send_request(&mut stream, &request) {
        Ok(DaemonMessage::SessionInfo { session, .. }) => {
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
        Err(DaemonClientError::DaemonError { ref code, .. })
            if *code == ErrorCode::SessionNotFound =>
        {
            debug!(
                event = "core.daemon.get_session_status_completed",
                daemon_session_id = daemon_session_id,
                result = "session_not_found"
            );
            Ok(None)
        }
        Err(e) => {
            warn!(
                event = "core.daemon.get_session_status_failed",
                daemon_session_id = daemon_session_id,
                error = %e
            );
            Err(e)
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
    let socket_path = crate::daemon::socket_path();

    let request = ClientMessage::GetSession {
        id: format!("info-{}", daemon_session_id),
        session_id: daemon_session_id.to_string(),
    };

    let mut stream = match connect(&socket_path) {
        Ok(s) => s,
        Err(DaemonClientError::NotRunning { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };

    stream.set_read_timeout(Some(Duration::from_secs(2)))?;

    match send_request(&mut stream, &request) {
        Ok(DaemonMessage::SessionInfo { session, .. }) => {
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
        Err(DaemonClientError::DaemonError { ref code, .. })
            if *code == ErrorCode::SessionNotFound =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// Read the scrollback buffer from a daemon session.
///
/// Returns the raw scrollback bytes (decoded from base64), or `None` if the
/// daemon is not running or the session is not found.
pub fn read_scrollback(daemon_session_id: &str) -> Result<Option<Vec<u8>>, DaemonClientError> {
    let socket_path = crate::daemon::socket_path();

    let request = ClientMessage::ReadScrollback {
        id: format!("scrollback-{}", daemon_session_id),
        session_id: daemon_session_id.to_string(),
    };

    let mut stream = match connect(&socket_path) {
        Ok(s) => s,
        Err(DaemonClientError::NotRunning { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };

    stream.set_read_timeout(Some(Duration::from_secs(2)))?;

    match send_request(&mut stream, &request) {
        Ok(DaemonMessage::ScrollbackContents { data, .. }) => {
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
        Err(DaemonClientError::DaemonError { ref code, .. })
            if *code == ErrorCode::SessionNotFound =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// Request the daemon to shut down gracefully.
pub fn request_shutdown() -> Result<(), DaemonClientError> {
    let socket_path = crate::daemon::socket_path();

    info!(event = "core.daemon.shutdown_started");

    let request = ClientMessage::DaemonStop {
        id: "shutdown".to_string(),
    };

    let mut stream = connect(&socket_path)?;
    send_request(&mut stream, &request)?;

    info!(event = "core.daemon.shutdown_completed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_returns_not_running_for_missing_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("daemon.sock");

        let result = connect(&socket_path);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), DaemonClientError::NotRunning { .. }),
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
}
