use crate::errors::PeekError;

#[derive(Debug, thiserror::Error)]
pub enum ScreenshotError {
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

    #[error("Window is minimized and cannot be captured: '{title}'")]
    WindowMinimized { title: String },

    #[error(
        "Screen recording permission denied. Enable in System Settings > Privacy & Security > Screen Recording"
    )]
    PermissionDenied,

    #[error("Failed to enumerate windows: {0}")]
    EnumerationFailed(String),

    #[error("Failed to capture image: {0}")]
    CaptureFailed(String),

    #[error("Image encoding failed: {0}")]
    EncodingError(String),

    #[error("Monitor not found at index: {index}")]
    MonitorNotFound { index: usize },

    /// Directory creation failed during screenshot save.
    ///
    /// Use this for mkdir-like failures when creating parent directories.
    /// Use `IoError` for file write failures after directories exist.
    #[error("Failed to create output directory '{path}': {source}")]
    DirectoryCreationFailed {
        path: String,
        source: std::io::Error,
    },

    #[error("IO error: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
}

impl PeekError for ScreenshotError {
    fn error_code(&self) -> &'static str {
        match self {
            ScreenshotError::WindowNotFound { .. } => "SCREENSHOT_WINDOW_NOT_FOUND",
            ScreenshotError::WindowNotFoundById { .. } => "SCREENSHOT_WINDOW_NOT_FOUND_BY_ID",
            ScreenshotError::WindowNotFoundByApp { .. } => "SCREENSHOT_WINDOW_NOT_FOUND_BY_APP",
            ScreenshotError::WaitTimeoutByTitle { .. } => "SCREENSHOT_WAIT_TIMEOUT_BY_TITLE",
            ScreenshotError::WaitTimeoutByApp { .. } => "SCREENSHOT_WAIT_TIMEOUT_BY_APP",
            ScreenshotError::WaitTimeoutByAppAndTitle { .. } => {
                "SCREENSHOT_WAIT_TIMEOUT_BY_APP_AND_TITLE"
            }
            ScreenshotError::WindowMinimized { .. } => "SCREENSHOT_WINDOW_MINIMIZED",
            ScreenshotError::PermissionDenied => "SCREENSHOT_PERMISSION_DENIED",
            ScreenshotError::EnumerationFailed(_) => "SCREENSHOT_ENUMERATION_FAILED",
            ScreenshotError::CaptureFailed(_) => "SCREENSHOT_CAPTURE_FAILED",
            ScreenshotError::EncodingError(_) => "SCREENSHOT_ENCODING_ERROR",
            ScreenshotError::MonitorNotFound { .. } => "SCREENSHOT_MONITOR_NOT_FOUND",
            ScreenshotError::DirectoryCreationFailed { .. } => {
                "SCREENSHOT_DIRECTORY_CREATION_FAILED"
            }
            ScreenshotError::IoError { .. } => "SCREENSHOT_IO_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            ScreenshotError::WindowNotFound { .. }
                | ScreenshotError::WindowNotFoundById { .. }
                | ScreenshotError::WindowNotFoundByApp { .. }
                | ScreenshotError::WaitTimeoutByTitle { .. }
                | ScreenshotError::WaitTimeoutByApp { .. }
                | ScreenshotError::WaitTimeoutByAppAndTitle { .. }
                | ScreenshotError::WindowMinimized { .. }
                | ScreenshotError::PermissionDenied
                | ScreenshotError::MonitorNotFound { .. }
                | ScreenshotError::DirectoryCreationFailed { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_screenshot_error_display() {
        let error = ScreenshotError::WindowNotFound {
            title: "Test Window".to_string(),
        };
        assert_eq!(error.to_string(), "Window not found: 'Test Window'");
        assert_eq!(error.error_code(), "SCREENSHOT_WINDOW_NOT_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_permission_denied_error() {
        let error = ScreenshotError::PermissionDenied;
        assert!(error.to_string().contains("Screen Recording"));
        assert_eq!(error.error_code(), "SCREENSHOT_PERMISSION_DENIED");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_encoding_error() {
        let error = ScreenshotError::EncodingError("invalid format".to_string());
        assert_eq!(error.to_string(), "Image encoding failed: invalid format");
        assert_eq!(error.error_code(), "SCREENSHOT_ENCODING_ERROR");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error: ScreenshotError = io_error.into();
        assert_eq!(error.error_code(), "SCREENSHOT_IO_ERROR");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ScreenshotError>();
    }

    #[test]
    fn test_error_source() {
        let io_error = std::io::Error::new(std::io::ErrorKind::Other, "test");
        let error: ScreenshotError = io_error.into();
        assert!(error.source().is_some());
    }

    #[test]
    fn test_wait_timeout_by_title_error() {
        let error = ScreenshotError::WaitTimeoutByTitle {
            title: "Test Window".to_string(),
            timeout_ms: 5000,
        };
        assert_eq!(
            error.to_string(),
            "Window 'Test Window' not found after 5000ms"
        );
        assert_eq!(error.error_code(), "SCREENSHOT_WAIT_TIMEOUT_BY_TITLE");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_wait_timeout_by_app_error() {
        let error = ScreenshotError::WaitTimeoutByApp {
            app: "Ghostty".to_string(),
            timeout_ms: 5000,
        };
        assert_eq!(
            error.to_string(),
            "Window for app 'Ghostty' not found after 5000ms"
        );
        assert_eq!(error.error_code(), "SCREENSHOT_WAIT_TIMEOUT_BY_APP");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_wait_timeout_by_app_and_title_error() {
        let error = ScreenshotError::WaitTimeoutByAppAndTitle {
            app: "Ghostty".to_string(),
            title: "Terminal".to_string(),
            timeout_ms: 5000,
        };
        assert_eq!(
            error.to_string(),
            "Window 'Terminal' in app 'Ghostty' not found after 5000ms"
        );
        assert_eq!(
            error.error_code(),
            "SCREENSHOT_WAIT_TIMEOUT_BY_APP_AND_TITLE"
        );
        assert!(error.is_user_error());
    }
}
