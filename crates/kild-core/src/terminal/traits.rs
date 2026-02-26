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

    /// Close a terminal window (fire-and-forget).
    ///
    /// # Arguments
    /// * `window_id` - The window ID (for iTerm/Terminal.app) or title (for Ghostty)
    ///
    /// # Behavior
    /// - If window_id is None, skips close (logs debug message)
    /// - If window_id is Some, attempts to close that specific window
    /// - Close failures are non-fatal and logged at warn level
    /// - Returns () because close operations should never block session destruction
    fn close_window(&self, window_id: Option<&str>);

    /// Focus a terminal window (bring to foreground).
    ///
    /// # Arguments
    /// * `window_id` - The window ID (for iTerm/Terminal.app) or title (for Ghostty)
    ///
    /// # Returns
    /// * `Ok(())` - Window was focused successfully
    /// * `Err(TerminalError)` - Focus failed (window not found, permission denied, etc.)
    fn focus_window(&self, window_id: &str) -> Result<(), TerminalError>;

    /// Hide/minimize a terminal window.
    ///
    /// # Arguments
    /// * `window_id` - The window ID (for iTerm/Terminal.app) or title (for Ghostty/Alacritty)
    ///
    /// # Returns
    /// * `Ok(())` - Window was hidden successfully
    /// * `Err(TerminalError)` - Hide failed (window not found, permission denied, etc.)
    fn hide_window(&self, window_id: &str) -> Result<(), TerminalError>;

    /// Check if a terminal window is open (by window ID or title).
    ///
    /// # Returns
    /// - `Ok(Some(true))` - Window is definitely open
    /// - `Ok(Some(false))` - Window is definitely closed
    /// - `Ok(None)` - Cannot determine (use PID-based detection instead)
    /// - `Err(...)` - Error occurred during check
    ///
    /// This is used as a fallback for terminals like Ghostty where PID tracking
    /// may not be available.
    fn is_window_open(&self, window_id: &str) -> Result<Option<bool>, TerminalError> {
        // Default: cannot determine, fall back to PID-based detection
        let _ = window_id;
        Ok(None)
    }
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

        fn close_window(&self, _window_id: Option<&str>) {}

        fn focus_window(&self, _window_id: &str) -> Result<(), TerminalError> {
            Ok(())
        }

        fn hide_window(&self, _window_id: &str) -> Result<(), TerminalError> {
            Ok(())
        }
    }

    #[test]
    fn mock_terminal_backend_name_and_availability_are_accessible() {
        let backend = MockBackend;
        assert_eq!(backend.name(), "mock");
        assert_eq!(backend.display_name(), "Mock Terminal");
        assert!(backend.is_available());
    }

    #[test]
    fn mock_terminal_backend_execute_spawn_returns_window_title() {
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
    fn mock_terminal_backend_close_window_does_not_panic() {
        let backend = MockBackend;
        // Verify fire-and-forget close does not panic
        backend.close_window(Some("123"));
    }
}
