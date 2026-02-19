//! Shared synchronous JSONL IPC client for Unix socket and TCP+TLS transports.
//!
//! Provides `IpcConnection` for connecting to the KILD daemon and sending
//! typed `ClientMessage`/`DaemonMessage` requests. Used by both `kild-core`
//! (Unix + TCP/TLS) and `kild-tmux-shim` (Unix only) to avoid duplicating
//! JSONL framing logic. TCP/TLS support requires the `tcp` Cargo feature.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use crate::{ClientMessage, DaemonMessage, ErrorCode};

/// Error from the shared IPC client layer.
#[non_exhaustive]
#[derive(Debug)]
pub enum IpcError {
    /// Daemon socket does not exist or connection was refused.
    NotRunning { path: String },
    /// Socket exists but connection failed for a non-`ConnectionRefused` reason.
    ConnectionFailed(std::io::Error),
    /// Daemon returned an explicit error response.
    DaemonError { code: ErrorCode, message: String },
    /// Protocol-level error (serialization, empty response, invalid JSON).
    ProtocolError { message: String },
    /// Other I/O error.
    Io(std::io::Error),
    /// TLS configuration or handshake error (only when `tcp` feature is enabled).
    #[cfg(feature = "tcp")]
    TlsConfig(String),
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcError::NotRunning { path } => {
                write!(f, "Daemon is not running (socket not found at {})", path)
            }
            IpcError::ConnectionFailed(e) => write!(f, "Connection failed: {}", e),
            IpcError::DaemonError { code, message } => {
                write!(f, "Daemon error [{}]: {}", code, message)
            }
            IpcError::ProtocolError { message } => write!(f, "Protocol error: {}", message),
            IpcError::Io(e) => write!(f, "IO error: {}", e),
            #[cfg(feature = "tcp")]
            IpcError::TlsConfig(msg) => write!(f, "TLS configuration error: {}", msg),
        }
    }
}

impl std::error::Error for IpcError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IpcError::ConnectionFailed(e) | IpcError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for IpcError {
    fn from(e: std::io::Error) -> Self {
        IpcError::Io(e)
    }
}

/// Internal stream type — Unix socket or TLS-wrapped TCP socket.
enum IpcStream {
    Unix(UnixStream),
    #[cfg(feature = "tcp")]
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, std::net::TcpStream>>),
}

/// RAII guard that restores a Unix socket's read timeout on drop.
struct TimeoutGuard<'a> {
    stream: &'a UnixStream,
    orig_timeout: Option<Duration>,
}

impl Drop for TimeoutGuard<'_> {
    fn drop(&mut self) {
        let _ = self.stream.set_read_timeout(self.orig_timeout);
    }
}

/// A synchronous JSONL connection to the KILD daemon.
///
/// Supports both Unix socket (local) and TCP+TLS (remote) transports.
/// The `tcp` Cargo feature must be enabled for TLS support.
#[derive(Debug)]
pub struct IpcConnection {
    stream: IpcStream,
}

impl std::fmt::Debug for IpcStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcStream::Unix(s) => write!(f, "IpcStream::Unix({:?})", s),
            #[cfg(feature = "tcp")]
            IpcStream::Tls(_) => write!(f, "IpcStream::Tls(...)"),
        }
    }
}

impl IpcConnection {
    /// Connect to the daemon at the given Unix socket path.
    ///
    /// Checks that the socket file exists, connects, and configures timeouts
    /// (30s read, 5s write). Returns `IpcError::NotRunning` if the socket
    /// doesn't exist or connection is refused.
    pub fn connect(socket_path: &Path) -> Result<Self, IpcError> {
        if !socket_path.exists() {
            return Err(IpcError::NotRunning {
                path: socket_path.display().to_string(),
            });
        }

        let stream = UnixStream::connect(socket_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::ConnectionRefused {
                IpcError::NotRunning {
                    path: socket_path.display().to_string(),
                }
            } else {
                IpcError::ConnectionFailed(e)
            }
        })?;

        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;

        Ok(Self {
            stream: IpcStream::Unix(stream),
        })
    }

    /// Connect to a remote daemon at `addr` (host:port) via TCP+TLS.
    ///
    /// The `verifier` is a TOFU fingerprint verifier that rejects connections
    /// if the server cert doesn't match the pinned fingerprint.
    ///
    /// No connection caching for TLS — probing a `StreamOwned` with a 1ms read
    /// timeout can corrupt the TLS state machine. CLI callers create a fresh
    /// connection per invocation; the cost is acceptable.
    #[cfg(feature = "tcp")]
    pub fn connect_tls(
        addr: &str,
        verifier: std::sync::Arc<dyn rustls::client::danger::ServerCertVerifier>,
    ) -> Result<Self, IpcError> {
        use std::net::TcpStream;
        use std::sync::Arc;

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| IpcError::TlsConfig(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();

        let host = addr.split(':').next().unwrap_or(addr);
        let server_name = rustls::pki_types::ServerName::try_from(host.to_owned())
            .map_err(|e| IpcError::TlsConfig(e.to_string()))?;

        let tcp_stream = TcpStream::connect(addr).map_err(IpcError::ConnectionFailed)?;
        tcp_stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(IpcError::Io)?;
        tcp_stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(IpcError::Io)?;

        let conn = rustls::ClientConnection::new(std::sync::Arc::new(config), server_name)
            .map_err(|e| IpcError::TlsConfig(e.to_string()))?;

        Ok(Self {
            stream: IpcStream::Tls(Box::new(rustls::StreamOwned::new(conn, tcp_stream))),
        })
    }

    /// Send a typed request and read one typed response.
    ///
    /// Serializes `request` as JSON, writes it as a single line, flushes,
    /// then reads one line of JSON response. Converts `DaemonMessage::Error`
    /// into `IpcError::DaemonError`.
    pub fn send(&mut self, request: &ClientMessage) -> Result<DaemonMessage, IpcError> {
        let msg = serde_json::to_string(request).map_err(|e| IpcError::ProtocolError {
            message: e.to_string(),
        })?;

        let line = match &mut self.stream {
            IpcStream::Unix(s) => {
                writeln!(s, "{}", msg)?;
                s.flush()?;
                // Transient BufReader — not stored as a field because KILD's
                // request-response protocol expects exactly one response line per send().
                // Storing it would risk buffering extra data from the stream.
                let mut reader = BufReader::new(&*s);
                let mut line = String::new();
                reader.read_line(&mut line)?;
                line
            }
            #[cfg(feature = "tcp")]
            IpcStream::Tls(s) => {
                // StreamOwned implements Read + Write; needs &mut for both.
                writeln!(s, "{}", msg)?;
                s.flush()?;
                // Transient BufReader over &mut StreamOwned for the read half.
                let mut reader = BufReader::new(s.by_ref());
                let mut line = String::new();
                reader.read_line(&mut line)?;
                line
            }
        };

        if line.is_empty() {
            return Err(IpcError::ProtocolError {
                message: "Empty response from daemon".to_string(),
            });
        }

        let response: DaemonMessage =
            serde_json::from_str(&line).map_err(|e| IpcError::ProtocolError {
                message: format!("Invalid JSON response: {}", e),
            })?;

        if let DaemonMessage::Error { code, message, .. } = response {
            return Err(IpcError::DaemonError { code, message });
        }

        Ok(response)
    }

    /// Override the read timeout on the underlying socket.
    ///
    /// Callers like `ping_daemon()` use shorter timeouts than the default 30s.
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> Result<(), IpcError> {
        match &self.stream {
            IpcStream::Unix(s) => Ok(s.set_read_timeout(timeout)?),
            #[cfg(feature = "tcp")]
            IpcStream::Tls(s) => Ok(s.get_ref().set_read_timeout(timeout)?),
        }
    }

    /// Check if the connection is still usable (peer hasn't closed).
    ///
    /// For Unix streams: temporarily sets a 1ms read timeout (restored via RAII
    /// guard, even on panic) and attempts a read. Returns `false` if the peer
    /// has definitely closed, `true` otherwise.
    ///
    /// For TLS streams: always returns `false` regardless of actual socket state.
    /// A 1ms read probe on a `StreamOwned<ClientConnection, TcpStream>` can
    /// corrupt the TLS state machine. TLS connections are never cached — callers
    /// create fresh connections per request. `false` here means "do not cache",
    /// not "definitely closed".
    pub fn is_alive(&self) -> bool {
        match &self.stream {
            IpcStream::Unix(s) => Self::is_unix_alive(s),
            #[cfg(feature = "tcp")]
            IpcStream::Tls(_) => false,
        }
    }

    fn is_unix_alive(s: &UnixStream) -> bool {
        use std::io::Read;

        let orig_timeout = s.read_timeout().ok().flatten();
        // RAII guard ensures timeout is restored even on panic
        let _guard = TimeoutGuard {
            stream: s,
            orig_timeout,
        };

        // Fail-closed: if we can't set the probe timeout, assume broken
        if s.set_read_timeout(Some(Duration::from_millis(1))).is_err() {
            return false;
        }

        let mut buf = [0u8; 1];
        let mut stream_ref = s;
        match stream_ref.read(&mut buf) {
            Ok(0) => false, // EOF — peer closed
            Ok(_) => true,  // Unexpected data but socket alive (possible protocol violation)
            Err(ref e)
                if e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                true
            }
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn test_connect_missing_socket() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("nonexistent.sock");

        let result = IpcConnection::connect(&sock_path);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), IpcError::NotRunning { .. }),
            "Should return NotRunning for missing socket"
        );
    }

    #[test]
    fn test_send_success() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(&stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();

            let response = r#"{"type":"ack","id":"test-123"}"#;
            writeln!(stream, "{}", response).unwrap();
            stream.flush().unwrap();
        });

        let mut conn = IpcConnection::connect(&sock_path).unwrap();
        let request = ClientMessage::Ping {
            id: "test-123".to_string(),
        };
        let response = conn.send(&request).unwrap();
        assert!(matches!(response, DaemonMessage::Ack { .. }));

        handle.join().unwrap();
    }

    #[test]
    fn test_send_error_response() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(&stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();

            let response = r#"{"type":"error","id":"1","code":"session_not_found","message":"no such session"}"#;
            writeln!(stream, "{}", response).unwrap();
            stream.flush().unwrap();
        });

        let mut conn = IpcConnection::connect(&sock_path).unwrap();
        let request = ClientMessage::Ping {
            id: "test".to_string(),
        };
        let result = conn.send(&request);
        assert!(result.is_err());
        match result.unwrap_err() {
            IpcError::DaemonError { code, message } => {
                assert_eq!(code, ErrorCode::SessionNotFound);
                assert_eq!(message, "no such session");
            }
            other => panic!("expected DaemonError, got: {}", other),
        }

        handle.join().unwrap();
    }

    #[test]
    fn test_send_empty_response() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(&stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            drop(stream);
        });

        let mut conn = IpcConnection::connect(&sock_path).unwrap();
        let request = ClientMessage::Ping {
            id: "test".to_string(),
        };
        let result = conn.send(&request);
        assert!(result.is_err());
        match result.unwrap_err() {
            IpcError::ProtocolError { message } => {
                assert!(message.contains("Empty response"), "got: {}", message);
            }
            other => panic!("expected ProtocolError, got: {}", other),
        }

        handle.join().unwrap();
    }

    #[test]
    fn test_send_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(&stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();

            writeln!(stream, "not-json{{").unwrap();
            stream.flush().unwrap();
        });

        let mut conn = IpcConnection::connect(&sock_path).unwrap();
        let request = ClientMessage::Ping {
            id: "test".to_string(),
        };
        let result = conn.send(&request);
        assert!(result.is_err());
        match result.unwrap_err() {
            IpcError::ProtocolError { message } => {
                assert!(message.contains("Invalid JSON"), "got: {}", message);
            }
            other => panic!("expected ProtocolError, got: {}", other),
        }

        handle.join().unwrap();
    }

    #[test]
    fn test_connection_reuse_multiple_sends() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut reader = std::io::BufReader::new(stream);
            // Handle two sequential requests on the same connection
            for _ in 0..2 {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let response = r#"{"type":"ack","id":"1"}"#;
                writeln!(writer, "{}", response).unwrap();
                writer.flush().unwrap();
            }
        });

        let mut conn = IpcConnection::connect(&sock_path).unwrap();
        let req = ClientMessage::Ping {
            id: "1".to_string(),
        };
        let r1 = conn.send(&req);
        assert!(r1.is_ok(), "First send failed: {:?}", r1.err());
        let r2 = conn.send(&req);
        assert!(r2.is_ok(), "Second send failed: {:?}", r2.err());

        handle.join().unwrap();
    }

    #[test]
    fn test_is_alive_on_connected_socket() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let _listener = UnixListener::bind(&sock_path).unwrap();

        let conn = IpcConnection::connect(&sock_path).unwrap();
        assert!(conn.is_alive(), "Connected socket should be alive");
    }

    #[test]
    fn test_is_alive_on_closed_socket() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let conn = IpcConnection::connect(&sock_path).unwrap();
        // Accept and immediately close the server side
        let (server_stream, _) = listener.accept().unwrap();
        drop(server_stream);

        // Give the kernel a moment to propagate the close
        std::thread::sleep(std::time::Duration::from_millis(50));

        assert!(
            !conn.is_alive(),
            "Socket with closed peer should not be alive"
        );
    }

    #[test]
    fn test_is_alive_restores_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let _listener = UnixListener::bind(&sock_path).unwrap();

        let conn = IpcConnection::connect(&sock_path).unwrap();

        // Default timeout is 30s from connect()
        let before = match &conn.stream {
            IpcStream::Unix(s) => s.read_timeout().unwrap(),
            #[cfg(feature = "tcp")]
            IpcStream::Tls(_) => unreachable!(),
        };
        assert_eq!(before, Some(Duration::from_secs(30)));

        // is_alive() temporarily sets 1ms timeout then restores
        assert!(conn.is_alive());

        let after = match &conn.stream {
            IpcStream::Unix(s) => s.read_timeout().unwrap(),
            #[cfg(feature = "tcp")]
            IpcStream::Tls(_) => unreachable!(),
        };
        assert_eq!(after, before, "is_alive() should restore original timeout");
    }
}
