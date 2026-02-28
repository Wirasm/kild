//! Async daemon client using smol on GPUI's BackgroundExecutor.
//!
//! Provides IPC operations for communicating with the kild daemon:
//! - `ping_daemon_async()` — check if daemon is running and responsive
//! - `list_sessions_async()` / `find_first_running_session()` — session discovery
//! - `get_session_async()` — query a single session by ID
//! - `stop_session_async()` — stop a running daemon session
//! - `connect_for_attach()` — two-connection attach for streaming PTY output
//! - `send_write_stdin()` / `send_resize()` / `send_detach()` — write operations
//!
//! Transport routing: when `remote_host` is set in config (or via KILD remote
//! override), connections use TCP+TLS instead of the local Unix socket. Both
//! paths are unified under `ErasedUiClient` via type erasure.

use std::os::unix::net::UnixStream;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
use futures::io::BufReader;
use futures_rustls::TlsConnector;
use kild_config::KildConfig;
use kild_protocol::{
    AsyncIpcClient, ClientMessage, DaemonMessage, DaemonSessionStatus, ErrorCode, IpcError,
    SessionId, SessionStatus,
};
use smol::Async;
use smol::io::split;
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
    Io(std::io::Error),

    #[error("unexpected response from daemon: {0:?}")]
    UnexpectedResponse(DaemonMessage),

    #[error("daemon error ({code}): {message}")]
    DaemonError { code: ErrorCode, message: String },

    #[allow(dead_code)]
    #[error("no running daemon session found")]
    SessionNotFound,

    #[error("base64 decode failed: {0}")]
    Base64Decode(#[from] base64::DecodeError),

    #[error("protocol error: {0}")]
    Protocol(String),
}

impl From<IpcError> for DaemonClientError {
    fn from(e: IpcError) -> Self {
        match e {
            IpcError::NotRunning { path } => {
                DaemonClientError::Connect(std::io::Error::new(std::io::ErrorKind::NotFound, path))
            }
            IpcError::ConnectionFailed(io) => DaemonClientError::Connect(io),
            IpcError::DaemonError { code, message } => {
                DaemonClientError::DaemonError { code, message }
            }
            IpcError::ProtocolError { message } => DaemonClientError::Protocol(message),
            IpcError::Io(io) => DaemonClientError::Io(io),
            _ => DaemonClientError::Protocol(e.to_string()),
        }
    }
}

/// Type-erased reader: boxes any `AsyncBufRead + Send + Unpin`.
///
/// Unifies Unix and TCP/TLS halves under a single type so both transport paths
/// share one `AsyncIpcClient` type. One allocation per connection — acceptable
/// for IPC.
type DynReader = Box<dyn futures::io::AsyncBufRead + Send + Unpin>;
/// Type-erased writer: boxes any `AsyncWrite + Send + Unpin`.
type DynWriter = Box<dyn futures::io::AsyncWrite + Send + Unpin>;
/// Unified async IPC client for both Unix socket and TCP/TLS connections.
type ErasedUiClient = AsyncIpcClient<DynReader, DynWriter>;

/// Connect to the daemon Unix socket, returning an `ErasedUiClient`.
async fn connect() -> Result<ErasedUiClient, DaemonClientError> {
    let socket_path = kild_core::daemon::socket_path();
    if !socket_path.exists() {
        return Err(DaemonClientError::Connect(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "daemon socket not found",
        )));
    }
    let stream = Async::<UnixStream>::connect(&socket_path)
        .await
        .map_err(DaemonClientError::Connect)?;
    let (r, w) = split(stream);
    Ok(AsyncIpcClient::new(
        Box::new(BufReader::new(r)) as DynReader,
        Box::new(w) as DynWriter,
    ))
}

/// Connect to a remote daemon via TCP+TLS, returning an `ErasedUiClient`.
///
/// Uses `futures_rustls` (not tokio-rustls) because the rest of the UI async
/// path uses the `futures::io` trait family. Box-pins the TLS stream so
/// `futures::io::split()` halves satisfy `'static`.
async fn connect_tcp(
    addr: &str,
    fingerprint: [u8; 32],
) -> Result<ErasedUiClient, DaemonClientError> {
    let stream = smol::net::TcpStream::connect(addr)
        .await
        .map_err(DaemonClientError::Connect)?;

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = kild_core::daemon::tofu::TofuVerifier::new(fingerprint);
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| DaemonClientError::Protocol(e.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(config));
    let host = addr.split(':').next().unwrap_or(addr);
    let server_name = rustls::pki_types::ServerName::try_from(host.to_owned())
        .map_err(|e| DaemonClientError::Protocol(e.to_string()))?;

    let tls_stream = connector
        .connect(server_name, stream)
        .await
        .map_err(DaemonClientError::Connect)?;

    // Box::pin extends the lifetime so split halves satisfy 'static.
    // futures_rustls::client::TlsStream: Unpin after Box::pin.
    let pinned: Pin<Box<futures_rustls::client::TlsStream<smol::net::TcpStream>>> =
        Box::pin(tls_stream);
    let (r, w) = split(pinned);
    Ok(AsyncIpcClient::new(
        Box::new(BufReader::new(r)) as DynReader,
        Box::new(w) as DynWriter,
    ))
}

/// Connect to the daemon using Unix socket or TCP/TLS based on config.
///
/// Routing priority (highest wins):
/// 1. `remote_host` in `~/.kild/config.toml`
/// 2. Local Unix socket (default)
///
/// Note: the `--remote` CLI override is not checked here because kild-ui is a
/// separate binary where `set_remote_override()` is never called. The UI reads
/// remote config exclusively from the config file.
async fn connect_for_config() -> Result<ErasedUiClient, DaemonClientError> {
    // Check config file
    let config = match KildConfig::load_hierarchy() {
        Ok(c) => c,
        Err(e) => {
            warn!(
                event = "ui.daemon.config_load_failed",
                error = %e,
                "Failed to load config; falling back to defaults. \
                 Remote daemon settings will not be applied."
            );
            KildConfig::default()
        }
    };
    if let Some(ref remote_host) = config.daemon.remote_host {
        let fp_str = config
            .daemon
            .remote_cert_fingerprint
            .as_deref()
            .ok_or_else(|| {
                DaemonClientError::Protocol(
                    "remote_host is set but remote_cert_fingerprint is missing".to_string(),
                )
            })?;
        let fingerprint = kild_core::daemon::tofu::parse_fingerprint(fp_str)
            .map_err(DaemonClientError::Protocol)?;
        return connect_tcp(remote_host, fingerprint).await;
    }

    connect().await
}

/// Async ping to the kild daemon via smol.
///
/// Returns `Ok(true)` if daemon responded with Ack, `Ok(false)` if daemon
/// is not running (socket missing or connection refused), `Err` for
/// unexpected failures.
pub async fn ping_daemon_async() -> Result<bool, DaemonClientError> {
    debug!(event = "ui.daemon.ping_started");

    let mut client = match connect_for_config().await {
        Ok(c) => c,
        Err(DaemonClientError::Connect(e))
            if e.kind() == std::io::ErrorKind::NotFound
                || e.kind() == std::io::ErrorKind::ConnectionRefused =>
        {
            info!(event = "ui.daemon.ping_completed", result = "not_reachable");
            return Ok(false);
        }
        Err(e) => {
            error!(event = "ui.daemon.ping_failed", error = %e);
            return Err(e);
        }
    };

    let request = ClientMessage::Ping {
        id: next_request_id(),
    };
    let response = client.send(&request).await?;

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
pub async fn list_sessions_async() -> Result<Vec<DaemonSessionStatus>, DaemonClientError> {
    debug!(event = "ui.daemon.list_sessions_started");

    let mut client = connect_for_config().await?;
    let request = ClientMessage::ListSessions {
        id: next_request_id(),
        project_id: None,
    };
    let response = client.send(&request).await?;

    match response {
        DaemonMessage::SessionList { sessions, .. } => {
            info!(
                event = "ui.daemon.list_sessions_completed",
                count = sessions.len()
            );
            Ok(sessions)
        }
        other => Err(DaemonClientError::UnexpectedResponse(other)),
    }
}

/// Find the first Running daemon session.
///
/// Temporary convenience for the Ctrl+D toggle flow. Phase 3 (layout shell)
/// replaces this with explicit sidebar-driven session selection.
#[allow(dead_code)]
pub async fn find_first_running_session() -> Result<DaemonSessionStatus, DaemonClientError> {
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
pub async fn get_session_async(
    session_id: &str,
) -> Result<Option<DaemonSessionStatus>, DaemonClientError> {
    debug!(
        event = "ui.daemon.get_session_started",
        session_id = session_id
    );

    let mut client = connect_for_config().await?;
    let request = ClientMessage::GetSession {
        id: next_request_id(),
        session_id: SessionId::from(session_id),
    };
    // Intercept SessionNotFound before IpcError is converted to DaemonClientError,
    // so we can return Ok(None) rather than propagating a DaemonError.
    let response = match client.send(&request).await {
        Ok(r) => r,
        Err(IpcError::DaemonError {
            code: ErrorCode::SessionNotFound,
            ..
        }) => {
            info!(
                event = "ui.daemon.get_session_completed",
                session_id = session_id,
                result = "not_found"
            );
            return Ok(None);
        }
        Err(e) => return Err(e.into()),
    };

    match response {
        DaemonMessage::SessionInfo { session, .. } => {
            info!(
                event = "ui.daemon.get_session_completed",
                session_id = session_id,
                status = %session.status
            );
            Ok(Some(session))
        }
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

    let mut client = connect_for_config().await?;
    let request = ClientMessage::StopSession {
        id: next_request_id(),
        session_id: SessionId::from(session_id),
    };
    let response = client.send(&request).await?;

    match response {
        DaemonMessage::Ack { .. } => {
            info!(
                event = "ui.daemon.stop_session_completed",
                session_id = session_id
            );
            Ok(())
        }
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

    let mut client = connect_for_config().await?;
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
    let response = client.send(&request).await?;

    match response {
        DaemonMessage::SessionCreated { session, .. } => {
            info!(
                event = "ui.daemon.create_session_completed",
                daemon_session_id = %session.id
            );
            Ok(session.id.into_inner())
        }
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
    reader: DynReader,
    writer: DynWriter,
    session_id: String,
}

impl DaemonConnection {
    /// Get the session ID for this connection.
    #[allow(dead_code)]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Consume the connection, returning its parts for use in reader/writer tasks.
    pub fn into_parts(self) -> (DynReader, DynWriter, String) {
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
    let mut read_client = connect_for_config().await?;
    let attach_request = ClientMessage::Attach {
        id: next_request_id(),
        session_id: SessionId::from(session_id),
        rows,
        cols,
    };
    let ack = read_client.send(&attach_request).await?;
    match ack {
        DaemonMessage::Ack { .. } => {
            info!(
                event = "ui.daemon.attach_ack_received",
                session_id = session_id
            );
        }
        other => {
            return Err(DaemonClientError::UnexpectedResponse(other));
        }
    }

    // Extract the reader half; discard the unused writer half of the read connection.
    let (reader, _) = read_client.into_parts();

    // Connection 2: writer — held open for WriteStdin/ResizePty/Detach.
    // No protocol handshake is sent on this connection: the daemon dispatches
    // WriteStdin/ResizePty/Detach by session_id from each message's payload,
    // not by connection-level attachment state.
    let write_client = connect_for_config().await?;
    let (_, writer) = write_client.into_parts();

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
    writer: &mut DynWriter,
    session_id: &str,
    data: &[u8],
) -> Result<(), DaemonClientError> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    let msg = ClientMessage::WriteStdin {
        id: next_request_id(),
        session_id: SessionId::from(session_id),
        data: encoded,
    };
    kild_protocol::async_client::write_jsonl(writer, &msg)
        .await
        .map_err(Into::into)
}

/// Send ResizePty IPC message.
pub async fn send_resize(
    writer: &mut DynWriter,
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
    kild_protocol::async_client::write_jsonl(writer, &msg)
        .await
        .map_err(Into::into)
}

/// Send Detach IPC message.
///
/// Flushes after writing to ensure the daemon receives the detach before the
/// writer is dropped — without flush a buffered Detach would be silently lost.
pub async fn send_detach(
    writer: &mut DynWriter,
    session_id: &str,
) -> Result<(), DaemonClientError> {
    let msg = ClientMessage::Detach {
        id: next_request_id(),
        session_id: SessionId::from(session_id),
    };
    kild_protocol::async_client::write_jsonl_flush(writer, &msg)
        .await
        .map_err(Into::into)
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
    fn test_error_display_session_not_found() {
        let err = DaemonClientError::SessionNotFound;
        assert_eq!(err.to_string(), "no running daemon session found");
    }

    #[test]
    fn test_error_display_daemon_error() {
        let err = DaemonClientError::DaemonError {
            code: ErrorCode::SessionNotFound,
            message: "no such session".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "daemon error (session_not_found): no such session"
        );
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
            panic!("expected session_info response");
        }
    }

    #[test]
    fn test_from_ipc_error_not_running() {
        let e = IpcError::NotRunning {
            path: "/tmp/kild.sock".to_string(),
        };
        let ce: DaemonClientError = e.into();
        assert!(matches!(ce, DaemonClientError::Connect(_)));
    }

    #[test]
    fn test_from_ipc_error_connection_failed() {
        let e =
            IpcError::ConnectionFailed(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe"));
        let ce: DaemonClientError = e.into();
        assert!(matches!(ce, DaemonClientError::Connect(_)));
    }

    #[test]
    fn test_from_ipc_error_daemon_error() {
        let e = IpcError::DaemonError {
            code: ErrorCode::SessionNotFound,
            message: "no such session".to_string(),
        };
        let ce: DaemonClientError = e.into();
        assert!(matches!(
            ce,
            DaemonClientError::DaemonError {
                code: ErrorCode::SessionNotFound,
                ..
            }
        ));
    }

    #[test]
    fn test_from_ipc_error_io() {
        let e = IpcError::Io(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"));
        let ce: DaemonClientError = e.into();
        assert!(matches!(ce, DaemonClientError::Io(_)));
    }

    #[test]
    fn test_from_ipc_error_protocol() {
        let e = IpcError::ProtocolError {
            message: "bad json".to_string(),
        };
        let ce: DaemonClientError = e.into();
        assert!(matches!(ce, DaemonClientError::Protocol(_)));
    }
}
