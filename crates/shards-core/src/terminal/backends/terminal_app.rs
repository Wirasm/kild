//! Terminal.app backend implementation.

use tracing::debug;

use crate::terminal::{
    common::{
        applescript::{close_applescript_window, execute_spawn_script},
        detection::app_exists_macos,
        escape::{applescript_escape, build_cd_command},
    },
    errors::TerminalError,
    traits::TerminalBackend,
    types::SpawnConfig,
};

/// AppleScript template for Terminal.app window launching (with window ID capture).
const TERMINAL_SCRIPT: &str = r#"tell application "Terminal"
        set newTab to do script "{command}"
        set newWindow to window of newTab
        return id of newWindow
    end tell"#;

/// AppleScript template for Terminal.app window closing (with window ID support).
/// Errors are handled in Rust, not AppleScript, for proper logging.
const TERMINAL_CLOSE_SCRIPT: &str = r#"tell application "Terminal"
        close window id {window_id}
    end tell"#;

/// AppleScript template for Terminal.app window focusing.
/// - `activate` brings Terminal.app to the foreground (above other apps)
/// - `set frontmost` ensures the specific window is in front of other Terminal.app windows
const TERMINAL_FOCUS_SCRIPT: &str = r#"tell application "Terminal"
        activate
        set frontmost of window id {window_id} to true
    end tell"#;

/// Backend implementation for Terminal.app.
pub struct TerminalAppBackend;

impl TerminalBackend for TerminalAppBackend {
    fn name(&self) -> &'static str {
        "terminal"
    }

    fn display_name(&self) -> &'static str {
        "Terminal.app"
    }

    fn is_available(&self) -> bool {
        app_exists_macos("Terminal")
    }

    #[cfg(target_os = "macos")]
    fn execute_spawn(
        &self,
        config: &SpawnConfig,
        _window_title: Option<&str>,
    ) -> Result<Option<String>, TerminalError> {
        let cd_command = build_cd_command(config.working_directory(), config.command());
        let script = TERMINAL_SCRIPT.replace("{command}", &applescript_escape(&cd_command));

        execute_spawn_script(&script, self.display_name())
    }

    #[cfg(not(target_os = "macos"))]
    fn execute_spawn(
        &self,
        _config: &SpawnConfig,
        _window_title: Option<&str>,
    ) -> Result<Option<String>, TerminalError> {
        debug!(
            event = "core.terminal.spawn_terminal_app_not_supported",
            platform = std::env::consts::OS
        );
        Ok(None)
    }

    #[cfg(target_os = "macos")]
    fn close_window(&self, window_id: Option<&str>) {
        let Some(id) = window_id else {
            debug!(
                event = "core.terminal.close_skipped_no_id",
                terminal = "terminal_app",
                message = "No window ID available, skipping close to avoid closing wrong window"
            );
            return;
        };

        let script = TERMINAL_CLOSE_SCRIPT.replace("{window_id}", id);
        close_applescript_window(&script, self.name(), id);
    }

    #[cfg(not(target_os = "macos"))]
    fn close_window(&self, _window_id: Option<&str>) {
        debug!(
            event = "core.terminal.close_not_supported",
            platform = std::env::consts::OS
        );
    }

    #[cfg(target_os = "macos")]
    fn focus_window(&self, window_id: &str) -> Result<(), TerminalError> {
        let script = TERMINAL_FOCUS_SCRIPT.replace("{window_id}", window_id);
        crate::terminal::common::applescript::focus_applescript_window(
            &script,
            self.display_name(),
            window_id,
        )
    }

    #[cfg(not(target_os = "macos"))]
    fn focus_window(&self, _window_id: &str) -> Result<(), TerminalError> {
        Err(TerminalError::FocusFailed {
            message: "Focus not supported on this platform".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_terminal_app_backend_name() {
        let backend = TerminalAppBackend;
        assert_eq!(backend.name(), "terminal");
    }

    #[test]
    fn test_terminal_app_backend_display_name() {
        let backend = TerminalAppBackend;
        assert_eq!(backend.display_name(), "Terminal.app");
    }

    #[test]
    fn test_terminal_app_close_window_skips_when_no_id() {
        let backend = TerminalAppBackend;
        // close_window returns () - just verify it doesn't panic
        backend.close_window(None);
    }

    #[test]
    fn test_terminal_script_has_window_id_return() {
        assert!(TERMINAL_SCRIPT.contains("return id of newWindow"));
    }

    #[test]
    fn test_terminal_close_script_has_window_id_placeholder() {
        assert!(TERMINAL_CLOSE_SCRIPT.contains("{window_id}"));
    }

    #[test]
    fn test_terminal_script_command_substitution() {
        let cd_command = build_cd_command(&PathBuf::from("/tmp"), "echo hello");
        let script = TERMINAL_SCRIPT.replace("{command}", &applescript_escape(&cd_command));
        assert!(script.contains("/tmp"));
        assert!(script.contains("echo hello"));
        assert!(script.contains("Terminal"));
    }
}
