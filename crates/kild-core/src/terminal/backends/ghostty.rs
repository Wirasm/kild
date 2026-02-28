//! Ghostty terminal backend implementation.

#[cfg(target_os = "macos")]
use tracing::{debug, warn};

use crate::terminal::{
    common::detection::app_exists_macos, errors::TerminalError, traits::TerminalBackend,
};

#[cfg(target_os = "macos")]
use crate::terminal::types::SpawnConfig;

#[cfg(target_os = "macos")]
use crate::terminal::common::escape::{build_cd_command, escape_regex, shell_escape};

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

    // The window_id is embedded in the sh -c command line via the ANSI title escape.
    // pgrep -f may match either the Ghostty process itself or the sh child process.
    // For sh matches, traverse to the Ghostty parent PID for window lookup.
    let found_pid = pids.into_iter().find_map(|pid| {
        // First check if the candidate is itself a Ghostty process
        if is_ghostty_process(pid) {
            return Some(pid);
        }

        // Not a Ghostty process - check if its parent is Ghostty
        if let Some(ppid) = get_parent_pid(pid)
            && is_ghostty_process(ppid)
        {
            return Some(ppid);
        }

        None
    });

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

/// Get the parent PID of a process.
#[cfg(target_os = "macos")]
fn get_parent_pid(pid: u32) -> Option<u32> {
    let output = std::process::Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .ok()?;

    let raw = String::from_utf8_lossy(&output.stdout);
    let trimmed = raw.trim();

    match trimmed.parse() {
        Ok(ppid) => Some(ppid),
        Err(e) => {
            // Non-empty output that fails to parse indicates a system issue
            if !trimmed.is_empty() {
                warn!(
                    event = "core.terminal.get_parent_pid_parse_failed",
                    pid = pid,
                    output = %trimmed,
                    error = %e,
                );
            }
            None
        }
    }
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

/// Look up a Ghostty window using Core Graphics API, with PID fallback.
///
/// Strategy:
/// 1. Try to find by app name + title match (via xcap)
/// 2. If title doesn't match (agent changed it), try PID-based lookup
#[cfg(target_os = "macos")]
fn find_ghostty_native_window(
    window_id: &str,
) -> Result<Option<crate::terminal::native::NativeWindow>, TerminalError> {
    use crate::terminal::native;

    debug!(
        event = "core.terminal.ghostty_native_lookup_started",
        window_id = %window_id
    );

    // Primary: find by title (most common case — title matches session ID)
    if let Some(window) = native::find_window("Ghostty", window_id)? {
        debug!(
            event = "core.terminal.ghostty_native_found_by_title",
            window_id = %window_id,
            cg_window_id = window.id
        );
        return Ok(Some(window));
    }

    // Fallback: find by PID (when agent has changed the window title)
    if let Some(pid) = find_ghostty_pid_by_session(window_id) {
        debug!(
            event = "core.terminal.ghostty_native_trying_pid",
            window_id = %window_id,
            pid = pid
        );
        if let Some(window) = native::find_window_by_pid("Ghostty", pid)? {
            debug!(
                event = "core.terminal.ghostty_native_found_by_pid",
                window_id = %window_id,
                pid = pid,
                cg_window_id = window.id
            );
            return Ok(Some(window));
        }
    }

    debug!(
        event = "core.terminal.ghostty_native_not_found",
        window_id = %window_id
    );
    Ok(None)
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

    #[cfg(target_os = "macos")]
    fn close_window_by_id(&self, window_id: &str) {
        debug!(
            event = "core.terminal.close_ghostty_pkill",
            window_title = %window_id
        );

        // Escape regex metacharacters in the window title to avoid matching wrong processes
        let escaped_id = escape_regex(window_id);
        // Kill the shell process that hosts our window. The window_id is embedded in the
        // sh -c command line via the ANSI title escape sequence (printf '\033]2;{id}\007').
        // We match just the window_id (not "Ghostty.*{id}") because the Ghostty app process
        // doesn't contain the window_id, and the sh process doesn't contain "Ghostty".
        // Window IDs are specific enough (kild-{hash}-{branch}_{index}) to avoid false matches.
        let result = std::process::Command::new("pkill")
            .arg("-f")
            .arg(&escaped_id)
            .output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    debug!(
                        event = "core.terminal.close_ghostty_completed",
                        window_title = %window_id
                    );
                } else {
                    // Log at warn level so this appears in production logs
                    // This is expected if the terminal was manually closed by the user
                    warn!(
                        event = "core.terminal.close_ghostty_no_match",
                        window_title = %window_id,
                        message = "No matching Ghostty process found - terminal may have been closed manually"
                    );
                }
            }
            Err(e) => {
                // Log at warn level so this appears in production logs
                warn!(
                    event = "core.terminal.close_ghostty_failed",
                    window_title = %window_id,
                    error = %e,
                    message = "pkill command failed - terminal window may remain open"
                );
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn focus_window(&self, window_id: &str) -> Result<(), TerminalError> {
        use crate::terminal::native;
        use tracing::info;

        debug!(event = "core.terminal.focus_ghostty_started", window_id = %window_id);

        match find_ghostty_native_window(window_id)? {
            Some(window) => {
                native::focus_window(&window)?;
                info!(event = "core.terminal.focus_completed", terminal = "Ghostty", window_id = %window_id);
                Ok(())
            }
            None => Err(TerminalError::FocusFailed {
                message: format!("No Ghostty window found matching '{}'", window_id),
            }),
        }
    }

    #[cfg(target_os = "macos")]
    fn hide_window(&self, window_id: &str) -> Result<(), TerminalError> {
        use crate::terminal::native;
        use tracing::info;

        debug!(event = "core.terminal.hide_ghostty_started", window_id = %window_id);

        match find_ghostty_native_window(window_id)? {
            Some(window) => {
                native::minimize_window(&window)?;
                info!(event = "core.terminal.hide_completed", terminal = "Ghostty", window_id = %window_id);
                Ok(())
            }
            None => Err(TerminalError::HideFailed {
                message: format!("No Ghostty window found matching '{}'", window_id),
            }),
        }
    }

    #[cfg(target_os = "macos")]
    fn is_window_open(&self, window_id: &str) -> Result<Option<bool>, TerminalError> {
        debug!(event = "core.terminal.check_ghostty_started", window_id = %window_id);

        match find_ghostty_native_window(window_id)? {
            Some(window) => {
                // Define "open" as visible (not minimized). Minimized windows are
                // considered "not open" for session status (agent is not actively visible).
                let is_open = !window.is_minimized;
                debug!(
                    event = "core.terminal.check_ghostty_completed",
                    window_id = %window_id,
                    is_open = is_open,
                    is_minimized = window.is_minimized
                );
                Ok(Some(is_open))
            }
            None => {
                debug!(
                    event = "core.terminal.check_ghostty_not_found",
                    window_id = %window_id
                );
                Ok(Some(false))
            }
        }
    }

    crate::terminal::common::helpers::platform_unsupported!(not(target_os = "macos"), "ghostty");

    #[cfg(not(target_os = "macos"))]
    fn is_window_open(&self, _window_id: &str) -> Result<Option<bool>, TerminalError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[cfg(target_os = "macos")]
    #[test]
    fn test_ghostty_pkill_pattern_escaping() {
        // Verify the pattern format used in close_window - matches just the window_id
        // (no "Ghostty" prefix since the sh process doesn't contain "Ghostty")
        let session_id = "my-kild.test";
        let escaped = escape_regex(session_id);
        // The pattern should escape the dot to avoid matching any character
        assert_eq!(escaped, "my-kild\\.test");
        // Pattern should NOT contain "Ghostty" prefix
        assert!(!escaped.contains("Ghostty"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_ghostty_spawn_command_structure() {
        use std::path::PathBuf;

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

    #[cfg(target_os = "macos")]
    #[test]
    fn test_is_ghostty_process_helper() {
        // Just verify the function doesn't panic with invalid PID
        // Can't test actual behavior without a running Ghostty process
        let result = is_ghostty_process(99999999);
        assert!(!result, "Non-existent PID should not be a Ghostty process");
    }

    #[cfg(target_os = "macos")]
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

    #[test]
    fn test_is_window_open_returns_option_type() {
        let backend = GhosttyBackend;
        // The method should return without panic
        let result = backend.is_window_open("nonexistent-window-title");
        // Result type should be Result<Option<bool>, _>
        assert!(result.is_ok());
        // For a non-existent window, should return Some(false) or None
        // (depends on whether Ghostty is installed/running)
        let value = result.unwrap();
        assert!(value.is_none() || value == Some(false));
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore] // Requires Ghostty installed - run manually
    fn test_is_window_open_ghostty_not_running() {
        // When Ghostty app is not running, should return Some(false)
        // This test is ignored because it depends on Ghostty being closed
        let backend = GhosttyBackend;
        let result = backend.is_window_open("any-window");
        // Should succeed and indicate window not found
        if let Ok(Some(found)) = result {
            assert!(
                !found,
                "Should report window not found when app not running"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_parent_pid_for_current_process() {
        let current_pid = std::process::id();
        let parent = get_parent_pid(current_pid);
        assert!(parent.is_some(), "Current process should have a parent PID");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_parent_pid_nonexistent_process() {
        let result = get_parent_pid(99999999);
        // Non-existent PID should return None (ps will fail or return empty)
        assert!(result.is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_close_window_pkill_pattern_no_ghostty_prefix() {
        // Verify the pkill pattern matches the window_id without requiring "Ghostty" prefix
        let window_id = "kild-project123-my-branch_0";
        let escaped = escape_regex(window_id);
        assert_eq!(escaped, "kild-project123-my-branch_0");
        assert!(!escaped.contains("Ghostty"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_find_ghostty_native_window_not_found() {
        // When no Ghostty window matches, should return None
        let result = find_ghostty_native_window("nonexistent-window-xyz-12345");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_native_window_fields() {
        use crate::terminal::native::NativeWindow;
        let info = NativeWindow {
            id: 12345,
            title: "test-window".to_string(),
            app_name: "Ghostty".to_string(),
            pid: Some(9999),
            is_minimized: false,
        };
        assert_eq!(info.id, 12345);
        assert_eq!(info.title, "test-window");
        assert_eq!(info.app_name, "Ghostty");
        assert_eq!(info.pid, Some(9999));
        assert!(!info.is_minimized);
    }
}
