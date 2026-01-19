use std::path::Path;
use std::process::Command;
use tracing::{debug, info, warn};

use crate::core::config::ShardsConfig;
use crate::terminal::{errors::TerminalError, operations, types::*};

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
            "iterm2" | "iterm" => {
                // Verify iTerm2 is actually available
                if operations::app_exists_macos("iTerm") {
                    TerminalType::ITerm
                } else {
                    warn!(
                        event = "terminal.preferred_not_available",
                        preferred = "iTerm",
                        message = "Preferred terminal not found, falling back to detection"
                    );
                    operations::detect_terminal()?
                }
            }
            "terminal" => {
                // Verify Terminal.app is actually available
                if operations::app_exists_macos("Terminal") {
                    TerminalType::TerminalApp
                } else {
                    warn!(
                        event = "terminal.preferred_not_available",
                        preferred = "Terminal",
                        message = "Preferred terminal not found, falling back to detection"
                    );
                    operations::detect_terminal()?
                }
            }
            _ => {
                warn!(
                    event = "terminal.preferred_unknown",
                    preferred = preferred,
                    message = "Unknown preferred terminal, falling back to detection"
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

    let spawn_command = operations::build_spawn_command(&spawn_config)?;

    debug!(
        event = "terminal.command_built",
        terminal_type = %terminal_type,
        command_args = ?spawn_command
    );

    info!(
        event = "terminal.launching",
        terminal_type = %terminal_type,
        message = "Launching terminal..."
    );

    // For AppleScript commands, use our enhanced execution function
    if spawn_command[0] == "osascript" && spawn_command.len() >= 3 {
        operations::execute_applescript(&spawn_command[2])?;

        info!(
            event = "terminal.spawn_completed",
            terminal_type = %terminal_type,
            working_directory = %working_directory.display(),
            command = command,
            message = "Terminal launched successfully"
        );

        // For AppleScript, we don't get a direct process ID, so return None
        let result = SpawnResult::new(
            terminal_type.clone(),
            command.to_string(),
            working_directory.to_path_buf(),
            None,
            None,
            None,
        );

        return Ok(result);
    }

    // Execute the command asynchronously (don't wait for terminal to close)
    let mut cmd = Command::new(&spawn_command[0]);
    if spawn_command.len() > 1 {
        cmd.args(&spawn_command[1..]);
    }

    let child = cmd.spawn().map_err(|e| TerminalError::SpawnFailed {
        message: format!("Failed to execute {}: {}", spawn_command[0], e),
    })?;

    let process_id = child.id();

    // Capture process metadata immediately for PID reuse protection
    let (process_name, process_start_time) =
        if let Ok(info) = crate::process::get_process_info(process_id) {
            (Some(info.name), Some(info.start_time))
        } else {
            (None, None)
        };

    let result = SpawnResult::new(
        terminal_type.clone(),
        command.to_string(),
        working_directory.to_path_buf(),
        Some(process_id),
        process_name.clone(),
        process_start_time,
    );

    info!(
        event = "terminal.spawn_completed",
        terminal_type = %terminal_type,
        working_directory = %working_directory.display(),
        command = command,
        process_id = process_id,
        process_name = ?process_name,
        message = "Terminal launched successfully"
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
