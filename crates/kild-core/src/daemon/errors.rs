use crate::errors::KildError;

/// Errors from daemon auto-start operations.
#[derive(Debug, thiserror::Error)]
pub enum DaemonAutoStartError {
    #[error(
        "Daemon is not running. To fix this, either:\n  \
         - Start it manually: kild daemon start\n  \
         - Enable auto-start in config: [daemon] auto_start = true\n  \
         - Use --no-daemon to launch in an external terminal instead"
    )]
    Disabled,

    #[error("Failed to start daemon: {message}")]
    SpawnFailed { message: String },

    #[error("Daemon auto-start timed out: {message}")]
    Timeout { message: String },

    #[error("Could not determine daemon binary path: {message}")]
    BinaryNotFound { message: String },
}

impl KildError for DaemonAutoStartError {
    fn error_code(&self) -> &'static str {
        match self {
            DaemonAutoStartError::Disabled => "DAEMON_AUTO_START_DISABLED",
            DaemonAutoStartError::SpawnFailed { .. } => "DAEMON_SPAWN_FAILED",
            DaemonAutoStartError::Timeout { .. } => "DAEMON_AUTO_START_TIMEOUT",
            DaemonAutoStartError::BinaryNotFound { .. } => "DAEMON_BINARY_NOT_FOUND",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(self, DaemonAutoStartError::Disabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kild_error_codes() {
        assert_eq!(
            DaemonAutoStartError::Disabled.error_code(),
            "DAEMON_AUTO_START_DISABLED"
        );
        assert_eq!(
            DaemonAutoStartError::SpawnFailed {
                message: "test".to_string()
            }
            .error_code(),
            "DAEMON_SPAWN_FAILED"
        );
        assert_eq!(
            DaemonAutoStartError::Timeout {
                message: "test".to_string()
            }
            .error_code(),
            "DAEMON_AUTO_START_TIMEOUT"
        );
        assert_eq!(
            DaemonAutoStartError::BinaryNotFound {
                message: "test".to_string()
            }
            .error_code(),
            "DAEMON_BINARY_NOT_FOUND"
        );
    }

    #[test]
    fn test_is_user_error() {
        assert!(DaemonAutoStartError::Disabled.is_user_error());
        assert!(
            !DaemonAutoStartError::SpawnFailed {
                message: "test".to_string()
            }
            .is_user_error()
        );
        assert!(
            !DaemonAutoStartError::Timeout {
                message: "test".to_string()
            }
            .is_user_error()
        );
        assert!(
            !DaemonAutoStartError::BinaryNotFound {
                message: "test".to_string()
            }
            .is_user_error()
        );
    }
}
