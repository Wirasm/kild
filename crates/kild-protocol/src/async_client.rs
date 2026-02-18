//! Generic async JSONL client over any `futures::io::AsyncBufRead + AsyncWrite` pair.
//!
//! Used by `kild-ui` (smol executor) and will be used by the TCP transport
//! when #479 is implemented. Transport-agnostic — callers supply the stream halves.

use futures::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
use serde::Serialize;

use crate::{ClientMessage, DaemonMessage, IpcError};

/// Async JSONL client, generic over any reader/writer pair.
///
/// `R` is typically `futures::io::BufReader<ReadHalf<T>>`.
/// `W` is typically `WriteHalf<T>` or `T` directly.
///
/// Constructed via `AsyncIpcClient::new(reader, writer)`.
/// For smol: split `Async<UnixStream>` with `futures::io::split()` first.
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
    /// For fire-and-forget writes (WriteStdin, ResizePty) where caller does not
    /// wait for the daemon's Ack. Caller is responsible for eventual flush or
    /// connection teardown to drain the buffer.
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
        return Ok(None);
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
    fn test_send_converts_error_response_to_ipc_error() {
        smol::block_on(async {
            let response_json = format!(
                "{}\n",
                serde_json::to_string(&DaemonMessage::Error {
                    id: "t1".to_string(),
                    code: crate::ErrorCode::SessionNotFound,
                    message: "no such session".to_string(),
                })
                .unwrap()
            );
            let mut reader =
                futures::io::BufReader::new(Cursor::new(response_json.as_bytes().to_vec()));
            let msg = read_jsonl(&mut reader).await.unwrap();
            if let DaemonMessage::Error { code, message, .. } = msg {
                assert_eq!(code, crate::ErrorCode::SessionNotFound);
                assert_eq!(message, "no such session");
            } else {
                panic!("expected Error");
            }
        });
    }
}
