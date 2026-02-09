//! iTerm2 terminal backend implementation.

use crate::terminal::{common::detection::app_exists_macos, traits::TerminalBackend};

#[cfg(target_os = "macos")]
use crate::terminal::{errors::TerminalError, types::SpawnConfig};

#[cfg(target_os = "macos")]
use crate::terminal::common::{
    applescript::{close_applescript_window, execute_spawn_script, hide_applescript_window},
    escape::{applescript_escape, build_cd_command},
};

/// AppleScript template for iTerm window launching (with window ID capture).
///
/// On cold start (iTerm not running), `tell application "iTerm"` launches iTerm which
/// automatically opens a default window. We reuse that window instead of creating a
/// second one. When iTerm is already running, we create a new window as before.
#[cfg(target_os = "macos")]
const ITERM_SCRIPT: &str = r#"set iTermWasRunning to application "iTerm" is running
    tell application "iTerm"
        activate
        if not iTermWasRunning then
            -- iTerm just launched. Wait for the default window to initialize
            -- before reusing it (activate is asynchronous).
            delay 0.5
            set newWindow to current window
        else
            set newWindow to (create window with default profile)
        end if
        set windowId to id of newWindow
        tell current session of newWindow
            write text "{command}"
        end tell
        return windowId
    end tell"#;

/// AppleScript template for iTerm window closing (with window ID support).
/// Errors are handled in Rust, not AppleScript, for proper logging.
#[cfg(target_os = "macos")]
const ITERM_CLOSE_SCRIPT: &str = r#"tell application "iTerm"
        close window id {window_id}
    end tell"#;

/// AppleScript template for iTerm window focusing.
/// - `activate` brings iTerm to the foreground (above other apps)
/// - `set miniaturized to false` restores minimized windows
/// - `select` brings the specific window in front of other iTerm windows
#[cfg(target_os = "macos")]
const ITERM_FOCUS_SCRIPT: &str = r#"tell application "iTerm"
        activate
        set miniaturized of window id {window_id} to false
        select window id {window_id}
    end tell"#;

/// AppleScript template for iTerm window hiding (minimize).
#[cfg(target_os = "macos")]
const ITERM_HIDE_SCRIPT: &str = r#"tell application "iTerm"
        set miniaturized of window id {window_id} to true
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

    #[cfg(target_os = "macos")]
    fn close_window(&self, window_id: Option<&str>) {
        let Some(id) = crate::terminal::common::helpers::require_window_id(window_id, self.name())
        else {
            return;
        };

        let script = ITERM_CLOSE_SCRIPT.replace("{window_id}", id);
        close_applescript_window(&script, self.name(), id);
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

    #[cfg(target_os = "macos")]
    fn hide_window(&self, window_id: &str) -> Result<(), TerminalError> {
        let script = ITERM_HIDE_SCRIPT.replace("{window_id}", window_id);
        hide_applescript_window(&script, self.display_name(), window_id)
    }

    crate::terminal::common::helpers::platform_unsupported!(not(target_os = "macos"), "iterm");
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[cfg(target_os = "macos")]
    #[test]
    fn test_iterm_script_has_window_id_return() {
        assert!(ITERM_SCRIPT.contains("return windowId"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_iterm_close_script_has_window_id_placeholder() {
        assert!(ITERM_CLOSE_SCRIPT.contains("{window_id}"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_iterm_focus_script_uses_valid_applescript() {
        assert!(ITERM_FOCUS_SCRIPT.contains("{window_id}"));
        assert!(ITERM_FOCUS_SCRIPT.contains("set miniaturized"));
        assert!(ITERM_FOCUS_SCRIPT.contains("select window id"));
        assert!(
            !ITERM_FOCUS_SCRIPT.contains("set frontmost"),
            "set frontmost is not a valid iTerm window property"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_iterm_script_command_substitution() {
        use std::path::PathBuf;
        let cd_command = build_cd_command(&PathBuf::from("/tmp"), "echo hello");
        let script = ITERM_SCRIPT.replace("{command}", &applescript_escape(&cd_command));
        assert!(script.contains("/tmp"));
        assert!(script.contains("echo hello"));
        assert!(script.contains("iTerm"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_iterm_script_checks_running_state() {
        assert!(ITERM_SCRIPT.contains(r#"application "iTerm" is running"#));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_iterm_script_reuses_window_on_cold_start() {
        assert!(ITERM_SCRIPT.contains("current window"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_iterm_script_creates_window_when_running() {
        assert!(ITERM_SCRIPT.contains("create window with default profile"));
    }
}
