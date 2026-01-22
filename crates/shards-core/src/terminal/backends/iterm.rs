//! iTerm2 terminal backend implementation.

use tracing::{debug, warn};

use crate::terminal::{
    common::{
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
const ITERM_CLOSE_SCRIPT: &str = r#"tell application "iTerm"
        try
            close window id {window_id}
        on error
            -- Window may already be closed
        end try
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
        let cd_command = build_cd_command(&config.working_directory, &config.command);
        let script = ITERM_SCRIPT.replace("{command}", &applescript_escape(&cd_command));

        debug!(
            event = "terminal.spawn_script_executing",
            terminal_type = "iterm",
            working_directory = %config.working_directory.display()
        );

        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| TerminalError::AppleScriptExecution {
                message: format!("Failed to execute iTerm spawn script: {}", e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TerminalError::SpawnFailed {
                message: format!("iTerm AppleScript failed: {}", stderr.trim()),
            });
        }

        let window_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

        debug!(
            event = "terminal.spawn_script_completed",
            terminal_type = "iterm",
            window_id = %window_id
        );

        if window_id.is_empty() {
            Ok(None)
        } else {
            Ok(Some(window_id))
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn execute_spawn(
        &self,
        _config: &SpawnConfig,
        _window_title: Option<&str>,
    ) -> Result<Option<String>, TerminalError> {
        debug!(
            event = "terminal.spawn_iterm_not_supported",
            platform = std::env::consts::OS
        );
        Ok(None)
    }

    #[cfg(target_os = "macos")]
    fn close_window(&self, window_id: Option<&str>) -> Result<(), TerminalError> {
        let Some(id) = window_id else {
            debug!(
                event = "terminal.close_skipped_no_id",
                terminal = "iterm",
                message = "No window ID available, skipping close to avoid closing wrong window"
            );
            return Ok(());
        };

        let script = ITERM_CLOSE_SCRIPT.replace("{window_id}", id);

        debug!(
            event = "terminal.close_started",
            terminal = "iterm",
            window_id = %id
        );

        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| TerminalError::AppleScriptExecution {
                message: format!("Failed to execute iTerm close script: {}", e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // All AppleScript failures are non-fatal - terminal close should never block destroy.
            warn!(
                event = "terminal.close_failed_non_fatal",
                terminal = "iterm",
                window_id = %id,
                stderr = %stderr.trim(),
                message = "Terminal close failed - continuing with destroy"
            );
            return Ok(());
        }

        debug!(
            event = "terminal.close_completed",
            terminal = "iterm",
            window_id = %id
        );

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    fn close_window(&self, _window_id: Option<&str>) -> Result<(), TerminalError> {
        debug!(
            event = "terminal.close_not_supported",
            platform = std::env::consts::OS
        );
        Ok(())
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
        let result = backend.close_window(None);
        assert!(result.is_ok());
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
