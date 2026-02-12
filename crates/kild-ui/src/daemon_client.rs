//! Async daemon client using smol on GPUI's BackgroundExecutor.
//!
//! Provides IPC operations for communicating with the kild daemon:
//! - `ping_daemon_async()` — Spike 1: simple roundtrip validation
//! - `list_sessions_async()` / `find_first_running_session()` — session discovery
//! - `connect_for_attach()` — two-connection attach for streaming PTY output
//! - `send_write_stdin()` / `send_resize()` / `send_detach()` — write operations

use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
use kild_protocol::{ClientMessage, DaemonMessage, SessionInfo, SessionStatus};
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

/// Send a JSONL message on a stream.
async fn send_message(
    stream: &mut Async<UnixStream>,
    msg: &ClientMessage,
) -> Result<(), DaemonClientError> {
    let json = serde_json::to_string(msg)?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
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
        id: "spike1-ping".to_string(),
    };

    // Write Ping as JSONL
    let json = serde_json::to_string(&request)?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;

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
pub async fn list_sessions_async() -> Result<Vec<SessionInfo>, DaemonClientError> {
    debug!(event = "ui.daemon.list_sessions_started");

    let mut stream = connect_to_daemon().await?;
    let request = ClientMessage::ListSessions {
        id: next_request_id(),
        project_id: None,
    };
    send_message(&mut stream, &request).await?;

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
pub async fn find_first_running_session() -> Result<SessionInfo, DaemonClientError> {
    let sessions = list_sessions_async().await?;
    sessions
        .into_iter()
        .find(|s| s.status == SessionStatus::Running)
        .ok_or(DaemonClientError::SessionNotFound)
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
        session_id: session_id.to_string(),
        rows,
        cols,
    };
    send_message(&mut reader_stream, &attach_request).await?;

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
        session_id: session_id.to_string(),
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
        session_id: session_id.to_string(),
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
        session_id: session_id.to_string(),
    };
    send_message(writer, &msg).await
}
