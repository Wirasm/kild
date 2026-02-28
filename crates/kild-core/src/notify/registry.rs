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
    fn detect(&self) -> Option<&dyn NotificationBackend> {
        self.backends.iter().find_map(|b| {
            if b.is_available() {
                Some(b.as_ref())
            } else {
                None
            }
        })
    }
}

/// Detect the first available notification backend for the current platform.
///
/// Returns `None` on unsupported platforms or when no notification tools are installed.
pub fn detect_backend() -> Option<&'static dyn NotificationBackend> {
    REGISTRY.detect()
}

/// Send a notification via the first available platform backend.
///
/// Best-effort: returns the backend's `Result` to let callers decide how
/// to handle failures. Returns `Ok(())` with a debug log if no backend is available.
pub fn send_via_backend(title: &str, message: &str) -> Result<(), super::errors::NotifyError> {
    let Some(backend) = detect_backend() else {
        debug!(
            event = "core.notify.send_skipped",
            reason = "no backend available",
        );
        return Ok(());
    };

    backend.send(title, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_backend_does_not_panic() {
        // Should never panic regardless of platform
        let _result = detect_backend();
    }

    #[test]
    fn registry_contains_expected_backends() {
        let registry = NotificationRegistry::new();
        let names: Vec<&str> = registry.backends.iter().map(|b| b.name()).collect();
        assert!(names.contains(&"macos"));
        assert!(names.contains(&"linux"));
    }

    #[test]
    fn send_via_backend_does_not_panic() {
        // Best-effort: should never panic even if no backend available
        let _result = send_via_backend("Test", "Hello");
    }
}
