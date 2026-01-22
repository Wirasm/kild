use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TerminalType {
    ITerm,
    TerminalApp,
    Ghostty,
    Native, // System default
}

#[derive(Debug, Clone)]
pub struct SpawnConfig {
    pub terminal_type: TerminalType,
    pub working_directory: PathBuf,
    pub command: String,
}

#[derive(Debug, Clone)]
pub struct SpawnResult {
    pub terminal_type: TerminalType,
    pub command_executed: String,
    pub working_directory: PathBuf,
    pub process_id: Option<u32>,
    pub process_name: Option<String>,
    pub process_start_time: Option<u64>,
    /// Terminal window ID for closing the correct window on destroy.
    ///
    /// For iTerm2/Terminal.app: The AppleScript window ID (e.g., "1596")
    /// For Ghostty: The unique title set via ANSI escape sequence
    /// None if: spawn failed to capture ID or terminal doesn't support it
    pub terminal_window_id: Option<String>,
}

impl SpawnConfig {
    pub fn new(terminal_type: TerminalType, working_directory: PathBuf, command: String) -> Self {
        Self {
            terminal_type,
            working_directory,
            command,
        }
    }
}

impl SpawnResult {
    pub fn new(
        terminal_type: TerminalType,
        command_executed: String,
        working_directory: PathBuf,
        process_id: Option<u32>,
        process_name: Option<String>,
        process_start_time: Option<u64>,
        terminal_window_id: Option<String>,
    ) -> Self {
        Self {
            terminal_type,
            command_executed,
            working_directory,
            process_id,
            process_name,
            process_start_time,
            terminal_window_id,
        }
    }
}

impl std::fmt::Display for TerminalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalType::ITerm => write!(f, "iterm"),
            TerminalType::TerminalApp => write!(f, "terminal"),
            TerminalType::Ghostty => write!(f, "ghostty"),
            TerminalType::Native => write!(f, "native"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_type_display() {
        assert_eq!(TerminalType::ITerm.to_string(), "iterm");
        assert_eq!(TerminalType::TerminalApp.to_string(), "terminal");
        assert_eq!(TerminalType::Ghostty.to_string(), "ghostty");
        assert_eq!(TerminalType::Native.to_string(), "native");
    }

    #[test]
    fn test_spawn_config() {
        let config = SpawnConfig::new(
            TerminalType::ITerm,
            PathBuf::from("/tmp/test"),
            "echo hello".to_string(),
        );

        assert_eq!(config.terminal_type, TerminalType::ITerm);
        assert_eq!(config.working_directory, PathBuf::from("/tmp/test"));
        assert_eq!(config.command, "echo hello");
    }

    #[test]
    fn test_spawn_config_ghostty() {
        let config = SpawnConfig::new(
            TerminalType::Ghostty,
            PathBuf::from("/tmp/test"),
            "kiro-cli chat".to_string(),
        );
        
        assert_eq!(config.terminal_type, TerminalType::Ghostty);
        assert_eq!(config.working_directory, PathBuf::from("/tmp/test"));
        assert_eq!(config.command, "kiro-cli chat");
    }

    #[test]
    fn test_native_terminal_type() {
        let config = SpawnConfig::new(
            TerminalType::Native,
            PathBuf::from("/tmp/test"),
            "echo hello".to_string(),
        );
        
        assert_eq!(config.terminal_type, TerminalType::Native);
        assert_eq!(config.working_directory, PathBuf::from("/tmp/test"));
        assert_eq!(config.command, "echo hello");
    }

    #[test]
    fn test_terminal_type_equality() {
        assert_eq!(TerminalType::Native, TerminalType::Native);
        assert_ne!(TerminalType::Native, TerminalType::Ghostty);
        assert_ne!(TerminalType::Ghostty, TerminalType::ITerm);
    }

    #[test]
    fn test_spawn_result() {
        let result = SpawnResult::new(
            TerminalType::ITerm,
            "cc".to_string(),
            PathBuf::from("/path/to/worktree"),
            None,
            None,
            None,
            None,
        );

        assert_eq!(result.terminal_type, TerminalType::ITerm);
        assert_eq!(result.command_executed, "cc");
        assert_eq!(result.working_directory, PathBuf::from("/path/to/worktree"));
        assert_eq!(result.terminal_window_id, None);
    }

    #[test]
    fn test_spawn_result_with_window_id() {
        let result = SpawnResult::new(
            TerminalType::ITerm,
            "cc".to_string(),
            PathBuf::from("/path/to/worktree"),
            Some(12345),
            Some("cc".to_string()),
            Some(1234567890),
            Some("1596".to_string()),
        );

        assert_eq!(result.terminal_type, TerminalType::ITerm);
        assert_eq!(result.terminal_window_id, Some("1596".to_string()));
    }
}
