use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

use crate::errors::DaemonError;
#[cfg(test)]
use kild_core::errors::KildError;

/// Read a single JSONL message from an async buffered reader.
///
/// Returns `Ok(None)` when the stream is closed (EOF).
/// Returns `Err` on malformed JSON or IO errors.
pub async fn read_message<R, T>(reader: &mut R) -> Result<Option<T>, DaemonError>
where
    R: AsyncBufRead + Unpin,
    T: DeserializeOwned,
{
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        return Ok(None); // EOF
    }

    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let msg: T = serde_json::from_str(trimmed)
        .map_err(|e| DaemonError::ProtocolError(format!("invalid JSON: {}: {}", e, trimmed)))?;
    Ok(Some(msg))
}

/// Write a single JSONL message to an async writer.
///
/// Serializes the message as compact JSON followed by a newline, then flushes.
pub async fn write_message<W, T>(writer: &mut W, msg: &T) -> Result<(), DaemonError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let json = serde_json::to_string(msg)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::messages::{ClientMessage, DaemonMessage};
    use crate::types::SessionInfo;

    #[tokio::test]
    async fn test_roundtrip_client_message() {
        let msg = ClientMessage::ListSessions {
            id: "req-1".to_string(),
            project_id: None,
        };

        // Write to buffer
        let mut buf: Vec<u8> = Vec::new();
        write_message(&mut buf, &msg).await.unwrap();

        // Read back
        let mut reader = tokio::io::BufReader::new(buf.as_slice());
        let parsed: Option<ClientMessage> = read_message(&mut reader).await.unwrap();
        assert!(parsed.is_some());
        assert_eq!(parsed.unwrap().id(), "req-1");
    }

    #[tokio::test]
    async fn test_roundtrip_daemon_message() {
        let msg = DaemonMessage::Ack {
            id: "req-1".to_string(),
        };

        let mut buf: Vec<u8> = Vec::new();
        write_message(&mut buf, &msg).await.unwrap();

        let mut reader = tokio::io::BufReader::new(buf.as_slice());
        let parsed: Option<DaemonMessage> = read_message(&mut reader).await.unwrap();
        assert!(parsed.is_some());
    }

    #[tokio::test]
    async fn test_read_eof() {
        let buf: &[u8] = b"";
        let mut reader = tokio::io::BufReader::new(buf);
        let result: Option<ClientMessage> = read_message(&mut reader).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_read_invalid_json() {
        let buf: &[u8] = b"not json\n";
        let mut reader = tokio::io::BufReader::new(buf);
        let result: Result<Option<ClientMessage>, _> = read_message(&mut reader).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_code(), "protocol_error");
    }

    #[tokio::test]
    async fn test_multiple_messages() {
        let msg1 = ClientMessage::DaemonStop {
            id: "1".to_string(),
        };
        let msg2 = ClientMessage::ListSessions {
            id: "2".to_string(),
            project_id: Some("myapp".to_string()),
        };

        let mut buf: Vec<u8> = Vec::new();
        write_message(&mut buf, &msg1).await.unwrap();
        write_message(&mut buf, &msg2).await.unwrap();

        let mut reader = tokio::io::BufReader::new(buf.as_slice());

        let parsed1: ClientMessage = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(parsed1.id(), "1");

        let parsed2: ClientMessage = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(parsed2.id(), "2");

        // EOF
        let parsed3: Option<ClientMessage> = read_message(&mut reader).await.unwrap();
        assert!(parsed3.is_none());
    }

    #[tokio::test]
    async fn test_roundtrip_complex_message() {
        let msg = DaemonMessage::SessionCreated {
            id: "req-1".to_string(),
            session: SessionInfo {
                id: "myapp_feature-auth".to_string(),
                working_directory: "/tmp/wt".to_string(),
                command: "claude".to_string(),
                status: "running".to_string(),
                created_at: "2026-02-09T14:30:00Z".to_string(),
                client_count: Some(1),
                pty_pid: Some(12345),
                exit_code: None,
            },
        };

        let mut buf: Vec<u8> = Vec::new();
        write_message(&mut buf, &msg).await.unwrap();

        let mut reader = tokio::io::BufReader::new(buf.as_slice());
        let parsed: DaemonMessage = read_message(&mut reader).await.unwrap().unwrap();

        if let DaemonMessage::SessionCreated { id, session } = parsed {
            assert_eq!(id, "req-1");
            assert_eq!(session.command, "claude");
            assert_eq!(session.pty_pid, Some(12345));
        } else {
            panic!("wrong variant");
        }
    }
}
