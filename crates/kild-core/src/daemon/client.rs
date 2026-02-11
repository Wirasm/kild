//! Synchronous IPC client for communicating with the KILD daemon.
//!
//! Uses `std::os::unix::net::UnixStream` — no tokio dependency.
//! Constructs JSONL messages manually with `serde_json::json!()` to avoid
//! importing types from kild-daemon (which depends on kild-core).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use tracing::{debug, info, warn};

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

    #[error("Daemon returned error: {message}")]
    DaemonError { message: String },

    #[error("IPC protocol error: {message}")]
    ProtocolError { message: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
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

/// Send a request and read one response over a JSONL connection.
///
/// Creates a new BufReader per call. This is safe because public methods in
/// this module open fresh connections for each operation. Do NOT reuse a stream
/// across multiple `send_request` calls — BufReader's internal buffer would
/// consume data meant for subsequent reads.
fn send_request(
    stream: &mut UnixStream,
    request: serde_json::Value,
) -> Result<serde_json::Value, DaemonClientError> {
    let msg = serde_json::to_string(&request).map_err(|e| DaemonClientError::ProtocolError {
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

    let response: serde_json::Value =
        serde_json::from_str(&line).map_err(|e| DaemonClientError::ProtocolError {
            message: format!("Invalid JSON response: {}", e),
        })?;

    // Check for error responses
    if response.get("type").and_then(|t| t.as_str()) == Some("error") {
        let code = response
            .get("code")
            .and_then(|c| c.as_str())
            .unwrap_or("unknown");
        let message = response
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown daemon error");
        return Err(DaemonClientError::DaemonError {
            message: format!("[{}] {}", code, message),
        });
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
    /// Session identifier (e.g. "myapp_feature-auth").
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

    let env_map: serde_json::Map<String, serde_json::Value> = request
        .env_vars
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();

    let msg = serde_json::json!({
        "id": request.request_id,
        "type": "create_session",
        "session_id": request.session_id,
        "working_directory": request.working_directory.to_string_lossy(),
        "command": request.command,
        "args": request.args,
        "env_vars": env_map,
        "rows": request.rows,
        "cols": request.cols,
        "use_login_shell": request.use_login_shell,
    });

    let mut stream = connect(&socket_path)?;
    let response = send_request(&mut stream, msg)?;

    let session_id = response
        .get("session")
        .and_then(|s| s.get("id"))
        .and_then(|id| id.as_str())
        .ok_or_else(|| DaemonClientError::ProtocolError {
            message: "Response missing session.id field".to_string(),
        })?
        .to_string();

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

    let request = serde_json::json!({
        "id": format!("stop-{}", daemon_session_id),
        "type": "stop_session",
        "session_id": daemon_session_id,
    });

    let mut stream = connect(&socket_path)?;
    send_request(&mut stream, request)?;

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

    let request = serde_json::json!({
        "id": format!("destroy-{}", daemon_session_id),
        "type": "destroy_session",
        "session_id": daemon_session_id,
        "force": force,
    });

    let mut stream = connect(&socket_path)?;
    send_request(&mut stream, request)?;

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

    let request = serde_json::json!({
        "id": "ping",
        "type": "ping",
    });

    let mut stream = match connect(&socket_path) {
        Ok(s) => s,
        Err(DaemonClientError::NotRunning { .. }) => return Ok(false),
        Err(e) => return Err(e),
    };

    // Use a short timeout for ping
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;

    match send_request(&mut stream, request) {
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
/// Returns `Ok(Some("running"))` or `Ok(Some("stopped"))` if the daemon is
/// reachable and knows about this session. Returns `Ok(None)` if the daemon
/// is not running or the session is not found in the daemon.
/// Returns `Err(...)` for unexpected failures (connection errors, protocol errors).
pub fn get_session_status(daemon_session_id: &str) -> Result<Option<String>, DaemonClientError> {
    let socket_path = crate::daemon::socket_path();

    debug!(
        event = "core.daemon.get_session_status_started",
        daemon_session_id = daemon_session_id
    );

    let request = serde_json::json!({
        "id": format!("status-{}", daemon_session_id),
        "type": "get_session",
        "session_id": daemon_session_id,
    });

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

    match send_request(&mut stream, request) {
        Ok(response) => {
            // Response is DaemonMessage::SessionInfo { session: { status: "running"|"stopped" } }
            let status = response
                .get("session")
                .and_then(|s| s.get("status"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());

            debug!(
                event = "core.daemon.get_session_status_completed",
                daemon_session_id = daemon_session_id,
                status = ?status
            );

            Ok(status)
        }
        Err(DaemonClientError::DaemonError { ref message })
            if message.contains("not_found") || message.contains("unknown_session") =>
        {
            // Session not found in daemon — expected when session was cleaned up
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
) -> Result<Option<(String, Option<i32>)>, DaemonClientError> {
    let socket_path = crate::daemon::socket_path();

    let request = serde_json::json!({
        "id": format!("info-{}", daemon_session_id),
        "type": "get_session",
        "session_id": daemon_session_id,
    });

    let mut stream = match connect(&socket_path) {
        Ok(s) => s,
        Err(DaemonClientError::NotRunning { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };

    stream.set_read_timeout(Some(Duration::from_secs(2)))?;

    match send_request(&mut stream, request) {
        Ok(response) => {
            let session = response.get("session");
            let status = session
                .and_then(|s| s.get("status"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            let exit_code = session
                .and_then(|s| s.get("exit_code"))
                .and_then(|c| c.as_i64())
                .map(|c| c as i32);
            match status {
                Some(s) => Ok(Some((s, exit_code))),
                None => Ok(None),
            }
        }
        Err(DaemonClientError::DaemonError { ref message })
            if message.contains("not_found") || message.contains("unknown_session") =>
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

    let request = serde_json::json!({
        "id": format!("scrollback-{}", daemon_session_id),
        "type": "read_scrollback",
        "session_id": daemon_session_id,
    });

    let mut stream = match connect(&socket_path) {
        Ok(s) => s,
        Err(DaemonClientError::NotRunning { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };

    stream.set_read_timeout(Some(Duration::from_secs(2)))?;

    match send_request(&mut stream, request) {
        Ok(response) => {
            let data = response.get("data").and_then(|d| d.as_str()).unwrap_or("");
            use base64::Engine;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(data)
                .unwrap_or_default();
            Ok(Some(decoded))
        }
        Err(DaemonClientError::DaemonError { ref message })
            if message.contains("not_found") || message.contains("unknown_session") =>
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

    let request = serde_json::json!({
        "id": "shutdown",
        "type": "daemon_stop",
    });

    let mut stream = connect(&socket_path)?;
    send_request(&mut stream, request)?;

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
}
