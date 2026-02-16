use base64::Engine;

use crate::errors::DaemonError;
use crate::protocol::messages::DaemonMessage;

/// A chunk of PTY output received from the daemon.
pub struct PtyOutputChunk {
    /// Raw PTY output bytes (decoded from base64).
    pub data: Vec<u8>,
}

/// Decode a `PtyOutput` daemon message into raw bytes.
///
/// Returns the decoded bytes, or `None` if the message is not a `PtyOutput`.
pub fn decode_pty_output(msg: &DaemonMessage) -> Result<Option<PtyOutputChunk>, DaemonError> {
    match msg {
        DaemonMessage::PtyOutput { data, .. } => {
            let decoded = base64::engine::general_purpose::STANDARD.decode(data)?;
            Ok(Some(PtyOutputChunk { data: decoded }))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_pty_output() {
        let msg = DaemonMessage::PtyOutput {
            session_id: "test".into(),
            data: base64::engine::general_purpose::STANDARD.encode(b"hello world"),
        };

        let chunk = decode_pty_output(&msg).unwrap().unwrap();
        assert_eq!(chunk.data, b"hello world");
    }

    #[test]
    fn test_decode_non_pty_output() {
        let msg = DaemonMessage::Ack {
            id: "1".to_string(),
        };

        let result = decode_pty_output(&msg).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_invalid_base64() {
        let msg = DaemonMessage::PtyOutput {
            session_id: "test".into(),
            data: "not-valid-base64!!!".to_string(),
        };

        let result = decode_pty_output(&msg);
        assert!(result.is_err());
    }
}
