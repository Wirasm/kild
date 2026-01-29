use crate::errors::PeekError;

#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("Failed to enumerate windows: {message}")]
    EnumerationFailed { message: String },

    #[error("Window not found: '{title}'")]
    WindowNotFound { title: String },

    #[error("Window not found with id: {id}")]
    WindowNotFoundById { id: u32 },

    #[error("Window not found for app: '{app}'")]
    WindowNotFoundByApp { app: String },

    #[error("Window '{title}' not found after {timeout_ms}ms")]
    WaitTimeoutByTitle { title: String, timeout_ms: u64 },

    #[error("Window for app '{app}' not found after {timeout_ms}ms")]
    WaitTimeoutByApp { app: String, timeout_ms: u64 },

    #[error("Window '{title}' in app '{app}' not found after {timeout_ms}ms")]
    WaitTimeoutByAppAndTitle {
        app: String,
        title: String,
        timeout_ms: u64,
    },

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
            WindowError::WindowNotFoundByApp { .. } => "WINDOW_NOT_FOUND_BY_APP",
            WindowError::WaitTimeoutByTitle { .. } => "WINDOW_WAIT_TIMEOUT_BY_TITLE",
            WindowError::WaitTimeoutByApp { .. } => "WINDOW_WAIT_TIMEOUT_BY_APP",
            WindowError::WaitTimeoutByAppAndTitle { .. } => "WINDOW_WAIT_TIMEOUT_BY_APP_AND_TITLE",
            WindowError::MonitorEnumerationFailed { .. } => "MONITOR_ENUMERATION_FAILED",
            WindowError::MonitorNotFound { .. } => "MONITOR_NOT_FOUND",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            WindowError::WindowNotFound { .. }
                | WindowError::WindowNotFoundById { .. }
                | WindowError::WindowNotFoundByApp { .. }
                | WindowError::WaitTimeoutByTitle { .. }
                | WindowError::WaitTimeoutByApp { .. }
                | WindowError::WaitTimeoutByAppAndTitle { .. }
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

    #[test]
    fn test_wait_timeout_by_title_error() {
        let error = WindowError::WaitTimeoutByTitle {
            title: "Test Window".to_string(),
            timeout_ms: 5000,
        };
        assert_eq!(
            error.to_string(),
            "Window 'Test Window' not found after 5000ms"
        );
        assert_eq!(error.error_code(), "WINDOW_WAIT_TIMEOUT_BY_TITLE");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_wait_timeout_by_app_error() {
        let error = WindowError::WaitTimeoutByApp {
            app: "Ghostty".to_string(),
            timeout_ms: 5000,
        };
        assert_eq!(
            error.to_string(),
            "Window for app 'Ghostty' not found after 5000ms"
        );
        assert_eq!(error.error_code(), "WINDOW_WAIT_TIMEOUT_BY_APP");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_wait_timeout_by_app_and_title_error() {
        let error = WindowError::WaitTimeoutByAppAndTitle {
            app: "Ghostty".to_string(),
            title: "Terminal".to_string(),
            timeout_ms: 5000,
        };
        assert_eq!(
            error.to_string(),
            "Window 'Terminal' in app 'Ghostty' not found after 5000ms"
        );
        assert_eq!(error.error_code(), "WINDOW_WAIT_TIMEOUT_BY_APP_AND_TITLE");
        assert!(error.is_user_error());
    }
}
