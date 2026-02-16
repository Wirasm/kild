use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum ShimError {
    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("State error: {message}")]
    StateError { message: String },

    #[error("IPC error: {message}")]
    IpcError { message: String },

    #[error("Parse error: {message}")]
    ParseError { message: String },

    #[error("Daemon is not running (socket not found)")]
    DaemonNotRunning,
}

impl From<kild_protocol::IpcError> for ShimError {
    fn from(e: kild_protocol::IpcError) -> Self {
        match e {
            kild_protocol::IpcError::NotRunning { .. } => ShimError::DaemonNotRunning,
            other => ShimError::IpcError {
                message: other.to_string(),
            },
        }
    }
}

impl ShimError {
    pub fn parse(msg: impl fmt::Display) -> Self {
        Self::ParseError {
            message: msg.to_string(),
        }
    }

    pub fn state(msg: impl fmt::Display) -> Self {
        Self::StateError {
            message: msg.to_string(),
        }
    }

    pub fn ipc(msg: impl fmt::Display) -> Self {
        Self::IpcError {
            message: msg.to_string(),
        }
    }
}
