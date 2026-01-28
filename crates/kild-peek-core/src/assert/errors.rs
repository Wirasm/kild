use crate::errors::PeekError;

#[derive(Debug, thiserror::Error)]
pub enum AssertError {
    #[error("Assertion failed: {message}")]
    AssertionFailed { message: String },

    #[error("Window not found: '{title}'")]
    WindowNotFound { title: String },

    #[error("Element not found matching query")]
    ElementNotFound,

    #[error("Image comparison failed: {0}")]
    ImageComparisonFailed(String),

    #[error("Failed to load image: {0}")]
    ImageLoadFailed(String),

    #[error("Screenshot capture failed: {0}")]
    ScreenshotFailed(String),
}

impl PeekError for AssertError {
    fn error_code(&self) -> &'static str {
        match self {
            AssertError::AssertionFailed { .. } => "ASSERT_FAILED",
            AssertError::WindowNotFound { .. } => "ASSERT_WINDOW_NOT_FOUND",
            AssertError::ElementNotFound => "ASSERT_ELEMENT_NOT_FOUND",
            AssertError::ImageComparisonFailed(_) => "ASSERT_IMAGE_COMPARISON_FAILED",
            AssertError::ImageLoadFailed(_) => "ASSERT_IMAGE_LOAD_FAILED",
            AssertError::ScreenshotFailed(_) => "ASSERT_SCREENSHOT_FAILED",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            AssertError::AssertionFailed { .. }
                | AssertError::WindowNotFound { .. }
                | AssertError::ElementNotFound
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assert_error_display() {
        let error = AssertError::AssertionFailed {
            message: "Window not visible".to_string(),
        };
        assert_eq!(error.to_string(), "Assertion failed: Window not visible");
        assert_eq!(error.error_code(), "ASSERT_FAILED");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_window_not_found_error() {
        let error = AssertError::WindowNotFound {
            title: "Terminal".to_string(),
        };
        assert_eq!(error.to_string(), "Window not found: 'Terminal'");
        assert_eq!(error.error_code(), "ASSERT_WINDOW_NOT_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_image_comparison_failed_error() {
        let error = AssertError::ImageComparisonFailed("SSIM calculation failed".to_string());
        assert_eq!(
            error.to_string(),
            "Image comparison failed: SSIM calculation failed"
        );
        assert_eq!(error.error_code(), "ASSERT_IMAGE_COMPARISON_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AssertError>();
    }
}
