use crate::errors::PeekError;

#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("Failed to enumerate windows: {message}")]
    EnumerationFailed { message: String },

    #[error("Window not found: '{title}'")]
    WindowNotFound { title: String },

    #[error("Window not found with id: {id}")]
    WindowNotFoundById { id: u32 },

    #[error("Failed to enumerate monitors: {message}")]
    MonitorEnumerationFailed { message: String },

    #[error("Monitor not found at index: {index}")]
    MonitorNotFound { index: usize },
}

impl PeekError for WindowError {
    fn error_code(&self) -> &'static str {
        match self {
            WindowError::EnumerationFailed { .. } => "WINDOW_ENUMERATION_FAILED",
            WindowError::WindowNotFound { .. } => "WINDOW_NOT_FOUND",
            WindowError::WindowNotFoundById { .. } => "WINDOW_NOT_FOUND_BY_ID",
            WindowError::MonitorEnumerationFailed { .. } => "MONITOR_ENUMERATION_FAILED",
            WindowError::MonitorNotFound { .. } => "MONITOR_NOT_FOUND",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            WindowError::WindowNotFound { .. }
                | WindowError::WindowNotFoundById { .. }
                | WindowError::MonitorNotFound { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_window_error_display() {
        let error = WindowError::WindowNotFound {
            title: "Test Window".to_string(),
        };
        assert_eq!(error.to_string(), "Window not found: 'Test Window'");
        assert_eq!(error.error_code(), "WINDOW_NOT_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_enumeration_error() {
        let error = WindowError::EnumerationFailed {
            message: "permission denied".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Failed to enumerate windows: permission denied"
        );
        assert_eq!(error.error_code(), "WINDOW_ENUMERATION_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WindowError>();
    }

    #[test]
    fn test_error_source() {
        let error = WindowError::WindowNotFound {
            title: "test".to_string(),
        };
        assert!(error.source().is_none());
    }
}
