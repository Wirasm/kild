//! Generic async JSONL client over any `futures::io::AsyncBufRead + AsyncWrite` pair.
//!
//! Used by `kild-ui` (smol executor) and will be used by the TCP transport
//! when #479 is implemented. The I/O transport is generic — callers supply the
//! stream halves. Message types are fixed to `ClientMessage`/`DaemonMessage`.

use futures::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
use serde::Serialize;

use crate::{ClientMessage, DaemonMessage, IpcError};

/// Async JSONL client, generic over any reader/writer pair.
///
/// `R` is typically `futures::io::BufReader<ReadHalf<T>>`.
/// `W` is typically `WriteHalf<T>` or `T` directly.
///
/// Constructed via `AsyncIpcClient::new(reader, writer)`.
/// For smol: split `Async<UnixStream>` with `smol::io::split()` first.
pub struct AsyncIpcClient<R, W> {
    reader: R,
    writer: W,
}

impl<R, W> AsyncIpcClient<R, W>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    /// Wrap a reader/writer pair.
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }

    /// Write a JSONL message and flush, then read one JSONL response.
    ///
    /// The standard request-response pattern. Flush is mandatory before reading
    /// to ensure the peer receives the request.
    ///
    /// Converts `DaemonMessage::Error` responses into `IpcError::DaemonError`.
    pub async fn send(&mut self, msg: &ClientMessage) -> Result<DaemonMessage, IpcError> {
        write_jsonl_flush(&mut self.writer, msg).await?;
        let response = read_jsonl(&mut self.reader).await?;
        if let DaemonMessage::Error { code, message, .. } = response {
            return Err(IpcError::DaemonError { code, message });
        }
        Ok(response)
    }

    /// Write a JSONL message without flushing and without reading a response.
    ///
    /// For fire-and-forget writes (WriteStdin, ResizePty) where the caller does not
    /// wait for the daemon's Ack. The caller must call `flush()` before the
    /// connection is dropped, or buffered data will be silently lost.
    pub async fn write(&mut self, msg: &ClientMessage) -> Result<(), IpcError> {
        write_jsonl(&mut self.writer, msg).await
    }

    /// Read one JSONL response from the stream.
    ///
    /// Returns `Ok(None)` on EOF. Used for streaming PtyOutput after Attach.
    pub async fn read_next(&mut self) -> Result<Option<DaemonMessage>, IpcError> {
        read_jsonl_optional(&mut self.reader).await
    }

    /// Consume the client, returning (reader, writer) for use in split tasks.
    pub fn into_parts(self) -> (R, W) {
        (self.reader, self.writer)
    }
}

/// Serialize `msg` as compact JSON, write as a single line, no flush.
pub async fn write_jsonl<W: AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    msg: &T,
) -> Result<(), IpcError> {
    let json = serde_json::to_string(msg).map_err(|e| IpcError::ProtocolError {
        message: format!("serialization failed: {e}"),
    })?;
    writer
        .write_all(json.as_bytes())
        .await
        .map_err(IpcError::Io)?;
    writer.write_all(b"\n").await.map_err(IpcError::Io)?;
    Ok(())
}

/// Serialize `msg`, write as a single line, and flush.
pub async fn write_jsonl_flush<W: AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    msg: &T,
) -> Result<(), IpcError> {
    write_jsonl(writer, msg).await?;
    writer.flush().await.map_err(IpcError::Io)?;
    Ok(())
}

/// Read one JSONL line and parse as `DaemonMessage`.
///
/// Returns `Err(IpcError::ProtocolError)` on EOF or empty line.
async fn read_jsonl<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<DaemonMessage, IpcError> {
    read_jsonl_optional(reader)
        .await?
        .ok_or_else(|| IpcError::ProtocolError {
            message: "Empty response from daemon".to_string(),
        })
}

/// Read one JSONL line. Returns `Ok(None)` on EOF (connection closed).
/// Returns `Err(IpcError::ProtocolError)` on a blank line (protocol violation).
async fn read_jsonl_optional<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<DaemonMessage>, IpcError> {
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await.map_err(IpcError::Io)?;
    if bytes_read == 0 {
        return Ok(None);
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(IpcError::ProtocolError {
            message: "Daemon sent empty line (protocol violation)".to_string(),
        });
    }
    serde_json::from_str(trimmed)
        .map(Some)
        .map_err(|e| IpcError::ProtocolError {
            message: format!("Invalid JSON response: {e}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::io::Cursor;

    #[test]
    fn test_write_jsonl_produces_newline_terminated_json() {
        smol::block_on(async {
            let mut buf = Vec::new();
            let msg = ClientMessage::Ping {
                id: "t1".to_string(),
            };
            write_jsonl(&mut buf, &msg).await.unwrap();
            let s = std::str::from_utf8(&buf).unwrap();
            assert!(s.ends_with('\n'));
            assert!(s.contains("\"type\":\"ping\""));
        });
    }

    #[test]
    fn test_read_jsonl_parses_ack() {
        smol::block_on(async {
            let data = b"{\"type\":\"ack\",\"id\":\"t1\"}\n";
            let mut reader = futures::io::BufReader::new(Cursor::new(data.as_ref()));
            let msg = read_jsonl(&mut reader).await.unwrap();
            assert!(matches!(msg, DaemonMessage::Ack { .. }));
        });
    }

    #[test]
    fn test_read_jsonl_optional_returns_none_on_eof() {
        smol::block_on(async {
            let data: &[u8] = b"";
            let mut reader = futures::io::BufReader::new(Cursor::new(data));
            let result = read_jsonl_optional(&mut reader).await.unwrap();
            assert!(result.is_none());
        });
    }

    #[test]
    fn test_read_jsonl_optional_empty_line_is_protocol_error() {
        smol::block_on(async {
            let data: &[u8] = b"\n";
            let mut reader = futures::io::BufReader::new(Cursor::new(data));
            let result = read_jsonl_optional(&mut reader).await;
            assert!(matches!(result, Err(IpcError::ProtocolError { .. })));
        });
    }

    #[test]
    fn test_send_success_roundtrip() {
        use std::io::{BufRead, Write};
        smol::block_on(async {
            let (client_stream, server_stream) = std::os::unix::net::UnixStream::pair().unwrap();
            let server = std::thread::spawn(move || {
                let mut reader = std::io::BufReader::new(&server_stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                writeln!(&server_stream, r#"{{"type":"ack","id":"t1"}}"#).unwrap();
            });
            let async_stream = smol::Async::new(client_stream).unwrap();
            let (r, w) = smol::io::split(async_stream);
            let mut client = AsyncIpcClient::new(futures::io::BufReader::new(r), w);
            let response = client
                .send(&ClientMessage::Ping {
                    id: "t1".to_string(),
                })
                .await
                .unwrap();
            assert!(matches!(response, DaemonMessage::Ack { .. }));
            server.join().unwrap();
        });
    }

    #[test]
    fn test_send_error_response_converts_to_ipc_error() {
        use std::io::{BufRead, Write};
        smol::block_on(async {
            let (client_stream, server_stream) = std::os::unix::net::UnixStream::pair().unwrap();
            let server = std::thread::spawn(move || {
                let mut reader = std::io::BufReader::new(&server_stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let resp = r#"{"type":"error","id":"t1","code":"session_not_found","message":"no such session"}"#;
                writeln!(&server_stream, "{resp}").unwrap();
            });
            let async_stream = smol::Async::new(client_stream).unwrap();
            let (r, w) = smol::io::split(async_stream);
            let mut client = AsyncIpcClient::new(futures::io::BufReader::new(r), w);
            let result = client
                .send(&ClientMessage::Ping {
                    id: "t1".to_string(),
                })
                .await;
            assert!(matches!(result, Err(IpcError::DaemonError { .. })));
            if let Err(IpcError::DaemonError { code, .. }) = result {
                assert_eq!(code, crate::ErrorCode::SessionNotFound);
            }
            server.join().unwrap();
        });
    }

    #[test]
    fn test_read_next_streams_messages_and_returns_none_on_eof() {
        smol::block_on(async {
            let data = concat!(
                r#"{"type":"pty_output","session_id":"s1","data":"aGk="}"#,
                "\n",
                r#"{"type":"pty_output","session_id":"s1","data":"dGhlcmU="}"#,
                "\n"
            );
            let reader = futures::io::BufReader::new(Cursor::new(data.as_bytes()));
            let writer = Cursor::new(vec![]);
            let mut client = AsyncIpcClient::new(reader, writer);

            let first = client.read_next().await.unwrap();
            assert!(matches!(first, Some(DaemonMessage::PtyOutput { .. })));
            let second = client.read_next().await.unwrap();
            assert!(matches!(second, Some(DaemonMessage::PtyOutput { .. })));
            let eof = client.read_next().await.unwrap();
            assert!(eof.is_none());
        });
    }
}
