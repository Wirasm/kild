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

#[cfg(test)]
mod tests {
    use super::*;
    use kild_protocol::{ErrorCode, IpcError};

    #[test]
    fn test_from_ipc_error_not_running() {
        let ipc_err = IpcError::NotRunning {
            path: "/tmp/test.sock".to_string(),
        };
        let shim_err: ShimError = ipc_err.into();
        assert!(matches!(shim_err, ShimError::DaemonNotRunning));
    }

    #[test]
    fn test_from_ipc_error_connection_failed() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "failed");
        let ipc_err = IpcError::ConnectionFailed(io_err);
        let shim_err: ShimError = ipc_err.into();
        assert!(
            matches!(shim_err, ShimError::IpcError { ref message } if message.contains("Connection failed")),
            "got: {:?}",
            shim_err
        );
    }

    #[test]
    fn test_from_ipc_error_daemon_error() {
        let ipc_err = IpcError::DaemonError {
            code: ErrorCode::SessionNotFound,
            message: "not found".to_string(),
        };
        let shim_err: ShimError = ipc_err.into();
        assert!(
            matches!(shim_err, ShimError::IpcError { ref message } if message.contains("session_not_found")),
            "got: {:?}",
            shim_err
        );
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
