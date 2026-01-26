//! iTerm2 terminal backend implementation.

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

/// AppleScript template for iTerm window launching (with window ID capture).
const ITERM_SCRIPT: &str = r#"tell application "iTerm"
        set newWindow to (create window with default profile)
        set windowId to id of newWindow
        tell current session of newWindow
            write text "{command}"
        end tell
        return windowId
    end tell"#;

/// AppleScript template for iTerm window closing (with window ID support).
/// Errors are handled in Rust, not AppleScript, for proper logging.
const ITERM_CLOSE_SCRIPT: &str = r#"tell application "iTerm"
        close window id {window_id}
    end tell"#;

/// AppleScript template for iTerm window focusing.
/// - `activate` brings iTerm to the foreground (above other apps)
/// - `set frontmost` ensures the specific window is in front of other iTerm windows
const ITERM_FOCUS_SCRIPT: &str = r#"tell application "iTerm"
        activate
        set frontmost of window id {window_id} to true
    end tell"#;

/// Backend implementation for iTerm2 terminal.
pub struct ITermBackend;

impl TerminalBackend for ITermBackend {
    fn name(&self) -> &'static str {
        "iterm"
    }

    fn display_name(&self) -> &'static str {
        "iTerm2"
    }

    fn is_available(&self) -> bool {
        app_exists_macos("iTerm")
    }

    #[cfg(target_os = "macos")]
    fn execute_spawn(
        &self,
        config: &SpawnConfig,
        _window_title: Option<&str>,
    ) -> Result<Option<String>, TerminalError> {
        let cd_command = build_cd_command(config.working_directory(), config.command());
        let script = ITERM_SCRIPT.replace("{command}", &applescript_escape(&cd_command));

        execute_spawn_script(&script, self.display_name())
    }

    #[cfg(not(target_os = "macos"))]
    fn execute_spawn(
        &self,
        _config: &SpawnConfig,
        _window_title: Option<&str>,
    ) -> Result<Option<String>, TerminalError> {
        debug!(
            event = "core.terminal.spawn_iterm_not_supported",
            platform = std::env::consts::OS
        );
        Ok(None)
    }

    #[cfg(target_os = "macos")]
    fn close_window(&self, window_id: Option<&str>) {
        let Some(id) = window_id else {
            debug!(
                event = "core.terminal.close_skipped_no_id",
                terminal = "iterm",
                message = "No window ID available, skipping close to avoid closing wrong window"
            );
            return;
        };

        let script = ITERM_CLOSE_SCRIPT.replace("{window_id}", id);
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
        let script = ITERM_FOCUS_SCRIPT.replace("{window_id}", window_id);
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
    fn test_iterm_backend_name() {
        let backend = ITermBackend;
        assert_eq!(backend.name(), "iterm");
    }

    #[test]
    fn test_iterm_backend_display_name() {
        let backend = ITermBackend;
        assert_eq!(backend.display_name(), "iTerm2");
    }

    #[test]
    fn test_iterm_close_window_skips_when_no_id() {
        let backend = ITermBackend;
        // close_window returns () - just verify it doesn't panic
        backend.close_window(None);
    }

    #[test]
    fn test_iterm_script_has_window_id_return() {
        assert!(ITERM_SCRIPT.contains("return windowId"));
    }

    #[test]
    fn test_iterm_close_script_has_window_id_placeholder() {
        assert!(ITERM_CLOSE_SCRIPT.contains("{window_id}"));
    }

    #[test]
    fn test_iterm_script_command_substitution() {
        let cd_command = build_cd_command(&PathBuf::from("/tmp"), "echo hello");
        let script = ITERM_SCRIPT.replace("{command}", &applescript_escape(&cd_command));
        assert!(script.contains("/tmp"));
        assert!(script.contains("echo hello"));
        assert!(script.contains("iTerm"));
    }
}
