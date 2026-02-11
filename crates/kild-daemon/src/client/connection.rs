use std::collections::HashMap;
use std::path::Path;

use tokio::io::BufReader;
use tokio::net::UnixStream;
use tracing::debug;

use crate::errors::DaemonError;
use crate::protocol::codec::{read_message, write_message};
use crate::protocol::messages::{ClientMessage, DaemonMessage, ErrorCode};
use crate::types::SessionInfo;

/// Client for communicating with the daemon over IPC.
///
/// Connects to the daemon's Unix socket and provides typed methods
/// for all supported operations.
pub struct DaemonClient {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
    next_id: u64,
}

impl DaemonClient {
    /// Connect to the daemon at the given socket path.
    pub async fn connect(socket_path: &Path) -> Result<Self, DaemonError> {
        let stream = UnixStream::connect(socket_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::ConnectionRefused
                || e.kind() == std::io::ErrorKind::NotFound
            {
                DaemonError::NotRunning
            } else {
                DaemonError::ConnectionFailed(e.to_string())
            }
        })?;

        let (reader, writer) = stream.into_split();

        debug!(
            event = "daemon.client.connected",
            socket = %socket_path.display(),
        );

        Ok(Self {
            reader: BufReader::new(reader),
            writer,
            next_id: 1,
        })
    }

    /// Generate a unique request ID.
    fn next_id(&mut self) -> String {
        let id = self.next_id;
        self.next_id += 1;
        format!("req-{}", id)
    }

    /// Send a request and read the response.
    async fn request(&mut self, msg: &ClientMessage) -> Result<DaemonMessage, DaemonError> {
        write_message(&mut self.writer, msg).await?;
        let response: DaemonMessage = read_message(&mut self.reader)
            .await?
            .ok_or_else(|| DaemonError::ConnectionFailed("connection closed".to_string()))?;
        Ok(response)
    }

    /// Check if a response is an error, and if so, convert it.
    fn check_error(response: &DaemonMessage) -> Result<(), DaemonError> {
        if let DaemonMessage::Error { code, message, .. } = response {
            match code {
                ErrorCode::SessionNotFound => {
                    return Err(DaemonError::SessionNotFound(message.clone()));
                }
                ErrorCode::SessionAlreadyExists => {
                    return Err(DaemonError::SessionAlreadyExists(message.clone()));
                }
                ErrorCode::SessionNotRunning => {
                    return Err(DaemonError::SessionNotRunning(message.clone()));
                }
                ErrorCode::PtyError => return Err(DaemonError::PtyError(message.clone())),
                _ => {
                    return Err(DaemonError::ProtocolError(format!("{}: {}", code, message)));
                }
            }
        }
        Ok(())
    }

    /// Create a new session with a PTY.
    ///
    /// The daemon creates the PTY and spawns the command. The caller (kild-core)
    /// is responsible for git worktree creation and session file persistence.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_session(
        &mut self,
        session_id: &str,
        working_directory: &str,
        command: &str,
        args: &[String],
        env_vars: &HashMap<String, String>,
        rows: u16,
        cols: u16,
        use_login_shell: bool,
    ) -> Result<SessionInfo, DaemonError> {
        let id = self.next_id();
        let msg = ClientMessage::CreateSession {
            id,
            session_id: session_id.to_string(),
            working_directory: working_directory.to_string(),
            command: command.to_string(),
            args: args.to_vec(),
            env_vars: env_vars.clone(),
            rows,
            cols,
            use_login_shell,
        };

        let response = self.request(&msg).await?;
        Self::check_error(&response)?;

        if let DaemonMessage::SessionCreated { session, .. } = response {
            Ok(session)
        } else {
            Err(DaemonError::ProtocolError(
                "unexpected response type".to_string(),
            ))
        }
    }

    /// Attach to a session's PTY output.
    ///
    /// Sends an attach request and waits for acknowledgment. After this call,
    /// use `read_next()` to receive streaming `PtyOutput` messages.
    pub async fn attach(
        &mut self,
        session_id: &str,
        rows: u16,
        cols: u16,
    ) -> Result<(), DaemonError> {
        let id = self.next_id();
        let msg = ClientMessage::Attach {
            id,
            session_id: session_id.to_string(),
            rows,
            cols,
        };

        let response = self.request(&msg).await?;
        Self::check_error(&response)?;
        Ok(())
    }

    /// Detach from a session.
    pub async fn detach(&mut self, session_id: &str) -> Result<(), DaemonError> {
        let id = self.next_id();
        let msg = ClientMessage::Detach {
            id,
            session_id: session_id.to_string(),
        };

        let response = self.request(&msg).await?;
        Self::check_error(&response)?;
        Ok(())
    }

    /// Resize a session's PTY.
    pub async fn resize_pty(
        &mut self,
        session_id: &str,
        rows: u16,
        cols: u16,
    ) -> Result<(), DaemonError> {
        let id = self.next_id();
        let msg = ClientMessage::ResizePty {
            id,
            session_id: session_id.to_string(),
            rows,
            cols,
        };

        let response = self.request(&msg).await?;
        Self::check_error(&response)?;
        Ok(())
    }

    /// Write data to a session's PTY stdin.
    pub async fn write_stdin(&mut self, session_id: &str, data: &[u8]) -> Result<(), DaemonError> {
        use base64::Engine;
        let id = self.next_id();
        let encoded = base64::engine::general_purpose::STANDARD.encode(data);
        let msg = ClientMessage::WriteStdin {
            id,
            session_id: session_id.to_string(),
            data: encoded,
        };

        let response = self.request(&msg).await?;
        Self::check_error(&response)?;
        Ok(())
    }

    /// Stop a session's agent process.
    pub async fn stop_session(&mut self, session_id: &str) -> Result<(), DaemonError> {
        let id = self.next_id();
        let msg = ClientMessage::StopSession {
            id,
            session_id: session_id.to_string(),
        };

        let response = self.request(&msg).await?;
        Self::check_error(&response)?;
        Ok(())
    }

    /// Destroy a session.
    pub async fn destroy_session(
        &mut self,
        session_id: &str,
        force: bool,
    ) -> Result<(), DaemonError> {
        let id = self.next_id();
        let msg = ClientMessage::DestroySession {
            id,
            session_id: session_id.to_string(),
            force,
        };

        let response = self.request(&msg).await?;
        Self::check_error(&response)?;
        Ok(())
    }

    /// List all sessions, optionally filtered by project.
    pub async fn list_sessions(
        &mut self,
        project_id: Option<&str>,
    ) -> Result<Vec<SessionInfo>, DaemonError> {
        let id = self.next_id();
        let msg = ClientMessage::ListSessions {
            id,
            project_id: project_id.map(String::from),
        };

        let response = self.request(&msg).await?;
        Self::check_error(&response)?;

        if let DaemonMessage::SessionList { sessions, .. } = response {
            Ok(sessions)
        } else {
            Err(DaemonError::ProtocolError(
                "unexpected response type".to_string(),
            ))
        }
    }

    /// Get details for a single session.
    pub async fn get_session(&mut self, session_id: &str) -> Result<SessionInfo, DaemonError> {
        let id = self.next_id();
        let msg = ClientMessage::GetSession {
            id,
            session_id: session_id.to_string(),
        };

        let response = self.request(&msg).await?;
        Self::check_error(&response)?;

        if let DaemonMessage::SessionInfo { session, .. } = response {
            Ok(session)
        } else {
            Err(DaemonError::ProtocolError(
                "unexpected response type".to_string(),
            ))
        }
    }

    /// Request daemon shutdown.
    pub async fn shutdown(&mut self) -> Result<(), DaemonError> {
        let id = self.next_id();
        let msg = ClientMessage::DaemonStop { id };

        let response = self.request(&msg).await?;
        Self::check_error(&response)?;
        Ok(())
    }

    /// Read the next daemon message (for streaming after attach).
    pub async fn read_next(&mut self) -> Result<Option<DaemonMessage>, DaemonError> {
        let msg = read_message(&mut self.reader).await?;
        if let Some(ref m) = msg {
            Self::check_error(m)?;
        }
        Ok(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_error_session_not_found() {
        let msg = DaemonMessage::Error {
            id: "req-1".to_string(),
            code: ErrorCode::SessionNotFound,
            message: "no such session".to_string(),
        };
        let result = DaemonClient::check_error(&msg);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DaemonError::SessionNotFound(_)
        ));
    }

    #[test]
    fn test_check_error_session_already_exists() {
        let msg = DaemonMessage::Error {
            id: "req-2".to_string(),
            code: ErrorCode::SessionAlreadyExists,
            message: "duplicate".to_string(),
        };
        let result = DaemonClient::check_error(&msg);
        assert!(matches!(
            result.unwrap_err(),
            DaemonError::SessionAlreadyExists(_)
        ));
    }

    #[test]
    fn test_check_error_session_not_running() {
        let msg = DaemonMessage::Error {
            id: "req-3".to_string(),
            code: ErrorCode::SessionNotRunning,
            message: "stopped".to_string(),
        };
        let result = DaemonClient::check_error(&msg);
        assert!(matches!(
            result.unwrap_err(),
            DaemonError::SessionNotRunning(_)
        ));
    }

    #[test]
    fn test_check_error_pty_error() {
        let msg = DaemonMessage::Error {
            id: "req-4".to_string(),
            code: ErrorCode::PtyError,
            message: "alloc failed".to_string(),
        };
        let result = DaemonClient::check_error(&msg);
        assert!(matches!(result.unwrap_err(), DaemonError::PtyError(_)));
    }

    #[test]
    fn test_check_error_unknown_code_maps_to_protocol_error() {
        let msg = DaemonMessage::Error {
            id: "req-5".to_string(),
            code: ErrorCode::Unknown,
            message: "unexpected".to_string(),
        };
        let result = DaemonClient::check_error(&msg);
        assert!(matches!(result.unwrap_err(), DaemonError::ProtocolError(_)));
    }

    #[test]
    fn test_check_error_non_error_message_ok() {
        let msg = DaemonMessage::Ack {
            id: "req-1".to_string(),
        };
        assert!(DaemonClient::check_error(&msg).is_ok());
    }
}
