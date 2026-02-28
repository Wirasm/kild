//! Notification error types.

use crate::errors::KildError;

#[derive(Debug, thiserror::Error)]
pub enum NotifyError {
    #[error("Notification tool not found: {tool}")]
    ToolNotFound { tool: String },

    #[error("Notification failed: {message}")]
    SendFailed { message: String },
}

impl KildError for NotifyError {
    fn error_code(&self) -> &'static str {
        match self {
            NotifyError::ToolNotFound { .. } => "NOTIFY_TOOL_NOT_FOUND",
            NotifyError::SendFailed { .. } => "NOTIFY_SEND_FAILED",
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
}
