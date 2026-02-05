use std::path::Path;
use tracing::{debug, info, warn};

use crate::config::KildConfig;
use crate::process::{
    ensure_pid_dir, get_pid_file_path, get_process_info, is_process_running,
    read_pid_file_with_retry, wrap_command_with_pid_capture,
};
use crate::terminal::{errors::TerminalError, operations, types::*};

/// Process info returned from find_agent_process_with_retry
type ProcessSearchResult = Result<(Option<u32>, Option<String>, Option<u64>), TerminalError>;

/// Find agent process with retry logic and exponential backoff
fn find_agent_process_with_retry(
    agent_name: &str,
    command: &str,
    config: &KildConfig,
) -> ProcessSearchResult {
    let max_attempts = config.terminal.max_retry_attempts;
    let mut delay_ms = config.terminal.spawn_delay_ms;

    for attempt in 1..=max_attempts {
        info!(
            event = "core.terminal.searching_for_agent_process",
            attempt, max_attempts, delay_ms, agent_name, command
        );

        std::thread::sleep(std::time::Duration::from_millis(delay_ms));

        match crate::process::find_process_by_name(agent_name, Some(command)) {
            Ok(Some(info)) => {
                let total_delay_ms = config.terminal.spawn_delay_ms * (2_u64.pow(attempt) - 1);
                info!(
                    event = "core.terminal.agent_process_found",
                    attempt,
                    total_delay_ms,
                    pid = info.pid.as_u32(),
                    process_name = info.name,
                    agent_name
                );
                return Ok((
                    Some(info.pid.as_u32()),
                    Some(info.name),
                    Some(info.start_time),
                ));
            }
            Ok(None) => {
                if attempt == max_attempts {
                    warn!(
                        event = "core.terminal.agent_process_not_found_final",
                        agent_name,
                        command,
                        attempts = max_attempts,
                        message = "Agent process not found after all retry attempts - session created but process tracking unavailable"
                    );
                } else {
                    info!(
                        event = "core.terminal.agent_process_not_found_retry",
                        attempt,
                        max_attempts,
                        agent_name,
                        next_delay_ms = delay_ms * 2
                    );
                }
            }
            Err(e) => {
                warn!(
                    event = "core.terminal.agent_process_search_error",
                    attempt,
                    agent_name,
                    error = %e
                );
            }
        }

        // Exponential backoff with cap: 1s, 2s, 4s, 8s, 8s
        delay_ms = std::cmp::min(delay_ms * 2, 8000);
    }

    Ok((None, None, None))
}

/// Spawn a terminal window with the given command
///
/// # Arguments
/// * `working_directory` - The directory to run the command in
/// * `command` - The command to execute
/// * `config` - The kild configuration
/// * `session_id` - Optional session ID for unique Ghostty window titles
/// * `kild_dir` - Optional kild directory for PID file tracking
///
/// Returns a SpawnResult containing the terminal type, process info, and window ID
pub fn spawn_terminal(
    working_directory: &Path,
    command: &str,
    config: &KildConfig,
    session_id: Option<&str>,
    kild_dir: Option<&Path>,
) -> Result<SpawnResult, TerminalError> {
    info!(
        event = "core.terminal.spawn_started",
        working_directory = %working_directory.display(),
        command = command,
        session_id = ?session_id
    );

    let terminal_type = if let Some(preferred) = &config.terminal.preferred {
        // Try to use preferred terminal, fall back to detection if not available
        match preferred.as_str() {
            "iterm2" | "iterm" => TerminalType::ITerm,
            "terminal" => TerminalType::TerminalApp,
            "ghostty" => TerminalType::Ghostty,
            "alacritty" => TerminalType::Alacritty,
            "native" => TerminalType::Native,
            _ => {
                warn!(
                    event = "core.terminal.unknown_preference",
                    preferred = preferred,
                    message = "Unknown terminal preference, falling back to detection"
                );
                operations::detect_terminal()?
            }
        }
    } else {
        operations::detect_terminal()?
    };

    debug!(
        event = "core.terminal.detect_completed",
        terminal_type = %terminal_type,
        working_directory = %working_directory.display()
    );

    // Set up PID file tracking if session_id and kild_dir are provided
    let pid_file_path = match (session_id, kild_dir) {
        (Some(sid), Some(sdir)) => {
            // Ensure PID directory exists
            ensure_pid_dir(sdir).map_err(|e| TerminalError::SpawnFailed {
                message: format!("Failed to create PID directory: {}", e),
            })?;

            let path = get_pid_file_path(sdir, sid);

            // Delete any stale PID file from a previous spawn with the same ID
            // (e.g., restart_session reuses spawn_id 0)
            if path.exists()
                && let Err(e) = std::fs::remove_file(&path)
            {
                warn!(
                    event = "core.terminal.stale_pid_file_cleanup_failed",
                    session_id = sid,
                    pid_file = %path.display(),
                    error = %e,
                    "Failed to delete stale PID file before spawn"
                );
            }

            debug!(
                event = "core.terminal.pid_file_configured",
                session_id = sid,
                pid_file = %path.display()
            );
            Some(path)
        }
        _ => None,
    };

    // Wrap command with PID capture if we have a PID file path
    let actual_command = match &pid_file_path {
        Some(path) => {
            let wrapped = wrap_command_with_pid_capture(command, path);
            debug!(
                event = "core.terminal.command_wrapped",
                original = command,
                wrapped = %wrapped
            );
            wrapped
        }
        None => command.to_string(),
    };

    let spawn_config = SpawnConfig::new(
        terminal_type.clone(),
        working_directory.to_path_buf(),
        actual_command.clone(),
    );

    // Generate unique window title for Ghostty (based on session_id if available)
    let ghostty_window_title = session_id
        .map(|id| format!("kild-{}", id.replace('/', "-")))
        .unwrap_or_else(|| format!("kild-{}", uuid::Uuid::new_v4().simple()));

    // Execute spawn script and capture window ID
    let terminal_window_id =
        operations::execute_spawn_script(&spawn_config, Some(&ghostty_window_title))?;

    debug!(
        event = "core.terminal.spawn_script_executed",
        terminal_type = %terminal_type,
        terminal_window_id = ?terminal_window_id
    );

    // Get process info - prefer PID file, fall back to process search
    let (process_id, process_name, process_start_time) = match &pid_file_path {
        Some(path) => read_pid_from_file_with_validation(path, config)?,
        None => {
            // Fall back to process search (legacy behavior)
            let agent_name = operations::extract_command_name(command);
            find_agent_process_with_retry(&agent_name, command, config)?
        }
    };

    let result = SpawnResult::new(
        terminal_type.clone(),
        command.to_string(), // Store original command, not wrapped
        working_directory.to_path_buf(),
        process_id,
        process_name.clone(),
        process_start_time,
        terminal_window_id.clone(),
    );

    info!(
        event = "core.terminal.spawn_completed",
        terminal_type = %terminal_type,
        working_directory = %working_directory.display(),
        command = command,
        process_id = process_id,
        process_name = ?process_name,
        terminal_window_id = ?terminal_window_id
    );

    Ok(result)
}

/// Read PID from file and validate the process exists
fn read_pid_from_file_with_validation(pid_file: &Path, config: &KildConfig) -> ProcessSearchResult {
    info!(
        event = "core.terminal.reading_pid_file",
        path = %pid_file.display()
    );

    // Read PID with retry (file may not exist immediately after spawn)
    let max_attempts = config.terminal.max_retry_attempts;
    let initial_delay = config.terminal.spawn_delay_ms;

    match read_pid_file_with_retry(pid_file, max_attempts, initial_delay) {
        Ok(Some(pid)) => {
            // Verify the process exists and get its info
            match is_process_running(pid) {
                Ok(true) => {
                    // Get full process info
                    match get_process_info(pid) {
                        Ok(info) => {
                            info!(
                                event = "core.terminal.pid_file_process_found",
                                pid,
                                process_name = %info.name,
                                start_time = info.start_time
                            );
                            Ok((Some(pid), Some(info.name), Some(info.start_time)))
                        }
                        Err(e) => {
                            warn!(
                                event = "core.terminal.pid_file_process_info_failed",
                                pid,
                                error = %e
                            );
                            // Process exists but couldn't get info - still return PID
                            Ok((Some(pid), None, None))
                        }
                    }
                }
                Ok(false) => {
                    warn!(
                        event = "core.terminal.pid_file_process_not_running",
                        pid,
                        message = "PID from file exists but process is not running"
                    );
                    Ok((None, None, None))
                }
                Err(e) => {
                    warn!(
                        event = "core.terminal.pid_file_process_check_failed",
                        pid,
                        error = %e
                    );
                    Ok((None, None, None))
                }
            }
        }
        Ok(None) => {
            warn!(
                event = "core.terminal.pid_file_not_found",
                path = %pid_file.display(),
                message = "PID file not created after spawn - process tracking unavailable. Session will be created but 'restart' and 'destroy' commands may not be able to manage the agent process automatically."
            );
            Ok((None, None, None))
        }
        Err(e) => {
            warn!(
                event = "core.terminal.pid_file_read_error",
                path = %pid_file.display(),
                error = %e,
                message = "Failed to read PID file - process tracking unavailable. Session will be created but 'restart' and 'destroy' commands may not be able to manage the agent process automatically."
            );
            Ok((None, None, None))
        }
    }
}

pub fn detect_available_terminal() -> Result<TerminalType, TerminalError> {
    info!(event = "core.terminal.detect_started");

    let terminal_type = operations::detect_terminal()?;

    info!(
        event = "core.terminal.detect_completed",
        terminal_type = %terminal_type
    );

    Ok(terminal_type)
}

/// Close a terminal window for a session (fire-and-forget).
///
/// This is a best-effort operation used during session destruction.
/// It will not fail if the terminal window is already closed or the terminal
/// application is not running.
///
/// # Arguments
/// * `terminal_type` - The type of terminal (iTerm, Terminal.app, Ghostty, Alacritty)
/// * `window_id` - The window ID (for iTerm/Terminal.app) or title (for Ghostty/Alacritty)
///
/// If window_id is None, the close is skipped to avoid closing the wrong window.
/// Errors are logged but never returned - terminal close should never block session destruction.
pub fn close_terminal(terminal_type: &TerminalType, window_id: Option<&str>) {
    info!(
        event = "core.terminal.close_started",
        terminal_type = %terminal_type,
        window_id = ?window_id
    );

    operations::close_terminal_window(terminal_type, window_id);

    info!(
        event = "core.terminal.close_completed",
        terminal_type = %terminal_type,
        window_id = ?window_id
    );
}

/// Focus a terminal window (bring to foreground).
///
/// # Arguments
/// * `terminal_type` - The type of terminal (iTerm, Terminal.app, Ghostty, Alacritty)
/// * `window_id` - The window ID (for iTerm/Terminal.app) or title (for Ghostty/Alacritty)
///
/// # Returns
/// * `Ok(())` - Window was focused successfully
/// * `Err(TerminalError)` - Focus failed (e.g., window not found, AppleScript execution error)
pub fn focus_terminal(terminal_type: &TerminalType, window_id: &str) -> Result<(), TerminalError> {
    info!(
        event = "core.terminal.focus_requested",
        terminal_type = %terminal_type,
        window_id = %window_id
    );

    operations::focus_terminal_window(terminal_type, window_id)
}

/// Hide/minimize a terminal window.
///
/// # Arguments
/// * `terminal_type` - The type of terminal (iTerm, Terminal.app, Ghostty, Alacritty)
/// * `window_id` - The window ID (for iTerm/Terminal.app) or title (for Ghostty/Alacritty)
///
/// # Returns
/// * `Ok(())` - Window was hidden successfully
/// * `Err(TerminalError)` - Hide failed (e.g., window not found, AppleScript execution error)
pub fn hide_terminal(terminal_type: &TerminalType, window_id: &str) -> Result<(), TerminalError> {
    info!(
        event = "core.terminal.hide_requested",
        terminal_type = %terminal_type,
        window_id = %window_id
    );

    operations::hide_terminal_window(terminal_type, window_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_available_terminal() {
        // This test depends on the system environment
        let _result = detect_available_terminal();
        // We can't assert specific results since it depends on what's installed
    }

    #[test]
    fn test_spawn_terminal_invalid_directory() {
        let config = KildConfig::default();
        let result = spawn_terminal(
            Path::new("/nonexistent/directory"),
            "echo hello",
            &config,
            None,
            None,
        );

        let err = result.unwrap_err();
        assert!(
            matches!(
                err,
                TerminalError::WorkingDirectoryNotFound { .. } | TerminalError::NoTerminalFound
            ),
            "Expected WorkingDirectoryNotFound or NoTerminalFound, got: {err}"
        );
    }

    #[test]
    fn test_spawn_terminal_empty_command() {
        let current_dir = std::env::current_dir().unwrap();
        let config = KildConfig::default();
        let result = spawn_terminal(&current_dir, "", &config, None, None);

        let err = result.unwrap_err();
        assert!(
            matches!(
                err,
                TerminalError::InvalidCommand | TerminalError::NoTerminalFound
            ),
            "Expected InvalidCommand or NoTerminalFound, got: {err}"
        );
    }

    #[test]
    #[ignore] // DANGEROUS: Actually closes terminal windows via AppleScript - run manually only
    fn test_close_terminal_does_not_panic_for_all_terminal_types() {
        // WARNING: This test executes real AppleScript that closes terminal windows!
        // It will close the window with the specified ID (or skip if None).
        // Only run manually when no important terminal windows are open.
        //
        // close_terminal returns () - terminal close failure should not block
        // session destruction.
        let terminal_types = vec![
            TerminalType::ITerm,
            TerminalType::TerminalApp,
            TerminalType::Ghostty,
            TerminalType::Native,
        ];

        for terminal_type in terminal_types {
            // Test with None window_id - should skip close and not panic
            close_terminal(&terminal_type, None);
        }
    }

    #[test]
    #[ignore] // DANGEROUS: Actually closes terminal windows via AppleScript - run manually only
    fn test_close_terminal_native_does_not_panic() {
        // WARNING: This test executes real AppleScript via detect_terminal -> close_terminal_window.
        // Only run manually when no important terminal windows are open.
        close_terminal(&TerminalType::Native, None);
    }

    #[test]
    fn test_close_terminal_with_no_window_id_skips() {
        // When window_id is None, close should be skipped to avoid closing wrong window
        // close_terminal returns () - just verify it doesn't panic
        let terminal_types = vec![
            TerminalType::ITerm,
            TerminalType::TerminalApp,
            TerminalType::Ghostty,
        ];

        for terminal_type in terminal_types {
            close_terminal(&terminal_type, None);
        }
    }

    // Note: Testing actual terminal spawning is complex and system-dependent
    // Integration tests would be more appropriate for full spawn testing
}
