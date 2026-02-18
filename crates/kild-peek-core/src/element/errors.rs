use crate::errors::PeekError;

#[derive(Debug, thiserror::Error)]
pub enum ElementError {
    #[error(
        "Accessibility permission required: enable in System Settings > Privacy & Security > Accessibility"
    )]
    AccessibilityPermissionDenied,

    #[error("Window not found: '{title}'")]
    WindowNotFound { title: String },

    #[error("Window not found for app: '{app}'")]
    WindowNotFoundByApp { app: String },

    #[error("No element found with text: '{text}'")]
    ElementNotFound { text: String },

    #[error("Multiple elements found with text '{text}': found {count}, expected 1")]
    ElementAmbiguous { text: String, count: usize },

    #[error("Accessibility query failed: {reason}")]
    AccessibilityQueryFailed { reason: String },

    #[error("Window has no PID available (required for Accessibility API)")]
    NoPidAvailable,

    #[error("Window is minimized: '{title}'")]
    WindowMinimized { title: String },

    #[error("Window lookup failed: {reason}")]
    WindowLookupFailed { reason: String },

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

    #[error("Invalid regex pattern '{pattern}': {reason}")]
    InvalidRegex { pattern: String, reason: String },

    #[error("Element with text '{text}' not found after {timeout_ms}ms")]
    WaitTimeoutElementNotFound { text: String, timeout_ms: u64 },

    #[error("Element with text '{text}' still exists after {timeout_ms}ms")]
    WaitTimeoutElementStillExists { text: String, timeout_ms: u64 },
}

impl PeekError for ElementError {
    fn error_code(&self) -> &'static str {
        match self {
            ElementError::AccessibilityPermissionDenied => "ELEMENT_ACCESSIBILITY_DENIED",
            ElementError::WindowNotFound { .. } => "ELEMENT_WINDOW_NOT_FOUND",
            ElementError::WindowNotFoundByApp { .. } => "ELEMENT_WINDOW_NOT_FOUND_BY_APP",
            ElementError::ElementNotFound { .. } => "ELEMENT_NOT_FOUND",
            ElementError::ElementAmbiguous { .. } => "ELEMENT_AMBIGUOUS",
            ElementError::AccessibilityQueryFailed { .. } => "ELEMENT_QUERY_FAILED",
            ElementError::NoPidAvailable => "ELEMENT_NO_PID",
            ElementError::WindowMinimized { .. } => "ELEMENT_WINDOW_MINIMIZED",
            ElementError::WindowLookupFailed { .. } => "ELEMENT_WINDOW_LOOKUP_FAILED",
            ElementError::WaitTimeoutByTitle { .. } => "ELEMENT_WAIT_TIMEOUT_BY_TITLE",
            ElementError::WaitTimeoutByApp { .. } => "ELEMENT_WAIT_TIMEOUT_BY_APP",
            ElementError::WaitTimeoutByAppAndTitle { .. } => {
                "ELEMENT_WAIT_TIMEOUT_BY_APP_AND_TITLE"
            }
            ElementError::InvalidRegex { .. } => "ELEMENT_INVALID_REGEX",
            ElementError::WaitTimeoutElementNotFound { .. } => "ELEMENT_WAIT_TIMEOUT_NOT_FOUND",
            ElementError::WaitTimeoutElementStillExists { .. } => {
                "ELEMENT_WAIT_TIMEOUT_STILL_EXISTS"
            }
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            ElementError::AccessibilityPermissionDenied
                | ElementError::WindowNotFound { .. }
                | ElementError::WindowNotFoundByApp { .. }
                | ElementError::ElementNotFound { .. }
                | ElementError::ElementAmbiguous { .. }
                | ElementError::NoPidAvailable
                | ElementError::WindowMinimized { .. }
                | ElementError::WindowLookupFailed { .. }
                | ElementError::WaitTimeoutByTitle { .. }
                | ElementError::WaitTimeoutByApp { .. }
                | ElementError::WaitTimeoutByAppAndTitle { .. }
                | ElementError::InvalidRegex { .. }
                | ElementError::WaitTimeoutElementNotFound { .. }
                | ElementError::WaitTimeoutElementStillExists { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_accessibility_denied_error() {
        let error = ElementError::AccessibilityPermissionDenied;
        assert!(error.to_string().contains("Accessibility permission"));
        assert_eq!(error.error_code(), "ELEMENT_ACCESSIBILITY_DENIED");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_window_not_found_error() {
        let error = ElementError::WindowNotFound {
            title: "Test Window".to_string(),
        };
        assert_eq!(error.to_string(), "Window not found: 'Test Window'");
        assert_eq!(error.error_code(), "ELEMENT_WINDOW_NOT_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_window_not_found_by_app_error() {
        let error = ElementError::WindowNotFoundByApp {
            app: "TestApp".to_string(),
        };
        assert_eq!(error.to_string(), "Window not found for app: 'TestApp'");
        assert_eq!(error.error_code(), "ELEMENT_WINDOW_NOT_FOUND_BY_APP");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_element_not_found_error() {
        let error = ElementError::ElementNotFound {
            text: "Submit".to_string(),
        };
        assert_eq!(error.to_string(), "No element found with text: 'Submit'");
        assert_eq!(error.error_code(), "ELEMENT_NOT_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_element_ambiguous_error() {
        let error = ElementError::ElementAmbiguous {
            text: "OK".to_string(),
            count: 3,
        };
        assert_eq!(
            error.to_string(),
            "Multiple elements found with text 'OK': found 3, expected 1"
        );
        assert_eq!(error.error_code(), "ELEMENT_AMBIGUOUS");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_accessibility_query_failed_error() {
        let error = ElementError::AccessibilityQueryFailed {
            reason: "timeout".to_string(),
        };
        assert_eq!(error.to_string(), "Accessibility query failed: timeout");
        assert_eq!(error.error_code(), "ELEMENT_QUERY_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_no_pid_available_error() {
        let error = ElementError::NoPidAvailable;
        assert!(error.to_string().contains("no PID available"));
        assert_eq!(error.error_code(), "ELEMENT_NO_PID");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_window_minimized_error() {
        let error = ElementError::WindowMinimized {
            title: "Terminal".to_string(),
        };
        assert_eq!(error.to_string(), "Window is minimized: 'Terminal'");
        assert_eq!(error.error_code(), "ELEMENT_WINDOW_MINIMIZED");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_window_lookup_failed_error() {
        let error = ElementError::WindowLookupFailed {
            reason: "enumeration failed".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Window lookup failed: enumeration failed"
        );
        assert_eq!(error.error_code(), "ELEMENT_WINDOW_LOOKUP_FAILED");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_wait_timeout_by_title_error() {
        let error = ElementError::WaitTimeoutByTitle {
            title: "Test Window".to_string(),
            timeout_ms: 5000,
        };
        assert_eq!(
            error.to_string(),
            "Window 'Test Window' not found after 5000ms"
        );
        assert_eq!(error.error_code(), "ELEMENT_WAIT_TIMEOUT_BY_TITLE");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_wait_timeout_by_app_error() {
        let error = ElementError::WaitTimeoutByApp {
            app: "Ghostty".to_string(),
            timeout_ms: 3000,
        };
        assert_eq!(
            error.to_string(),
            "Window for app 'Ghostty' not found after 3000ms"
        );
        assert_eq!(error.error_code(), "ELEMENT_WAIT_TIMEOUT_BY_APP");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_wait_timeout_by_app_and_title_error() {
        let error = ElementError::WaitTimeoutByAppAndTitle {
            app: "Ghostty".to_string(),
            title: "Terminal".to_string(),
            timeout_ms: 10000,
        };
        assert_eq!(
            error.to_string(),
            "Window 'Terminal' in app 'Ghostty' not found after 10000ms"
        );
        assert_eq!(error.error_code(), "ELEMENT_WAIT_TIMEOUT_BY_APP_AND_TITLE");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ElementError>();
    }

    #[test]
    fn test_error_source() {
        let error = ElementError::ElementNotFound {
            text: "test".to_string(),
        };
        assert!(error.source().is_none());
    }

    #[test]
    fn test_invalid_regex_error() {
        let error = ElementError::InvalidRegex {
            pattern: "[invalid".to_string(),
            reason: "unclosed character class".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Invalid regex pattern '[invalid': unclosed character class"
        );
        assert_eq!(error.error_code(), "ELEMENT_INVALID_REGEX");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_element_not_found_empty_text() {
        // Edge case: empty text search returns ElementNotFound with empty text
        let error = ElementError::ElementNotFound {
            text: String::new(),
        };
        assert_eq!(error.to_string(), "No element found with text: ''");
        assert_eq!(error.error_code(), "ELEMENT_NOT_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_wait_timeout_element_not_found_error() {
        let error = ElementError::WaitTimeoutElementNotFound {
            text: "Submit".to_string(),
            timeout_ms: 5000,
        };
        assert_eq!(
            error.to_string(),
            "Element with text 'Submit' not found after 5000ms"
        );
        assert_eq!(error.error_code(), "ELEMENT_WAIT_TIMEOUT_NOT_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_wait_timeout_element_still_exists_error() {
        let error = ElementError::WaitTimeoutElementStillExists {
            text: "Loading...".to_string(),
            timeout_ms: 3000,
        };
        assert_eq!(
            error.to_string(),
            "Element with text 'Loading...' still exists after 3000ms"
        );
        assert_eq!(error.error_code(), "ELEMENT_WAIT_TIMEOUT_STILL_EXISTS");
        assert!(error.is_user_error());
    }
}
