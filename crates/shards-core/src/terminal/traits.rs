//! Terminal backend trait definition.

use crate::terminal::{errors::TerminalError, types::SpawnConfig};

/// Trait defining the interface for terminal backends.
///
/// Each supported terminal (Ghostty, iTerm, Terminal.app) implements this trait
/// to provide terminal-specific behavior like spawning windows and closing them.
pub trait TerminalBackend: Send + Sync {
    /// The canonical name of this terminal (e.g., "ghostty", "iterm").
    fn name(&self) -> &'static str;

    /// The display name for this terminal (e.g., "Ghostty", "iTerm2").
    fn display_name(&self) -> &'static str;

    /// Check if this terminal is available on the system.
    fn is_available(&self) -> bool;

    /// Execute spawn and return window ID.
    ///
    /// # Arguments
    /// * `config` - The spawn configuration
    /// * `window_title` - Optional unique title for window identification
    ///
    /// # Returns
    /// * `Ok(Some(window_id))` - Window ID captured successfully
    /// * `Ok(None)` - Spawn succeeded but no window ID captured
    /// * `Err(TerminalError)` - Spawn execution failed
    fn execute_spawn(
        &self,
        config: &SpawnConfig,
        window_title: Option<&str>,
    ) -> Result<Option<String>, TerminalError>;

    /// Close a terminal window.
    ///
    /// # Arguments
    /// * `window_id` - The window ID (for iTerm/Terminal.app) or title (for Ghostty)
    ///
    /// # Behavior
    /// - If window_id is None, skips close (logs debug message)
    /// - If window_id is Some, attempts to close that specific window
    /// - Close failures are non-fatal and logged as debug/warn
    fn close_window(&self, window_id: Option<&str>) -> Result<(), TerminalError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct MockBackend;

    impl TerminalBackend for MockBackend {
        fn name(&self) -> &'static str {
            "mock"
        }

        fn display_name(&self) -> &'static str {
            "Mock Terminal"
        }

        fn is_available(&self) -> bool {
            true
        }

        fn execute_spawn(
            &self,
            _config: &SpawnConfig,
            window_title: Option<&str>,
        ) -> Result<Option<String>, TerminalError> {
            Ok(window_title.map(|s| s.to_string()))
        }

        fn close_window(&self, _window_id: Option<&str>) -> Result<(), TerminalError> {
            Ok(())
        }
    }

    #[test]
    fn test_terminal_backend_basic_methods() {
        let backend = MockBackend;
        assert_eq!(backend.name(), "mock");
        assert_eq!(backend.display_name(), "Mock Terminal");
        assert!(backend.is_available());
    }

    #[test]
    fn test_terminal_backend_execute_spawn() {
        let backend = MockBackend;
        let config = SpawnConfig::new(
            crate::terminal::types::TerminalType::Native,
            PathBuf::from("/tmp"),
            "echo test".to_string(),
        );
        let result = backend.execute_spawn(&config, Some("test-window"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("test-window".to_string()));
    }

    #[test]
    fn test_terminal_backend_close_window() {
        let backend = MockBackend;
        let result = backend.close_window(Some("123"));
        assert!(result.is_ok());
    }
}
