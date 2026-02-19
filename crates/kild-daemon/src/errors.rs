use std::io;

use kild_core::errors::KildError;

/// All error types for the kild-daemon crate.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("daemon not running")]
    NotRunning,

    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("protocol error: {0}")]
    ProtocolError(String),

    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("session already exists: {0}")]
    SessionAlreadyExists(String),

    #[error("session not running: {0}")]
    SessionNotRunning(String),

    #[error("invalid state transition: {0}")]
    InvalidStateTransition(String),

    #[error("PTY error: {0}")]
    PtyError(String),

    #[error("invalid daemon config: {0}")]
    ConfigInvalid(String),

    #[error("daemon already running (pid {0})")]
    AlreadyRunning(u32),

    #[error("shutdown timeout exceeded")]
    ShutdownTimeout,

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),

    #[error("session error: {0}")]
    Session(#[from] kild_core::sessions::errors::SessionError),

    #[error("TLS configuration error: {0}")]
    TlsConfig(String),
}

impl KildError for DaemonError {
    fn error_code(&self) -> &'static str {
        match self {
            DaemonError::NotRunning => "daemon_not_running",
            DaemonError::ConnectionFailed(_) => "connection_failed",
            DaemonError::ProtocolError(_) => "protocol_error",
            DaemonError::SessionNotFound(_) => "session_not_found",
            DaemonError::SessionAlreadyExists(_) => "session_already_exists",
            DaemonError::SessionNotRunning(_) => "session_not_running",
            DaemonError::InvalidStateTransition(_) => "invalid_state_transition",
            DaemonError::PtyError(_) => "pty_error",
            DaemonError::ConfigInvalid(_) => "config_invalid",
            DaemonError::AlreadyRunning(_) => "daemon_already_running",
            DaemonError::ShutdownTimeout => "shutdown_timeout",
            DaemonError::Io(_) => "io_error",
            DaemonError::Serde(_) => "serialization_error",
            DaemonError::Base64Decode(_) => "base64_decode_error",
            DaemonError::Session(_) => "session_error",
            DaemonError::TlsConfig(_) => "tls_config_error",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            DaemonError::SessionNotFound(_)
                | DaemonError::SessionAlreadyExists(_)
                | DaemonError::SessionNotRunning(_)
                | DaemonError::ConfigInvalid(_)
                | DaemonError::AlreadyRunning(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = DaemonError::SessionNotFound("test-session".to_string());
        assert_eq!(err.to_string(), "session not found: test-session");
        assert_eq!(err.error_code(), "session_not_found");
        assert!(err.is_user_error());
    }

    #[test]
    fn test_error_codes() {
        let cases: Vec<(DaemonError, &str)> = vec![
            (DaemonError::NotRunning, "daemon_not_running"),
            (
                DaemonError::ConnectionFailed("refused".to_string()),
                "connection_failed",
            ),
            (
                DaemonError::ProtocolError("bad json".to_string()),
                "protocol_error",
            ),
            (
                DaemonError::SessionNotFound("x".to_string()),
                "session_not_found",
            ),
            (
                DaemonError::SessionAlreadyExists("x".to_string()),
                "session_already_exists",
            ),
            (
                DaemonError::SessionNotRunning("x".to_string()),
                "session_not_running",
            ),
            (
                DaemonError::PtyError("alloc failed".to_string()),
                "pty_error",
            ),
            (
                DaemonError::ConfigInvalid("bad value".to_string()),
                "config_invalid",
            ),
            (DaemonError::AlreadyRunning(1234), "daemon_already_running"),
            (DaemonError::ShutdownTimeout, "shutdown_timeout"),
            (
                DaemonError::InvalidStateTransition("test".to_string()),
                "invalid_state_transition",
            ),
        ];

        for (err, expected_code) in cases {
            assert_eq!(err.error_code(), expected_code);
        }
    }

    #[test]
    fn test_user_error_classification() {
        assert!(DaemonError::SessionNotFound("x".to_string()).is_user_error());
        assert!(DaemonError::SessionAlreadyExists("x".to_string()).is_user_error());
        assert!(DaemonError::SessionNotRunning("x".to_string()).is_user_error());
        assert!(DaemonError::AlreadyRunning(123).is_user_error());
        assert!(DaemonError::ConfigInvalid("x".to_string()).is_user_error());

        assert!(!DaemonError::NotRunning.is_user_error());
        assert!(!DaemonError::PtyError("x".to_string()).is_user_error());
        assert!(!DaemonError::ShutdownTimeout.is_user_error());
        assert!(!DaemonError::InvalidStateTransition("x".to_string()).is_user_error());
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let daemon_err: DaemonError = io_err.into();
        assert_eq!(daemon_err.error_code(), "io_error");
        assert!(!daemon_err.is_user_error());
    }
}
