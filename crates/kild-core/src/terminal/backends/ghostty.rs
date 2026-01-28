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

/// Find the Ghostty process PID that contains the given session identifier in its command line.
/// Returns the first matching Ghostty process PID, or None if no match is found.
#[cfg(target_os = "macos")]
fn find_ghostty_pid_by_session(session_id: &str) -> Option<u32> {
    use tracing::debug;

    // Use pgrep -f to find processes with session_id in their command line
    let pgrep_output = match std::process::Command::new("pgrep")
        .arg("-f")
        .arg(session_id)
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            debug!(
                event = "core.terminal.ghostty_pgrep_failed",
                session_id = %session_id,
                error = %e,
                message = "Failed to execute pgrep - falling back to title search"
            );
            return None;
        }
    };

    if !pgrep_output.status.success() {
        debug!(
            event = "core.terminal.ghostty_pgrep_no_match",
            session_id = %session_id
        );
        return None;
    }

    let pids: Vec<u32> = String::from_utf8_lossy(&pgrep_output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect();

    debug!(
        event = "core.terminal.ghostty_pgrep_candidates",
        session_id = %session_id,
        candidate_count = pids.len()
    );

    // Find the first Ghostty process among candidates by checking each process's
    // executable name (via ps -o comm=) for "ghostty"
    let found_pid = pids.into_iter().find(|&pid| is_ghostty_process(pid));

    if let Some(pid) = found_pid {
        debug!(
            event = "core.terminal.ghostty_pid_found",
            session_id = %session_id,
            pid = pid
        );
    } else {
        debug!(
            event = "core.terminal.ghostty_pid_not_found",
            session_id = %session_id
        );
    }

    found_pid
}

/// Check if a process is a Ghostty process by examining its executable name.
#[cfg(target_os = "macos")]
fn is_ghostty_process(pid: u32) -> bool {
    match std::process::Command::new("ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
    {
        Ok(output) => {
            let comm = String::from_utf8_lossy(&output.stdout);
            comm.to_lowercase().contains("ghostty")
        }
        Err(e) => {
            debug!(
                event = "core.terminal.is_ghostty_process_failed",
                pid = pid,
                error = %e,
                message = "Failed to check if PID is Ghostty process"
            );
            false
        }
    }
}

/// Focus a Ghostty window by finding its process via PID and using System Events.
#[cfg(target_os = "macos")]
fn focus_by_pid(pid: u32) -> Result<(), TerminalError> {
    use tracing::{debug, info};

    debug!(
        event = "core.terminal.focus_ghostty_by_pid_started",
        pid = pid
    );

    // Use System Events with unix id to target the specific process
    let focus_script = format!(
        r#"tell application "System Events"
            set targetProc to first process whose unix id is {}
            set frontmost of targetProc to true
            tell targetProc
                if (count of windows) > 0 then
                    perform action "AXRaise" of window 1
                    return "focused"
                else
                    return "no windows"
                end if
            end tell
        end tell"#,
        pid
    );

    match std::process::Command::new("osascript")
        .arg("-e")
        .arg(&focus_script)
        .output()
    {
        Ok(output) if output.status.success() => {
            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if result == "focused" {
                info!(
                    event = "core.terminal.focus_completed",
                    terminal = "Ghostty",
                    method = "pid",
                    pid = pid
                );
                Ok(())
            } else {
                debug!(
                    event = "core.terminal.focus_ghostty_by_pid_no_windows",
                    pid = pid,
                    result = %result
                );
                Err(TerminalError::FocusFailed {
                    message: format!("Ghostty process {} has no windows", pid),
                })
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            debug!(
                event = "core.terminal.focus_ghostty_by_pid_failed",
                pid = pid,
                stderr = %stderr
            );
            Err(TerminalError::FocusFailed {
                message: format!("Failed to focus Ghostty by PID {}: {}", pid, stderr),
            })
        }
        Err(e) => {
            debug!(
                event = "core.terminal.focus_ghostty_by_pid_error",
                pid = pid,
                error = %e
            );
            Err(TerminalError::FocusFailed {
                message: format!("osascript error for PID {}: {}", pid, e),
            })
        }
    }
}

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
        let title = window_title.unwrap_or("kild-session");

        // Shell-escape the title to prevent injection if it contains special characters
        let escaped_title = shell_escape(title);
        // Set window title via ANSI escape sequence (OSC 2) for process identification.
        // Format: \033]2;title\007 - ESC ] 2 ; title BEL
        // The title string is embedded in the command line, enabling process lookup
        // via pgrep -f (for focus) and pkill -f (for close).
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
        use tracing::{debug, error, info, warn};

        debug!(
            event = "core.terminal.focus_ghostty_started",
            window_id = %window_id
        );

        // Step 1: Activate Ghostty app to bring it to the foreground
        let activate_script = r#"tell application "Ghostty" to activate"#;
        let activation_result = std::process::Command::new("osascript")
            .arg("-e")
            .arg(activate_script)
            .output();

        match activation_result {
            Ok(output) if output.status.success() => {
                debug!(
                    event = "core.terminal.focus_ghostty_activated",
                    window_id = %window_id
                );
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                warn!(
                    event = "core.terminal.focus_ghostty_activate_failed",
                    window_id = %window_id,
                    stderr = %stderr,
                    message = "Ghostty activation failed - continuing with focus attempt"
                );
            }
            Err(e) => {
                warn!(
                    event = "core.terminal.focus_ghostty_activate_failed",
                    window_id = %window_id,
                    error = %e,
                    message = "Failed to execute osascript for activation - continuing with focus attempt"
                );
            }
        }

        // Step 2: Try PID-based focus first (handles dynamic title changes)
        // The session ID is embedded in the process command line and persists
        // even when the window title is overwritten by running commands.
        if let Some(pid) = find_ghostty_pid_by_session(window_id) {
            debug!(
                event = "core.terminal.focus_ghostty_trying_pid",
                window_id = %window_id,
                pid = pid
            );
            match focus_by_pid(pid) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    debug!(
                        event = "core.terminal.focus_ghostty_pid_failed_fallback",
                        window_id = %window_id,
                        pid = pid,
                        error = %e,
                        message = "PID-based focus failed, falling back to title search"
                    );
                }
            }
        } else {
            debug!(
                event = "core.terminal.focus_ghostty_no_pid_fallback",
                window_id = %window_id,
                message = "No matching Ghostty process found, falling back to title search"
            );
        }

        // Step 3: Fallback to title-based search (for edge cases)
        // This handles scenarios where PID lookup fails: pgrep unavailable, permission
        // issues, or race conditions where the process exits between lookup and focus.
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
                        method = "title",
                        window_id = %window_id
                    );
                    Ok(())
                } else {
                    warn!(
                        event = "core.terminal.focus_failed",
                        terminal = "Ghostty",
                        window_id = %window_id,
                        message = "Window not found by PID or title"
                    );
                    Err(TerminalError::FocusFailed {
                        message: format!(
                            "Ghostty window '{}' not found (terminal may have been closed)",
                            window_id
                        ),
                    })
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                warn!(
                    event = "core.terminal.focus_failed",
                    terminal = "Ghostty",
                    window_id = %window_id,
                    stderr = %stderr
                );
                Err(TerminalError::FocusFailed { message: stderr })
            }
            Err(e) => {
                error!(
                    event = "core.terminal.focus_failed",
                    terminal = "Ghostty",
                    window_id = %window_id,
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
        let session_id = "my-kild.test";
        let escaped = escape_regex(session_id);
        let pattern = format!("Ghostty.*{}", escaped);
        // The pattern should escape the dot to avoid matching any character
        assert_eq!(pattern, "Ghostty.*my-kild\\.test");
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
        let title = "kild-test-session";
        let escaped_title = shell_escape(title);
        let cd_command = build_cd_command(config.working_directory(), config.command());
        let ghostty_command = format!(
            "printf '\\033]2;'{}'\\007' && {}",
            escaped_title, cd_command
        );

        assert!(ghostty_command.contains("kild-test-session"));
        assert!(ghostty_command.contains("claude"));
        assert!(ghostty_command.contains("/tmp/test"));
    }

    #[test]
    fn test_is_ghostty_process_helper() {
        // Just verify the function doesn't panic with invalid PID
        // Can't test actual behavior without a running Ghostty process
        let result = is_ghostty_process(99999999);
        assert!(!result, "Non-existent PID should not be a Ghostty process");
    }

    #[test]
    fn test_find_ghostty_pid_no_match() {
        // Search for a session ID that definitely doesn't exist
        let result = find_ghostty_pid_by_session("nonexistent-session-12345-xyz");
        assert!(
            result.is_none(),
            "Should return None for non-existent session"
        );
    }

    #[test]
    fn test_pid_parsing_handles_malformed_output() {
        // Test that the parsing logic used in find_ghostty_pid_by_session
        // correctly handles malformed pgrep output without panicking
        let input = "12345\n\nnot_a_number\n67890\n  \n";
        let pids: Vec<u32> = input
            .lines()
            .filter_map(|line| line.trim().parse().ok())
            .collect();
        assert_eq!(pids, vec![12345, 67890]);
    }

    #[test]
    fn test_ghostty_comm_matching_is_case_insensitive() {
        // Test that the string matching logic used in is_ghostty_process
        // is case-insensitive and handles various executable name formats
        let check_comm = |comm: &str| comm.to_lowercase().contains("ghostty");

        assert!(check_comm("Ghostty"));
        assert!(check_comm("ghostty"));
        assert!(check_comm("GHOSTTY"));
        assert!(check_comm(
            "/Applications/Ghostty.app/Contents/MacOS/ghostty"
        ));
        assert!(!check_comm("iterm"));
        assert!(!check_comm("Terminal"));
    }
}
