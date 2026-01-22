//! Terminal.app backend implementation.

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

/// AppleScript template for Terminal.app window launching (with window ID capture).
const TERMINAL_SCRIPT: &str = r#"tell application "Terminal"
        set newTab to do script "{command}"
        set newWindow to window of newTab
        return id of newWindow
    end tell"#;

/// AppleScript template for Terminal.app window closing (with window ID support).
const TERMINAL_CLOSE_SCRIPT: &str = r#"tell application "Terminal"
        try
            close window id {window_id}
        on error
            -- Window may already be closed
        end try
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
        let cd_command = build_cd_command(&config.working_directory, &config.command);
        let script = TERMINAL_SCRIPT.replace("{command}", &applescript_escape(&cd_command));

        debug!(
            event = "terminal.spawn_script_executing",
            terminal_type = "terminal_app",
            working_directory = %config.working_directory.display()
        );

        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| TerminalError::AppleScriptExecution {
                message: format!("Failed to execute Terminal.app spawn script: {}", e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TerminalError::SpawnFailed {
                message: format!("Terminal.app AppleScript failed: {}", stderr.trim()),
            });
        }

        let window_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

        debug!(
            event = "terminal.spawn_script_completed",
            terminal_type = "terminal_app",
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
            event = "terminal.spawn_terminal_app_not_supported",
            platform = std::env::consts::OS
        );
        Ok(None)
    }

    #[cfg(target_os = "macos")]
    fn close_window(&self, window_id: Option<&str>) -> Result<(), TerminalError> {
        let Some(id) = window_id else {
            debug!(
                event = "terminal.close_skipped_no_id",
                terminal = "terminal_app",
                message = "No window ID available, skipping close to avoid closing wrong window"
            );
            return Ok(());
        };

        let script = TERMINAL_CLOSE_SCRIPT.replace("{window_id}", id);

        debug!(
            event = "terminal.close_started",
            terminal = "terminal_app",
            window_id = %id
        );

        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| TerminalError::AppleScriptExecution {
                message: format!("Failed to execute Terminal.app close script: {}", e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // All AppleScript failures are non-fatal - terminal close should never block destroy.
            warn!(
                event = "terminal.close_failed_non_fatal",
                terminal = "terminal_app",
                window_id = %id,
                stderr = %stderr.trim(),
                message = "Terminal close failed - continuing with destroy"
            );
            return Ok(());
        }

        debug!(
            event = "terminal.close_completed",
            terminal = "terminal_app",
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
        let result = backend.close_window(None);
        assert!(result.is_ok());
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
