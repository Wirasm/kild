//! Terminal registry for managing and looking up terminal backends.

use std::collections::HashMap;
use std::sync::LazyLock;

#[cfg(target_os = "macos")]
use tracing::debug;
use tracing::warn;

use super::backends::{GhosttyBackend, ITermBackend, TerminalAppBackend};
use super::errors::TerminalError;
use super::traits::TerminalBackend;
use super::types::TerminalType;

/// Global registry of all supported terminal backends.
static REGISTRY: LazyLock<TerminalRegistry> = LazyLock::new(TerminalRegistry::new);

/// Registry that manages all terminal backend implementations.
struct TerminalRegistry {
    backends: HashMap<TerminalType, Box<dyn TerminalBackend>>,
}

impl TerminalRegistry {
    fn new() -> Self {
        let mut backends: HashMap<TerminalType, Box<dyn TerminalBackend>> = HashMap::new();
        backends.insert(TerminalType::Ghostty, Box::new(GhosttyBackend));
        backends.insert(TerminalType::ITerm, Box::new(ITermBackend));
        backends.insert(TerminalType::TerminalApp, Box::new(TerminalAppBackend));
        // Note: Native is NOT registered - it delegates to detected type
        Self { backends }
    }

    /// Get a reference to a terminal backend by type.
    fn get(&self, terminal_type: &TerminalType) -> Option<&dyn TerminalBackend> {
        self.backends.get(terminal_type).map(|b| b.as_ref())
    }
}

/// Get a reference to a terminal backend by type.
///
/// Returns None for `TerminalType::Native` since it should be resolved
/// via `detect_terminal()` first.
pub fn get_backend(terminal_type: &TerminalType) -> Option<&'static dyn TerminalBackend> {
    REGISTRY.get(terminal_type)
}

/// Detect available terminal (Ghostty > iTerm > Terminal.app).
///
/// Checks terminals in preference order and returns the first available one.
/// This function will never return `TerminalType::Native`.
#[cfg(target_os = "macos")]
pub fn detect_terminal() -> Result<TerminalType, TerminalError> {
    debug!(event = "core.terminal.detection_started");

    // Check in preference order: Ghostty > iTerm > Terminal.app
    let terminals = [
        TerminalType::Ghostty,
        TerminalType::ITerm,
        TerminalType::TerminalApp,
    ];

    for terminal_type in terminals {
        if let Some(backend) = get_backend(&terminal_type)
            && backend.is_available()
        {
            debug!(event = "core.terminal.detected", terminal = backend.name());
            return Ok(terminal_type);
        }
    }

    warn!(
        event = "core.terminal.none_found",
        checked = "Ghostty,iTerm,Terminal"
    );
    Err(TerminalError::NoTerminalFound)
}

#[cfg(not(target_os = "macos"))]
pub fn detect_terminal() -> Result<TerminalType, TerminalError> {
    warn!(
        event = "core.terminal.platform_not_supported",
        platform = std::env::consts::OS
    );
    Err(TerminalError::NoTerminalFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_backend_ghostty() {
        let backend = get_backend(&TerminalType::Ghostty);
        assert!(backend.is_some());
        assert_eq!(backend.unwrap().name(), "ghostty");
    }

    #[test]
    fn test_get_backend_iterm() {
        let backend = get_backend(&TerminalType::ITerm);
        assert!(backend.is_some());
        assert_eq!(backend.unwrap().name(), "iterm");
    }

    #[test]
    fn test_get_backend_terminal_app() {
        let backend = get_backend(&TerminalType::TerminalApp);
        assert!(backend.is_some());
        assert_eq!(backend.unwrap().name(), "terminal");
    }

    #[test]
    fn test_get_backend_native_returns_none() {
        // Native is not registered - must use detect_terminal() first
        let backend = get_backend(&TerminalType::Native);
        assert!(backend.is_none());
    }

    #[test]
    fn test_detect_terminal_does_not_panic() {
        // This test depends on the system, but should never panic
        let _result = detect_terminal();
    }

    #[test]
    fn test_registry_contains_expected_terminals() {
        let expected = [
            TerminalType::Ghostty,
            TerminalType::ITerm,
            TerminalType::TerminalApp,
        ];
        for terminal_type in expected {
            let backend = get_backend(&terminal_type);
            assert!(
                backend.is_some(),
                "Registry should contain {:?}",
                terminal_type
            );
        }
    }

    #[test]
    fn test_all_registered_backends_have_correct_names() {
        let checks = [
            (TerminalType::Ghostty, "ghostty"),
            (TerminalType::ITerm, "iterm"),
            (TerminalType::TerminalApp, "terminal"),
        ];
        for (terminal_type, expected_name) in checks {
            let backend = get_backend(&terminal_type).unwrap();
            assert_eq!(
                backend.name(),
                expected_name,
                "Backend for {:?} should have name '{}'",
                terminal_type,
                expected_name
            );
        }
    }
}
