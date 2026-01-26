//! Ghostty terminal backend implementation.

use tracing::{debug, warn};

use crate::terminal::{
    common::{
        detection::app_exists_macos,
        escape::{build_cd_command, escape_regex, shell_escape},
    },
    errors::TerminalError,
    traits::TerminalBackend,
    types::SpawnConfig,
};

/// Backend implementation for Ghostty terminal.
pub struct GhosttyBackend;

impl TerminalBackend for GhosttyBackend {
    fn name(&self) -> &'static str {
        "ghostty"
    }

    fn display_name(&self) -> &'static str {
        "Ghostty"
    }

    fn is_available(&self) -> bool {
        app_exists_macos("Ghostty")
    }

    #[cfg(target_os = "macos")]
    fn execute_spawn(
        &self,
        config: &SpawnConfig,
        window_title: Option<&str>,
    ) -> Result<Option<String>, TerminalError> {
        let cd_command = build_cd_command(config.working_directory(), config.command());
        let title = window_title.unwrap_or("shards-session");

        // Shell-escape the title to prevent injection if it contains special characters
        let escaped_title = shell_escape(title);
        // Set window title via ANSI escape sequence (OSC 2) for later process identification.
        // Format: \033]2;title\007 - ESC ] 2 ; title BEL
        // This title is embedded in the command line, allowing pkill -f to match it.
        let ghostty_command = format!(
            "printf '\\033]2;'{}'\\007' && {}",
            escaped_title, cd_command
        );

        debug!(
            event = "core.terminal.spawn_ghostty_starting",
            terminal_type = %config.terminal_type(),
            working_directory = %config.working_directory().display(),
            window_title = %title
        );

        // On macOS, the ghostty CLI spawns headless processes, not GUI windows.
        // Must use 'open -na Ghostty.app --args' where:
        //   -n opens a new instance, -a specifies the application
        // Arguments after --args are passed to Ghostty's -e flag for command execution.
        let status = std::process::Command::new("open")
            .arg("-na")
            .arg("Ghostty.app")
            .arg("--args")
            .arg("-e")
            .arg("sh")
            .arg("-c")
            .arg(&ghostty_command)
            .status()
            .map_err(|e| TerminalError::SpawnFailed {
                message: format!(
                    "Failed to spawn Ghostty (title='{}', cwd='{}', cmd='{}'): {}",
                    title,
                    config.working_directory().display(),
                    config.command(),
                    e
                ),
            })?;

        if !status.success() {
            return Err(TerminalError::SpawnFailed {
                message: format!(
                    "Ghostty launch failed with exit code: {:?} (title='{}', cwd='{}', cmd='{}')",
                    status.code(),
                    title,
                    config.working_directory().display(),
                    config.command()
                ),
            });
        }

        debug!(
            event = "core.terminal.spawn_ghostty_launched",
            terminal_type = %config.terminal_type(),
            window_title = %title,
            message = "open command completed successfully, Ghostty window should be visible"
        );

        // Return window_title as identifier for close_window
        Ok(Some(title.to_string()))
    }

    #[cfg(not(target_os = "macos"))]
    fn execute_spawn(
        &self,
        _config: &SpawnConfig,
        _window_title: Option<&str>,
    ) -> Result<Option<String>, TerminalError> {
        debug!(
            event = "core.terminal.spawn_ghostty_not_supported",
            platform = std::env::consts::OS
        );
        Ok(None)
    }

    #[cfg(target_os = "macos")]
    fn close_window(&self, window_id: Option<&str>) {
        let Some(id) = window_id else {
            debug!(
                event = "core.terminal.close_skipped_no_id",
                terminal = "ghostty",
                message = "No window ID available, skipping close to avoid closing wrong window"
            );
            return;
        };

        debug!(
            event = "core.terminal.close_ghostty_pkill",
            window_title = %id
        );

        // Escape regex metacharacters in the window title to avoid matching wrong processes
        let escaped_id = escape_regex(id);
        // Use pkill to kill Ghostty processes that contain our session identifier
        let result = std::process::Command::new("pkill")
            .arg("-f")
            .arg(format!("Ghostty.*{}", escaped_id))
            .output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    debug!(
                        event = "core.terminal.close_ghostty_completed",
                        window_title = %id
                    );
                } else {
                    // Log at warn level so this appears in production logs
                    // This is expected if the terminal was manually closed by the user
                    warn!(
                        event = "core.terminal.close_ghostty_no_match",
                        window_title = %id,
                        message = "No matching Ghostty process found - terminal may have been closed manually"
                    );
                }
            }
            Err(e) => {
                // Log at warn level so this appears in production logs
                warn!(
                    event = "core.terminal.close_ghostty_failed",
                    window_title = %id,
                    error = %e,
                    message = "pkill command failed - terminal window may remain open"
                );
            }
        }
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
        use tracing::{error, info};

        debug!(
            event = "core.terminal.focus_ghostty_started",
            window_title = %window_id
        );

        // Ghostty uses window title for identification, not a numeric window ID like iTerm/Terminal.app.
        // Unlike AppleScript-scriptable apps, Ghostty requires System Events to manipulate windows.
        // Step 1: Activate the app to bring it to the foreground
        // Step 2: Use System Events to find our specific window by title and raise it
        let activate_script = r#"tell application "Ghostty" to activate"#;
        match std::process::Command::new("osascript")
            .arg("-e")
            .arg(activate_script)
            .output()
        {
            Ok(output) if output.status.success() => {
                debug!(
                    event = "core.terminal.focus_ghostty_activated",
                    window_title = %window_id
                );
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!(
                    event = "core.terminal.focus_ghostty_activate_failed",
                    window_title = %window_id,
                    stderr = %stderr.trim(),
                    message = "Ghostty activation failed - focus may not work correctly"
                );
            }
            Err(e) => {
                warn!(
                    event = "core.terminal.focus_ghostty_activate_failed",
                    window_title = %window_id,
                    error = %e,
                    message = "Failed to execute osascript for Ghostty activation"
                );
            }
        }

        // Use System Events to find and raise the window by title.
        // AXRaise is an accessibility action that brings a window to the front of other windows.
        let focus_script = format!(
            r#"tell application "System Events"
            tell process "Ghostty"
                set frontmost to true
                repeat with w in windows
                    if name of w contains "{}" then
                        perform action "AXRaise" of w
                        return "focused"
                    end if
                end repeat
                return "not found"
            end tell
        end tell"#,
            window_id
        );

        match std::process::Command::new("osascript")
            .arg("-e")
            .arg(&focus_script)
            .output()
        {
            Ok(output) if output.status.success() => {
                let result = String::from_utf8_lossy(&output.stdout);
                if result.trim() == "focused" {
                    info!(
                        event = "core.terminal.focus_completed",
                        terminal = "Ghostty",
                        window_title = %window_id
                    );
                    Ok(())
                } else {
                    warn!(
                        event = "core.terminal.focus_failed",
                        terminal = "Ghostty",
                        window_title = %window_id,
                        message = "Window not found"
                    );
                    Err(TerminalError::FocusFailed {
                        message: format!("Ghostty window '{}' not found", window_id),
                    })
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!(
                    event = "core.terminal.focus_failed",
                    terminal = "Ghostty",
                    window_title = %window_id,
                    stderr = %stderr.trim()
                );
                Err(TerminalError::FocusFailed {
                    message: stderr.trim().to_string(),
                })
            }
            Err(e) => {
                error!(
                    event = "core.terminal.focus_failed",
                    terminal = "Ghostty",
                    window_title = %window_id,
                    error = %e
                );
                Err(TerminalError::FocusFailed {
                    message: e.to_string(),
                })
            }
        }
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
    fn test_ghostty_backend_name() {
        let backend = GhosttyBackend;
        assert_eq!(backend.name(), "ghostty");
    }

    #[test]
    fn test_ghostty_backend_display_name() {
        let backend = GhosttyBackend;
        assert_eq!(backend.display_name(), "Ghostty");
    }

    #[test]
    fn test_ghostty_close_window_skips_when_no_id() {
        let backend = GhosttyBackend;
        // close_window returns () - just verify it doesn't panic
        backend.close_window(None);
    }

    #[test]
    fn test_ghostty_pkill_pattern_escaping() {
        // Verify the pattern format used in close_window
        let session_id = "my-shard.test";
        let escaped = escape_regex(session_id);
        let pattern = format!("Ghostty.*{}", escaped);
        // The pattern should escape the dot to avoid matching any character
        assert_eq!(pattern, "Ghostty.*my-shard\\.test");
    }

    #[test]
    fn test_ghostty_spawn_command_structure() {
        // Verify the structure of what would be passed to 'open'
        let config = SpawnConfig::new(
            crate::terminal::types::TerminalType::Ghostty,
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
        );

        // The title escaping should work correctly
        let title = "shards-test-session";
        let escaped_title = shell_escape(title);
        let cd_command = build_cd_command(config.working_directory(), config.command());
        let ghostty_command = format!(
            "printf '\\033]2;'{}'\\007' && {}",
            escaped_title, cd_command
        );

        assert!(ghostty_command.contains("shards-test-session"));
        assert!(ghostty_command.contains("claude"));
        assert!(ghostty_command.contains("/tmp/test"));
    }
}
