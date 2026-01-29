use crate::errors::PeekError;

#[derive(Debug, thiserror::Error)]
pub enum InteractionError {
    #[error(
        "Accessibility permission required: enable in System Settings > Privacy & Security > Accessibility"
    )]
    AccessibilityPermissionDenied,

    #[error("Window not found: '{title}'")]
    WindowNotFound { title: String },

    #[error("Window not found for app: '{app}'")]
    WindowNotFoundByApp { app: String },

    #[error("Failed to create event source")]
    EventSourceFailed,

    #[error("Failed to create mouse event at ({x}, {y})")]
    MouseEventFailed { x: f64, y: f64 },

    #[error("Failed to create keyboard event for keycode {keycode}")]
    KeyboardEventFailed { keycode: u16 },

    #[error("Unknown key name: '{name}'")]
    UnknownKey { name: String },

    #[error("Invalid coordinate: ({x}, {y}) is outside window bounds ({width}x{height})")]
    CoordinateOutOfBounds {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },

    #[error("Window is minimized: '{title}'")]
    WindowMinimized { title: String },
}

impl PeekError for InteractionError {
    fn error_code(&self) -> &'static str {
        match self {
            InteractionError::AccessibilityPermissionDenied => "INTERACTION_ACCESSIBILITY_DENIED",
            InteractionError::WindowNotFound { .. } => "INTERACTION_WINDOW_NOT_FOUND",
            InteractionError::WindowNotFoundByApp { .. } => "INTERACTION_WINDOW_NOT_FOUND_BY_APP",
            InteractionError::EventSourceFailed => "INTERACTION_EVENT_SOURCE_FAILED",
            InteractionError::MouseEventFailed { .. } => "INTERACTION_MOUSE_EVENT_FAILED",
            InteractionError::KeyboardEventFailed { .. } => "INTERACTION_KEYBOARD_EVENT_FAILED",
            InteractionError::UnknownKey { .. } => "INTERACTION_UNKNOWN_KEY",
            InteractionError::CoordinateOutOfBounds { .. } => {
                "INTERACTION_COORDINATE_OUT_OF_BOUNDS"
            }
            InteractionError::WindowMinimized { .. } => "INTERACTION_WINDOW_MINIMIZED",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            InteractionError::AccessibilityPermissionDenied
                | InteractionError::WindowNotFound { .. }
                | InteractionError::WindowNotFoundByApp { .. }
                | InteractionError::UnknownKey { .. }
                | InteractionError::CoordinateOutOfBounds { .. }
                | InteractionError::WindowMinimized { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_accessibility_error() {
        let error = InteractionError::AccessibilityPermissionDenied;
        assert!(error.to_string().contains("Accessibility permission"));
        assert_eq!(error.error_code(), "INTERACTION_ACCESSIBILITY_DENIED");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_window_not_found_error() {
        let error = InteractionError::WindowNotFound {
            title: "Test Window".to_string(),
        };
        assert_eq!(error.to_string(), "Window not found: 'Test Window'");
        assert_eq!(error.error_code(), "INTERACTION_WINDOW_NOT_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_window_not_found_by_app_error() {
        let error = InteractionError::WindowNotFoundByApp {
            app: "TestApp".to_string(),
        };
        assert_eq!(error.to_string(), "Window not found for app: 'TestApp'");
        assert_eq!(error.error_code(), "INTERACTION_WINDOW_NOT_FOUND_BY_APP");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_event_source_failed_error() {
        let error = InteractionError::EventSourceFailed;
        assert_eq!(error.to_string(), "Failed to create event source");
        assert_eq!(error.error_code(), "INTERACTION_EVENT_SOURCE_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_mouse_event_failed_error() {
        let error = InteractionError::MouseEventFailed { x: 100.0, y: 200.0 };
        assert_eq!(
            error.to_string(),
            "Failed to create mouse event at (100, 200)"
        );
        assert_eq!(error.error_code(), "INTERACTION_MOUSE_EVENT_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_keyboard_event_failed_error() {
        let error = InteractionError::KeyboardEventFailed { keycode: 36 };
        assert_eq!(
            error.to_string(),
            "Failed to create keyboard event for keycode 36"
        );
        assert_eq!(error.error_code(), "INTERACTION_KEYBOARD_EVENT_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_unknown_key_error() {
        let error = InteractionError::UnknownKey {
            name: "foobar".to_string(),
        };
        assert_eq!(error.to_string(), "Unknown key name: 'foobar'");
        assert_eq!(error.error_code(), "INTERACTION_UNKNOWN_KEY");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_coordinate_out_of_bounds_error() {
        let error = InteractionError::CoordinateOutOfBounds {
            x: 999,
            y: 999,
            width: 800,
            height: 600,
        };
        assert_eq!(
            error.to_string(),
            "Invalid coordinate: (999, 999) is outside window bounds (800x600)"
        );
        assert_eq!(error.error_code(), "INTERACTION_COORDINATE_OUT_OF_BOUNDS");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_window_minimized_error() {
        let error = InteractionError::WindowMinimized {
            title: "Terminal".to_string(),
        };
        assert_eq!(error.to_string(), "Window is minimized: 'Terminal'");
        assert_eq!(error.error_code(), "INTERACTION_WINDOW_MINIMIZED");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<InteractionError>();
    }

    #[test]
    fn test_error_source() {
        let error = InteractionError::WindowNotFound {
            title: "test".to_string(),
        };
        assert!(error.source().is_none());
    }
}
