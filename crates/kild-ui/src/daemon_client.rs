//! Async daemon client using smol on GPUI's BackgroundExecutor.
//!
//! Provides IPC operations for communicating with the kild daemon:
//! - `ping_daemon_async()` — check if daemon is running and responsive
//! - `list_sessions_async()` / `find_first_running_session()` — session discovery
//! - `get_session_async()` — query a single session by ID
//! - `stop_session_async()` — stop a running daemon session
//! - `connect_for_attach()` — two-connection attach for streaming PTY output
//! - `send_write_stdin()` / `send_resize()` / `send_detach()` — write operations

use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
use kild_protocol::{
    ClientMessage, DaemonMessage, ErrorCode, SessionId, SessionInfo, SessionStatus,
};
use smol::Async;
use smol::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use thiserror::Error;
use tracing::{debug, error, info, warn};

/// Monotonic counter for generating unique request IDs within this process.
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_request_id() -> String {
    let n = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("ui-{n}")
}

#[derive(Debug, Error)]
pub enum DaemonClientError {
    #[error("failed to connect to daemon: {0}")]
    Connect(std::io::Error),

    #[error("failed to serialize request: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("daemon closed connection (EOF)")]
    ConnectionClosed,

    #[error("daemon sent empty response")]
    EmptyResponse,

    #[error("invalid JSON from daemon: {source}: {json}")]
    InvalidJson {
        source: serde_json::Error,
        json: String,
    },

    #[error("unexpected response from daemon: {0:?}")]
    UnexpectedResponse(DaemonMessage),

    #[error("daemon error ({code}): {message}")]
    DaemonError { code: String, message: String },

    #[allow(dead_code)]
    #[error("no running daemon session found")]
    SessionNotFound,

    #[error("base64 decode failed: {0}")]
    Base64Decode(#[from] base64::DecodeError),
}

/// Connect to the daemon socket, returning an async stream.
///
/// Shared connection logic for all IPC operations.
async fn connect_to_daemon() -> Result<Async<UnixStream>, DaemonClientError> {
    let socket_path = kild_core::daemon::socket_path();
    if !socket_path.exists() {
        return Err(DaemonClientError::Connect(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "daemon socket not found",
        )));
    }
    Async::<UnixStream>::connect(&socket_path)
        .await
        .map_err(DaemonClientError::Connect)
}

/// Send a JSONL message on a stream without flushing.
///
/// For write-heavy operations (WriteStdin, ResizePty) where the caller
/// doesn't need an immediate response. Callers expecting a response should
/// use `send_message_flush()` to ensure the message reaches the peer.
async fn send_message(
    stream: &mut Async<UnixStream>,
    msg: &ClientMessage,
) -> Result<(), DaemonClientError> {
    let json = serde_json::to_string(msg)?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    Ok(())
}

/// Send a JSONL message and flush immediately.
///
/// For request-response patterns where a read follows the write.
async fn send_message_flush(
    stream: &mut Async<UnixStream>,
    msg: &ClientMessage,
) -> Result<(), DaemonClientError> {
    send_message(stream, msg).await?;
    stream.flush().await?;
    Ok(())
}

/// Read one JSONL line and parse as DaemonMessage.
async fn read_response(
    reader: &mut BufReader<Async<UnixStream>>,
) -> Result<DaemonMessage, DaemonClientError> {
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        return Err(DaemonClientError::ConnectionClosed);
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(DaemonClientError::EmptyResponse);
    }
    serde_json::from_str(trimmed).map_err(|e| DaemonClientError::InvalidJson {
        source: e,
        json: trimmed.to_string(),
    })
}

/// Async ping to the kild daemon via smol.
///
/// Returns `Ok(true)` if daemon responded with Ack, `Ok(false)` if daemon
/// is not running (socket missing or connection refused), `Err` for
/// unexpected failures.
pub async fn ping_daemon_async() -> Result<bool, DaemonClientError> {
    let socket_path = kild_core::daemon::socket_path();

    debug!(event = "ui.daemon.ping_started");

    if !socket_path.exists() {
        info!(
            event = "ui.daemon.ping_completed",
            result = "socket_missing"
        );
        return Ok(false);
    }

    let mut stream = match Async::<UnixStream>::connect(&socket_path).await {
        Ok(s) => s,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::ConnectionRefused {
                info!(
                    event = "ui.daemon.ping_completed",
                    result = "connection_refused"
                );
                return Ok(false);
            }
            error!(
                event = "ui.daemon.ping_failed",
                error = %e,
            );
            return Err(DaemonClientError::Connect(e));
        }
    };

    let request = ClientMessage::Ping {
        id: next_request_id(),
    };

    send_message_flush(&mut stream, &request).await?;

    // Read Ack response (hand ownership to BufReader — done writing)
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        return Err(DaemonClientError::ConnectionClosed);
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(DaemonClientError::EmptyResponse);
    }
    let response: DaemonMessage =
        serde_json::from_str(trimmed).map_err(|e| DaemonClientError::InvalidJson {
            source: e,
            json: trimmed.to_string(),
        })?;

    match response {
        DaemonMessage::Ack { .. } => {
            info!(event = "ui.daemon.ping_completed", result = "ack");
            Ok(true)
        }
        other => {
            warn!(
                event = "ui.daemon.ping_completed",
                result = "unexpected_response",
                response = ?other,
            );
            Err(DaemonClientError::UnexpectedResponse(other))
        }
    }
}

/// List all daemon sessions.
#[allow(dead_code)]
pub async fn list_sessions_async() -> Result<Vec<SessionInfo>, DaemonClientError> {
    debug!(event = "ui.daemon.list_sessions_started");

    let mut stream = connect_to_daemon().await?;
    let request = ClientMessage::ListSessions {
        id: next_request_id(),
        project_id: None,
    };
    send_message_flush(&mut stream, &request).await?;

    let mut reader = BufReader::new(stream);
    let response = read_response(&mut reader).await?;

    match response {
        DaemonMessage::SessionList { sessions, .. } => {
            info!(
                event = "ui.daemon.list_sessions_completed",
                count = sessions.len()
            );
            Ok(sessions)
        }
        DaemonMessage::Error { code, message, .. } => Err(DaemonClientError::DaemonError {
            code: code.to_string(),
            message,
        }),
        other => Err(DaemonClientError::UnexpectedResponse(other)),
    }
}

/// Find the first Running daemon session.
///
/// Temporary convenience for the Ctrl+D toggle flow. Phase 3 (layout shell)
/// replaces this with explicit sidebar-driven session selection.
#[allow(dead_code)]
pub async fn find_first_running_session() -> Result<SessionInfo, DaemonClientError> {
    let sessions = list_sessions_async().await?;
    sessions
        .into_iter()
        .find(|s| s.status == SessionStatus::Running)
        .ok_or(DaemonClientError::SessionNotFound)
}

/// Query a single daemon session by ID.
///
/// Returns `Ok(Some(session))` if found, `Ok(None)` if the session doesn't
/// exist in the daemon. Mirrors the sync client pattern from kild-core.
#[allow(dead_code)]
pub async fn get_session_async(session_id: &str) -> Result<Option<SessionInfo>, DaemonClientError> {
    debug!(
        event = "ui.daemon.get_session_started",
        session_id = session_id
    );

    let mut stream = connect_to_daemon().await?;
    let request = ClientMessage::GetSession {
        id: next_request_id(),
        session_id: SessionId::from(session_id),
    };
    send_message_flush(&mut stream, &request).await?;

    let mut reader = BufReader::new(stream);
    let response = read_response(&mut reader).await?;

    match response {
        DaemonMessage::SessionInfo { session, .. } => {
            info!(
                event = "ui.daemon.get_session_completed",
                session_id = session_id,
                status = %session.status
            );
            Ok(Some(session))
        }
        DaemonMessage::Error {
            code: ErrorCode::SessionNotFound,
            ..
        } => {
            info!(
                event = "ui.daemon.get_session_completed",
                session_id = session_id,
                result = "not_found"
            );
            Ok(None)
        }
        DaemonMessage::Error { code, message, .. } => Err(DaemonClientError::DaemonError {
            code: code.to_string(),
            message,
        }),
        other => Err(DaemonClientError::UnexpectedResponse(other)),
    }
}

/// Stop a running daemon session.
///
/// Sends a stop command to the daemon, which kills the PTY process.
/// Returns `Ok(())` on success, `Err` on failure.
pub async fn stop_session_async(session_id: &str) -> Result<(), DaemonClientError> {
    debug!(
        event = "ui.daemon.stop_session_started",
        session_id = session_id
    );

    let mut stream = connect_to_daemon().await?;
    let request = ClientMessage::StopSession {
        id: next_request_id(),
        session_id: SessionId::from(session_id),
    };
    send_message_flush(&mut stream, &request).await?;

    let mut reader = BufReader::new(stream);
    let response = read_response(&mut reader).await?;

    match response {
        DaemonMessage::Ack { .. } => {
            info!(
                event = "ui.daemon.stop_session_completed",
                session_id = session_id
            );
            Ok(())
        }
        DaemonMessage::Error { code, message, .. } => Err(DaemonClientError::DaemonError {
            code: code.to_string(),
            message,
        }),
        other => Err(DaemonClientError::UnexpectedResponse(other)),
    }
}

/// Create a new daemon session with a login shell in the given directory.
///
/// Returns the daemon session ID on success.
pub async fn create_session_async(
    session_id: &str,
    working_directory: &str,
) -> Result<String, DaemonClientError> {
    info!(
        event = "ui.daemon.create_session_started",
        session_id = session_id,
        working_directory = working_directory
    );

    let mut stream = connect_to_daemon().await?;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let request = ClientMessage::CreateSession {
        id: next_request_id(),
        session_id: SessionId::from(session_id),
        working_directory: working_directory.to_string(),
        command: shell,
        args: vec![],
        env_vars: std::collections::HashMap::new(),
        rows: 24,
        cols: 80,
        use_login_shell: true,
    };
    send_message_flush(&mut stream, &request).await?;

    let mut reader = BufReader::new(stream);
    let response = read_response(&mut reader).await?;

    match response {
        DaemonMessage::SessionCreated { session, .. } => {
            info!(
                event = "ui.daemon.create_session_completed",
                daemon_session_id = %session.id
            );
            Ok(session.id.into_inner())
        }
        DaemonMessage::Error { code, message, .. } => Err(DaemonClientError::DaemonError {
            code: code.to_string(),
            message,
        }),
        other => Err(DaemonClientError::UnexpectedResponse(other)),
    }
}

/// Two-connection handle for attached daemon session.
///
/// - `reader`: receives streaming PtyOutput messages after Attach
/// - `writer`: sends WriteStdin, ResizePty, Detach commands
///
/// Fields are private to enforce invariants established during construction
/// (reader is attached, session_id matches the attached session).
pub struct DaemonConnection {
    reader: BufReader<Async<UnixStream>>,
    writer: Async<UnixStream>,
    session_id: String,
}

impl DaemonConnection {
    /// Get the session ID for this connection.
    #[allow(dead_code)]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Consume the connection, returning its parts for use in reader/writer tasks.
    pub fn into_parts(self) -> (BufReader<Async<UnixStream>>, Async<UnixStream>, String) {
        (self.reader, self.writer, self.session_id)
    }
}

/// Open two connections to the daemon: one for streaming reads (Attach),
/// one for writes (WriteStdin/ResizePty/Detach).
pub async fn connect_for_attach(
    session_id: &str,
    rows: u16,
    cols: u16,
) -> Result<DaemonConnection, DaemonClientError> {
    info!(
        event = "ui.daemon.attach_started",
        session_id = session_id,
        rows = rows,
        cols = cols
    );

    // Connection 1: reader — send Attach, read Ack, then stream PtyOutput
    let mut reader_stream = connect_to_daemon().await?;
    let attach_request = ClientMessage::Attach {
        id: next_request_id(),
        session_id: SessionId::from(session_id),
        rows,
        cols,
    };
    send_message_flush(&mut reader_stream, &attach_request).await?;

    let mut reader = BufReader::new(reader_stream);
    let ack = read_response(&mut reader).await?;
    match ack {
        DaemonMessage::Ack { .. } => {
            info!(
                event = "ui.daemon.attach_ack_received",
                session_id = session_id
            );
        }
        DaemonMessage::Error { code, message, .. } => {
            return Err(DaemonClientError::DaemonError {
                code: code.to_string(),
                message,
            });
        }
        other => {
            return Err(DaemonClientError::UnexpectedResponse(other));
        }
    }

    // Connection 2: writer — held open for WriteStdin/ResizePty/Detach
    let writer = connect_to_daemon().await?;

    info!(
        event = "ui.daemon.attach_completed",
        session_id = session_id
    );

    Ok(DaemonConnection {
        reader,
        writer,
        session_id: session_id.to_string(),
    })
}

/// Send WriteStdin IPC message (base64-encoded data).
pub async fn send_write_stdin(
    writer: &mut Async<UnixStream>,
    session_id: &str,
    data: &[u8],
) -> Result<(), DaemonClientError> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    let msg = ClientMessage::WriteStdin {
        id: next_request_id(),
        session_id: SessionId::from(session_id),
        data: encoded,
    };
    send_message(writer, &msg).await
}

/// Send ResizePty IPC message.
pub async fn send_resize(
    writer: &mut Async<UnixStream>,
    session_id: &str,
    rows: u16,
    cols: u16,
) -> Result<(), DaemonClientError> {
    let msg = ClientMessage::ResizePty {
        id: next_request_id(),
        session_id: SessionId::from(session_id),
        rows,
        cols,
    };
    send_message(writer, &msg).await
}

/// Send Detach IPC message.
pub async fn send_detach(
    writer: &mut Async<UnixStream>,
    session_id: &str,
) -> Result<(), DaemonClientError> {
    let msg = ClientMessage::Detach {
        id: next_request_id(),
        session_id: SessionId::from(session_id),
    };
    send_message(writer, &msg).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use kild_protocol::{ClientMessage, DaemonMessage, SessionId};

    #[test]
    fn test_next_request_id_increments() {
        let id1 = next_request_id();
        let id2 = next_request_id();
        assert!(id1.starts_with("ui-"));
        assert!(id2.starts_with("ui-"));
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_error_display_connection_closed() {
        let err = DaemonClientError::ConnectionClosed;
        assert_eq!(err.to_string(), "daemon closed connection (EOF)");
    }

    #[test]
    fn test_error_display_session_not_found() {
        let err = DaemonClientError::SessionNotFound;
        assert_eq!(err.to_string(), "no running daemon session found");
    }

    #[test]
    fn test_error_display_empty_response() {
        let err = DaemonClientError::EmptyResponse;
        assert_eq!(err.to_string(), "daemon sent empty response");
    }

    #[test]
    fn test_error_display_daemon_error() {
        let err = DaemonClientError::DaemonError {
            code: "session_not_found".to_string(),
            message: "no such session".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "daemon error (session_not_found): no such session"
        );
    }

    #[test]
    fn test_error_variants_are_distinct() {
        let connect =
            DaemonClientError::Connect(std::io::Error::new(std::io::ErrorKind::NotFound, "test"));
        let closed = DaemonClientError::ConnectionClosed;
        let empty = DaemonClientError::EmptyResponse;
        let not_found = DaemonClientError::SessionNotFound;
        assert_ne!(connect.to_string(), closed.to_string());
        assert_ne!(empty.to_string(), not_found.to_string());
    }

    #[test]
    fn test_client_message_serialization() {
        let msg = ClientMessage::Ping {
            id: "test-1".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ping\""));
        assert!(json.contains("\"id\":\"test-1\""));
    }

    #[test]
    fn test_daemon_message_ack_parsing() {
        let json = r#"{"type":"ack","id":"test-1"}"#;
        let msg: DaemonMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, DaemonMessage::Ack { .. }));
    }

    #[test]
    fn test_pty_output_parsing_with_base64() {
        let json = r#"{"type":"pty_output","session_id":"test","data":"aGVsbG8="}"#;
        let msg: DaemonMessage = serde_json::from_str(json).unwrap();
        if let DaemonMessage::PtyOutput { data, session_id } = msg {
            assert_eq!(&*session_id, "test");
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(&data)
                .unwrap();
            assert_eq!(decoded, b"hello");
        } else {
            panic!("expected PtyOutput");
        }
    }

    #[test]
    fn test_error_response_parsing() {
        let json = r#"{"type":"error","id":"req-1","code":"session_not_found","message":"no such session"}"#;
        let msg: DaemonMessage = serde_json::from_str(json).unwrap();
        if let DaemonMessage::Error { code, message, .. } = msg {
            assert_eq!(code, ErrorCode::SessionNotFound);
            assert_eq!(message, "no such session");
        } else {
            panic!("expected Error");
        }
    }

    #[test]
    fn test_write_stdin_base64_roundtrip() {
        let data = b"hello world";
        let encoded = base64::engine::general_purpose::STANDARD.encode(data);
        let msg = ClientMessage::WriteStdin {
            id: "test".to_string(),
            session_id: SessionId::new("sess"),
            data: encoded.clone(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(&encoded));
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
        if let ClientMessage::WriteStdin { data: d, .. } = parsed {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(&d)
                .unwrap();
            assert_eq!(decoded, b"hello world");
        } else {
            panic!("expected WriteStdin");
        }
    }

    #[test]
    fn test_get_session_message_roundtrip() {
        let msg = ClientMessage::GetSession {
            id: "ui-42".to_string(),
            session_id: SessionId::new("myapp_feature-auth"),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"get_session\""));
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id(), "ui-42");
    }

    #[test]
    fn test_stop_session_message_roundtrip() {
        let msg = ClientMessage::StopSession {
            id: "ui-43".to_string(),
            session_id: SessionId::new("myapp_feature-auth"),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"stop_session\""));
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id(), "ui-43");
    }

    #[test]
    fn test_session_info_response_parsing() {
        let json = r#"{"type":"session_info","id":"req-1","session":{"id":"test-sess","working_directory":"/tmp","command":"bash","status":"running","created_at":"2026-02-12T00:00:00Z"}}"#;
        let msg: DaemonMessage = serde_json::from_str(json).unwrap();
        if let DaemonMessage::SessionInfo { session, .. } = msg {
            assert_eq!(&*session.id, "test-sess");
            assert_eq!(session.status, SessionStatus::Running);
        } else {
            panic!("expected SessionInfo");
        }
    }
}
