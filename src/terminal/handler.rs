use std::path::Path;
use tracing::{debug, info, warn};

use crate::core::config::ShardsConfig;
use crate::terminal::{errors::TerminalError, operations, types::*};

/// Process info returned from find_agent_process_with_retry
type ProcessSearchResult = Result<(Option<u32>, Option<String>, Option<u64>), TerminalError>;

/// Find agent process with retry logic and exponential backoff
fn find_agent_process_with_retry(
    agent_name: &str,
    command: &str,
    config: &ShardsConfig,
) -> ProcessSearchResult {
    let max_attempts = config.terminal.max_retry_attempts;
    let mut delay_ms = config.terminal.spawn_delay_ms;
    
    for attempt in 1..=max_attempts {
        info!(
            event = "terminal.searching_for_agent_process",
            attempt,
            max_attempts,
            delay_ms,
            agent_name,
            command
        );
        
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        
        match crate::process::find_process_by_name(agent_name, Some(command)) {
            Ok(Some(info)) => {
                let total_delay_ms = config.terminal.spawn_delay_ms * (2_u64.pow(attempt) - 1);
                info!(
                    event = "terminal.agent_process_found",
                    attempt,
                    total_delay_ms,
                    pid = info.pid.as_u32(),
                    process_name = info.name,
                    agent_name
                );
                return Ok((Some(info.pid.as_u32()), Some(info.name), Some(info.start_time)));
            }
            Ok(None) => {
                if attempt == max_attempts {
                    warn!(
                        event = "terminal.agent_process_not_found_final",
                        agent_name,
                        command,
                        attempts = max_attempts,
                        message = "Agent process not found after all retry attempts - session created but process tracking unavailable"
                    );
                } else {
                    info!(
                        event = "terminal.agent_process_not_found_retry",
                        attempt,
                        max_attempts,
                        agent_name,
                        next_delay_ms = delay_ms * 2
                    );
                }
            }
            Err(e) => {
                warn!(
                    event = "terminal.agent_process_search_error",
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
/// * `config` - The shards configuration
/// * `session_id` - Optional session ID for unique Ghostty window titles
///
/// Returns a SpawnResult containing the terminal type, process info, and window ID
pub fn spawn_terminal(
    working_directory: &Path,
    command: &str,
    config: &ShardsConfig,
    session_id: Option<&str>,
) -> Result<SpawnResult, TerminalError> {
    info!(
        event = "terminal.spawn_started",
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
            "native" => TerminalType::Native,
            _ => {
                warn!(
                    event = "terminal.unknown_preference",
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
        event = "terminal.detect_completed",
        terminal_type = %terminal_type,
        working_directory = %working_directory.display()
    );

    let spawn_config = SpawnConfig::new(
        terminal_type.clone(),
        working_directory.to_path_buf(),
        command.to_string(),
    );

    // Generate unique window title for Ghostty (based on session_id if available)
    let ghostty_window_title = session_id
        .map(|id| format!("shards-{}", id.replace('/', "-")))
        .unwrap_or_else(|| format!("shards-{}", uuid::Uuid::new_v4().simple()));

    // Execute spawn script and capture window ID
    let terminal_window_id = operations::execute_spawn_script(
        &spawn_config,
        Some(&ghostty_window_title),
    )?;

    debug!(
        event = "terminal.spawn_script_executed",
        terminal_type = %terminal_type,
        terminal_window_id = ?terminal_window_id
    );

    // Try to find the actual agent process with retry logic
    let agent_name = operations::extract_command_name(command);
    let (process_id, process_name, process_start_time) =
        find_agent_process_with_retry(&agent_name, command, config)?;

    let result = SpawnResult::new(
        terminal_type.clone(),
        command.to_string(),
        working_directory.to_path_buf(),
        process_id,
        process_name.clone(),
        process_start_time,
        terminal_window_id.clone(),
    );

    info!(
        event = "terminal.spawn_completed",
        terminal_type = %terminal_type,
        working_directory = %working_directory.display(),
        command = command,
        process_id = process_id,
        process_name = ?process_name,
        terminal_window_id = ?terminal_window_id
    );

    Ok(result)
}

pub fn detect_available_terminal() -> Result<TerminalType, TerminalError> {
    info!(event = "terminal.detect_started");

    let terminal_type = operations::detect_terminal()?;

    info!(
        event = "terminal.detect_completed",
        terminal_type = %terminal_type
    );

    Ok(terminal_type)
}

/// Close a terminal window for a session
///
/// This is a best-effort operation used during session destruction.
/// It will not fail if the terminal window is already closed or the terminal
/// application is not running.
///
/// # Arguments
/// * `terminal_type` - The type of terminal (iTerm, Terminal.app, Ghostty)
/// * `window_id` - The window ID (for iTerm/Terminal.app) or title (for Ghostty)
///
/// If window_id is None, the close is skipped to avoid closing the wrong window.
pub fn close_terminal(
    terminal_type: &TerminalType,
    window_id: Option<&str>,
) -> Result<(), TerminalError> {
    info!(
        event = "terminal.close_started",
        terminal_type = %terminal_type,
        window_id = ?window_id
    );

    let result = operations::close_terminal_window(terminal_type, window_id);

    match &result {
        Ok(()) => info!(
            event = "terminal.close_completed",
            terminal_type = %terminal_type,
            window_id = ?window_id
        ),
        Err(e) => warn!(
            event = "terminal.close_failed",
            terminal_type = %terminal_type,
            window_id = ?window_id,
            error = %e,
            message = "Continuing with destroy despite terminal close failure"
        ),
    }

    // Always return Ok - terminal close failure should not block destroy
    Ok(())
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
        let config = ShardsConfig::default();
        let result = spawn_terminal(Path::new("/nonexistent/directory"), "echo hello", &config, None);

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, TerminalError::WorkingDirectoryNotFound { .. }));
        }
    }

    #[test]
    fn test_spawn_terminal_empty_command() {
        let current_dir = std::env::current_dir().unwrap();
        let config = ShardsConfig::default();
        let result = spawn_terminal(&current_dir, "", &config, None);

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, TerminalError::InvalidCommand));
        }
    }

    #[test]
    #[ignore] // DANGEROUS: Actually closes terminal windows via AppleScript - run manually only
    fn test_close_terminal_returns_ok_for_all_terminal_types() {
        // WARNING: This test executes real AppleScript that closes terminal windows!
        // It will close the window with the specified ID (or skip if None).
        // Only run manually when no important terminal windows are open.
        //
        // close_terminal is designed to ALWAYS return Ok, even if the underlying
        // AppleScript operation fails. This is intentional - terminal close failure
        // should not block session destruction.
        let terminal_types = vec![
            TerminalType::ITerm,
            TerminalType::TerminalApp,
            TerminalType::Ghostty,
            TerminalType::Native,
        ];

        for terminal_type in terminal_types {
            // Test with None window_id - should skip close and return Ok
            let result = close_terminal(&terminal_type, None);
            assert!(
                result.is_ok(),
                "close_terminal should always return Ok for {:?}, but got {:?}",
                terminal_type,
                result
            );
        }
    }

    #[test]
    #[ignore] // DANGEROUS: Actually closes terminal windows via AppleScript - run manually only
    fn test_close_terminal_native_is_noop() {
        // WARNING: This test executes real AppleScript via detect_terminal -> close_terminal_window.
        // Only run manually when no important terminal windows are open.
        let result = close_terminal(&TerminalType::Native, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_close_terminal_with_no_window_id_skips() {
        // When window_id is None, close should be skipped to avoid closing wrong window
        let terminal_types = vec![
            TerminalType::ITerm,
            TerminalType::TerminalApp,
            TerminalType::Ghostty,
        ];

        for terminal_type in terminal_types {
            let result = close_terminal(&terminal_type, None);
            assert!(
                result.is_ok(),
                "close_terminal with None window_id should return Ok for {:?}",
                terminal_type
            );
        }
    }

    // Note: Testing actual terminal spawning is complex and system-dependent
    // Integration tests would be more appropriate for full spawn testing
}
