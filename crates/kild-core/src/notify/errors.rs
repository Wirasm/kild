//! Notification error types.

use crate::errors::KildError;

#[derive(Debug, thiserror::Error)]
pub enum NotifyError {
    #[error("Notification tool not found: {tool}")]
    ToolNotFound { tool: String },

    #[error("Notification failed: {message}")]
    SendFailed { message: String },

    #[error("IO error during notification: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
}

impl KildError for NotifyError {
    fn error_code(&self) -> &'static str {
        match self {
            NotifyError::ToolNotFound { .. } => "NOTIFY_TOOL_NOT_FOUND",
            NotifyError::SendFailed { .. } => "NOTIFY_SEND_FAILED",
            NotifyError::IoError { .. } => "NOTIFY_IO_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(self, NotifyError::ToolNotFound { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_not_found() {
        let error = NotifyError::ToolNotFound {
            tool: "notify-send".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Notification tool not found: notify-send"
        );
        assert_eq!(error.error_code(), "NOTIFY_TOOL_NOT_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_send_failed() {
        let error = NotifyError::SendFailed {
            message: "osascript exited with code 1".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Notification failed: osascript exited with code 1"
        );
        assert_eq!(error.error_code(), "NOTIFY_SEND_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_io_error() {
        let error = NotifyError::IoError {
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
        };
        assert!(error.to_string().contains("IO error"));
        assert_eq!(error.error_code(), "NOTIFY_IO_ERROR");
        assert!(!error.is_user_error());
    }
}
