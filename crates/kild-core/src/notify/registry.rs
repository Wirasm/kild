//! Notification registry for managing and looking up notification backends.

use std::sync::LazyLock;

use tracing::debug;

use super::backends::{LinuxNotificationBackend, MacOsNotificationBackend};
use super::traits::NotificationBackend;

/// Global registry of all supported notification backends.
static REGISTRY: LazyLock<NotificationRegistry> = LazyLock::new(NotificationRegistry::new);

/// Registry that manages all notification backend implementations.
struct NotificationRegistry {
    backends: Vec<Box<dyn NotificationBackend>>,
}

impl NotificationRegistry {
    fn new() -> Self {
        Self {
            backends: vec![
                Box::new(MacOsNotificationBackend),
                Box::new(LinuxNotificationBackend),
            ],
        }
    }

    /// Detect the first available notification backend.
    ///
    /// Returns the first backend (in registration order) that reports
    /// `is_available() == true`. Registration order in `new()` therefore
    /// determines priority.
    fn detect(&self) -> Option<&dyn NotificationBackend> {
        self.backends
            .iter()
            .find(|b| b.is_available())
            .map(|b| b.as_ref())
    }
}

/// Send a notification via the first available platform backend.
///
/// Returns `Ok(true)` if the notification was sent, `Ok(false)` if no backend
/// is available (skipped), or `Err` if the backend failed.
pub fn send_via_backend(title: &str, message: &str) -> Result<bool, super::errors::NotifyError> {
    let Some(backend) = REGISTRY.detect() else {
        debug!(
            event = "core.notify.send_skipped",
            reason = "no backend available",
        );
        return Ok(false);
    };

    backend.send(title, message)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_expected_backends() {
        let registry = NotificationRegistry::new();
        let names: Vec<&str> = registry.backends.iter().map(|b| b.name()).collect();
        assert!(names.contains(&"macos"));
        assert!(names.contains(&"linux"));
    }

    #[test]
    fn send_via_backend_does_not_panic() {
        let _result = send_via_backend("Test", "Hello");
    }
}
