//! Terminal.app backend implementation.

use crate::terminal::{common::detection::app_exists_macos, traits::TerminalBackend};

#[cfg(target_os = "macos")]
use crate::terminal::{errors::TerminalError, types::SpawnConfig};

#[cfg(target_os = "macos")]
use crate::terminal::common::applescript::{
    close_via_applescript, focus_via_applescript, hide_via_applescript, spawn_via_applescript,
};

/// AppleScript template for Terminal.app window launching (with window ID capture).
#[cfg(target_os = "macos")]
const TERMINAL_SCRIPT: &str = r#"tell application "Terminal"
        set newTab to do script "{command}"
        set newWindow to window of newTab
        return id of newWindow
    end tell"#;

/// AppleScript template for Terminal.app window closing (with window ID support).
/// Errors are handled in Rust, not AppleScript, for proper logging.
#[cfg(target_os = "macos")]
const TERMINAL_CLOSE_SCRIPT: &str = r#"tell application "Terminal"
        close window id {window_id}
    end tell"#;

/// AppleScript template for Terminal.app window focusing.
/// - `activate` brings Terminal.app to the foreground (above other apps)
/// - `set frontmost` ensures the specific window is in front of other Terminal.app windows
#[cfg(target_os = "macos")]
const TERMINAL_FOCUS_SCRIPT: &str = r#"tell application "Terminal"
        activate
        set frontmost of window id {window_id} to true
    end tell"#;

/// AppleScript template for Terminal.app window hiding (minimize).
#[cfg(target_os = "macos")]
const TERMINAL_HIDE_SCRIPT: &str = r#"tell application "Terminal"
        set miniaturized of window id {window_id} to true
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
        spawn_via_applescript(TERMINAL_SCRIPT, self.display_name(), config)
    }

    #[cfg(target_os = "macos")]
    fn close_window_by_id(&self, window_id: &str) {
        close_via_applescript(TERMINAL_CLOSE_SCRIPT, self.name(), window_id);
    }

    #[cfg(target_os = "macos")]
    fn focus_window(&self, window_id: &str) -> Result<(), TerminalError> {
        focus_via_applescript(TERMINAL_FOCUS_SCRIPT, self.display_name(), window_id)
    }

    #[cfg(target_os = "macos")]
    fn hide_window(&self, window_id: &str) -> Result<(), TerminalError> {
        hide_via_applescript(TERMINAL_HIDE_SCRIPT, self.display_name(), window_id)
    }

    crate::terminal::common::helpers::platform_unsupported!(
        not(target_os = "macos"),
        "terminal_app"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[cfg(target_os = "macos")]
    #[test]
    fn test_terminal_script_has_window_id_return() {
        assert!(TERMINAL_SCRIPT.contains("return id of newWindow"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_terminal_close_script_has_window_id_placeholder() {
        assert!(TERMINAL_CLOSE_SCRIPT.contains("{window_id}"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_terminal_script_command_substitution() {
        use crate::escape::applescript_escape;
        use crate::terminal::common::escape::build_cd_command;
        use std::path::PathBuf;
        let cd_command = build_cd_command(&PathBuf::from("/tmp"), "echo hello");
        let script = TERMINAL_SCRIPT.replace("{command}", &applescript_escape(&cd_command));
        assert!(script.contains("/tmp"));
        assert!(script.contains("echo hello"));
        assert!(script.contains("Terminal"));
    }
}
