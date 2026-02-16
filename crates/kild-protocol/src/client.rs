//! Shared synchronous JSONL-over-Unix-socket IPC client.
//!
//! Provides `IpcConnection` for connecting to the KILD daemon and sending
//! typed `ClientMessage`/`DaemonMessage` requests. Used by both `kild-core`
//! and `kild-tmux-shim` to avoid duplicating JSONL framing logic.

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

/// A synchronous JSONL connection to the KILD daemon over a Unix socket.
#[derive(Debug)]
pub struct IpcConnection {
    stream: UnixStream,
}

impl IpcConnection {
    /// Connect to the daemon at the given socket path.
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

        Ok(Self { stream })
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

        writeln!(self.stream, "{}", msg)?;
        self.stream.flush()?;

        let mut reader = BufReader::new(&self.stream);
        let mut line = String::new();
        reader.read_line(&mut line)?;

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
        self.stream.set_read_timeout(timeout)?;
        Ok(())
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
}
