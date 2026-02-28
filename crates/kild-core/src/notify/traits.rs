//! Notification backend trait definition.

use crate::notify::errors::NotifyError;

/// Trait defining the interface for notification backends.
///
/// Each supported platform (macOS, Linux) implements this trait
/// to provide platform-specific desktop notification delivery.
pub trait NotificationBackend: Send + Sync {
    /// The canonical name of this backend (e.g., "macos", "linux").
    fn name(&self) -> &'static str;

    /// Check if this notification backend is available on the system.
    fn is_available(&self) -> bool;

    /// Send a desktop notification.
    ///
    /// # Arguments
    /// * `title` - The notification title
    /// * `message` - The notification body text
    fn send(&self, title: &str, message: &str) -> Result<(), NotifyError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockBackend {
        available: bool,
    }

    impl NotificationBackend for MockBackend {
        fn name(&self) -> &'static str {
            "mock"
        }

        fn is_available(&self) -> bool {
            self.available
        }

        fn send(&self, _title: &str, _message: &str) -> Result<(), NotifyError> {
            if self.available {
                Ok(())
            } else {
                Err(NotifyError::ToolNotFound {
                    tool: "mock".to_string(),
                })
            }
        }
    }

    #[test]
    fn mock_notification_backend_available() {
        let backend = MockBackend { available: true };
        assert_eq!(backend.name(), "mock");
        assert!(backend.is_available());
        assert!(backend.send("Test", "Hello").is_ok());
    }

    #[test]
    fn mock_notification_backend_unavailable() {
        let backend = MockBackend { available: false };
        assert!(!backend.is_available());
        assert!(backend.send("Test", "Hello").is_err());
    }
}
