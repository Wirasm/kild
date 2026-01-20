use std::path::Path;
use std::process::Command;
use tracing::{debug, info, warn};

use crate::core::config::ShardsConfig;
use crate::terminal::{errors::TerminalError, operations, types::*};

/// Find agent process with retry logic and exponential backoff
fn find_agent_process_with_retry(
    agent_name: &str,
    command: &str,
    config: &ShardsConfig,
) -> Result<(Option<u32>, Option<String>, Option<u64>), TerminalError> {
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

pub fn spawn_terminal(
    working_directory: &Path,
    command: &str,
    config: &ShardsConfig,
) -> Result<SpawnResult, TerminalError> {
    info!(
        event = "terminal.spawn_started",
        working_directory = %working_directory.display(),
        command = command
    );

    let terminal_type = if let Some(preferred) = &config.terminal.preferred {
        // Try to use preferred terminal, fall back to detection if not available
        match preferred.as_str() {
            "iterm2" | "iterm" => TerminalType::ITerm,
            "terminal" => TerminalType::TerminalApp,
            _ => operations::detect_terminal()?,
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

    let spawn_command = operations::build_spawn_command(&spawn_config)?;

    debug!(
        event = "terminal.command_built",
        terminal_type = %terminal_type,
        command_args = ?spawn_command
    );

    // Execute the command asynchronously (don't wait for terminal to close)
    let mut cmd = Command::new(&spawn_command[0]);
    if spawn_command.len() > 1 {
        cmd.args(&spawn_command[1..]);
    }

    let _child = cmd.spawn().map_err(|e| TerminalError::SpawnFailed {
        message: format!("Failed to execute {}: {}", spawn_command[0], e),
    })?;

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
    );

    info!(
        event = "terminal.spawn_completed",
        terminal_type = %terminal_type,
        working_directory = %working_directory.display(),
        command = command,
        process_id = process_id,
        process_name = ?process_name
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
        let result = spawn_terminal(Path::new("/nonexistent/directory"), "echo hello", &config);

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, TerminalError::WorkingDirectoryNotFound { .. }));
        }
    }

    #[test]
    fn test_spawn_terminal_empty_command() {
        let current_dir = std::env::current_dir().unwrap();
        let config = ShardsConfig::default();
        let result = spawn_terminal(&current_dir, "", &config);

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, TerminalError::InvalidCommand));
        }
    }

    // Note: Testing actual terminal spawning is complex and system-dependent
    // Integration tests would be more appropriate for full spawn testing
}
