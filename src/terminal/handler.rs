use std::path::Path;
use std::process::Command;
use tracing::{debug, info};

use crate::terminal::{errors::TerminalError, operations, types::*};

pub fn spawn_terminal(
    working_directory: &Path,
    command: &str,
) -> Result<SpawnResult, TerminalError> {
    info!(
        event = "terminal.spawn_started",
        working_directory = %working_directory.display(),
        command = command
    );

    let terminal_type = operations::detect_terminal()?;

    debug!(
        event = "terminal.detect_completed",
        terminal_type = %terminal_type,
        working_directory = %working_directory.display()
    );

    let config = SpawnConfig::new(
        terminal_type.clone(),
        working_directory.to_path_buf(),
        command.to_string(),
    );

    let spawn_command = operations::build_spawn_command(&config)?;

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

    let result = SpawnResult::new(
        terminal_type.clone(),
        command.to_string(),
        working_directory.to_path_buf(),
    );

    info!(
        event = "terminal.spawn_completed",
        terminal_type = %terminal_type,
        working_directory = %working_directory.display(),
        command = command
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
        let result = spawn_terminal(Path::new("/nonexistent/directory"), "echo hello");

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, TerminalError::WorkingDirectoryNotFound { .. }));
        }
    }

    #[test]
    fn test_spawn_terminal_empty_command() {
        let current_dir = std::env::current_dir().unwrap();
        let result = spawn_terminal(&current_dir, "");

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, TerminalError::InvalidCommand));
        }
    }

    // Note: Testing actual terminal spawning is complex and system-dependent
    // Integration tests would be more appropriate for full spawn testing
}
