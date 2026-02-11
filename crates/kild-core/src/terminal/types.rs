use crate::terminal::errors::TerminalError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TerminalType {
    #[serde(alias = "ITerm")]
    ITerm,
    #[serde(rename = "terminal", alias = "TerminalApp")]
    TerminalApp,
    #[serde(alias = "Ghostty")]
    Ghostty,
    #[serde(alias = "Alacritty")]
    Alacritty,
    #[serde(alias = "Native")]
    Native, // System default
}

/// Configuration for spawning a terminal window.
///
/// Fields are private to enforce validation at construction time.
/// Use accessor methods to read values.
#[derive(Debug, Clone)]
pub struct SpawnConfig {
    terminal_type: TerminalType,
    working_directory: PathBuf,
    command: String,
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
    /// Create a new spawn configuration without validation.
    ///
    /// This constructor allows creating configs that may be invalid (e.g., for testing
    /// or when validation will be done separately). Use `try_new()` for validated construction.
    pub fn new(terminal_type: TerminalType, working_directory: PathBuf, command: String) -> Self {
        Self {
            terminal_type,
            working_directory,
            command,
        }
    }

    /// Create a new spawn configuration with validation.
    ///
    /// Returns an error if the command is empty or the working directory doesn't exist.
    pub fn try_new(
        terminal_type: TerminalType,
        working_directory: PathBuf,
        command: String,
    ) -> Result<Self, TerminalError> {
        let config = Self {
            terminal_type,
            working_directory,
            command,
        };
        config.validate()?;
        Ok(config)
    }

    /// Get the terminal type.
    pub fn terminal_type(&self) -> &TerminalType {
        &self.terminal_type
    }

    /// Get the working directory path.
    pub fn working_directory(&self) -> &std::path::Path {
        &self.working_directory
    }

    /// Get the command to execute.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Validate the spawn configuration.
    pub fn validate(&self) -> Result<(), TerminalError> {
        if self.command.trim().is_empty() {
            return Err(TerminalError::InvalidCommand);
        }

        if !self.working_directory.exists() {
            return Err(TerminalError::WorkingDirectoryNotFound {
                path: self.working_directory.display().to_string(),
            });
        }

        Ok(())
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
            TerminalType::Alacritty => write!(f, "alacritty"),
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
        assert_eq!(TerminalType::Alacritty.to_string(), "alacritty");
        assert_eq!(TerminalType::Native.to_string(), "native");
    }

    #[test]
    fn test_spawn_config() {
        let config = SpawnConfig::new(
            TerminalType::ITerm,
            PathBuf::from("/tmp/test"),
            "echo hello".to_string(),
        );

        assert_eq!(*config.terminal_type(), TerminalType::ITerm);
        assert_eq!(config.working_directory(), PathBuf::from("/tmp/test"));
        assert_eq!(config.command(), "echo hello");
    }

    #[test]
    fn test_spawn_config_ghostty() {
        let config = SpawnConfig::new(
            TerminalType::Ghostty,
            PathBuf::from("/tmp/test"),
            "kiro-cli chat".to_string(),
        );

        assert_eq!(*config.terminal_type(), TerminalType::Ghostty);
        assert_eq!(config.working_directory(), PathBuf::from("/tmp/test"));
        assert_eq!(config.command(), "kiro-cli chat");
    }

    #[test]
    fn test_native_terminal_type() {
        let config = SpawnConfig::new(
            TerminalType::Native,
            PathBuf::from("/tmp/test"),
            "echo hello".to_string(),
        );

        assert_eq!(*config.terminal_type(), TerminalType::Native);
        assert_eq!(config.working_directory(), PathBuf::from("/tmp/test"));
        assert_eq!(config.command(), "echo hello");
    }

    #[test]
    fn test_spawn_config_try_new_valid() {
        let result = SpawnConfig::try_new(
            TerminalType::ITerm,
            std::env::current_dir().unwrap(),
            "echo hello".to_string(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_spawn_config_try_new_invalid_command() {
        let result = SpawnConfig::try_new(
            TerminalType::ITerm,
            std::env::current_dir().unwrap(),
            "".to_string(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_spawn_config_try_new_invalid_directory() {
        let result = SpawnConfig::try_new(
            TerminalType::ITerm,
            PathBuf::from("/nonexistent/directory/path"),
            "echo hello".to_string(),
        );
        assert!(result.is_err());
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

    #[test]
    fn test_spawn_config_validate_empty_command() {
        let config = SpawnConfig::new(
            TerminalType::ITerm,
            std::env::current_dir().unwrap(),
            "".to_string(),
        );
        assert!(config.validate().is_err());

        let config_whitespace = SpawnConfig::new(
            TerminalType::ITerm,
            std::env::current_dir().unwrap(),
            "   ".to_string(),
        );
        assert!(config_whitespace.validate().is_err());
    }

    #[test]
    fn test_spawn_config_validate_nonexistent_directory() {
        let config = SpawnConfig::new(
            TerminalType::ITerm,
            PathBuf::from("/nonexistent/directory/path"),
            "echo hello".to_string(),
        );
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_spawn_config_validate_success() {
        let config = SpawnConfig::new(
            TerminalType::ITerm,
            std::env::current_dir().unwrap(),
            "echo hello".to_string(),
        );
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_terminal_type_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&TerminalType::ITerm).unwrap(),
            r#""iterm""#
        );
        assert_eq!(
            serde_json::to_string(&TerminalType::TerminalApp).unwrap(),
            r#""terminal""#
        );
        assert_eq!(
            serde_json::to_string(&TerminalType::Ghostty).unwrap(),
            r#""ghostty""#
        );
        assert_eq!(
            serde_json::to_string(&TerminalType::Alacritty).unwrap(),
            r#""alacritty""#
        );
        assert_eq!(
            serde_json::to_string(&TerminalType::Native).unwrap(),
            r#""native""#
        );
    }

    #[test]
    fn test_terminal_type_deserializes_old_pascal_case() {
        assert_eq!(
            serde_json::from_str::<TerminalType>(r#""ITerm""#).unwrap(),
            TerminalType::ITerm
        );
        assert_eq!(
            serde_json::from_str::<TerminalType>(r#""TerminalApp""#).unwrap(),
            TerminalType::TerminalApp
        );
        assert_eq!(
            serde_json::from_str::<TerminalType>(r#""Ghostty""#).unwrap(),
            TerminalType::Ghostty
        );
        assert_eq!(
            serde_json::from_str::<TerminalType>(r#""Alacritty""#).unwrap(),
            TerminalType::Alacritty
        );
        assert_eq!(
            serde_json::from_str::<TerminalType>(r#""Native""#).unwrap(),
            TerminalType::Native
        );
    }

    #[test]
    fn test_terminal_type_roundtrip_new_format() {
        for tt in [
            TerminalType::ITerm,
            TerminalType::TerminalApp,
            TerminalType::Ghostty,
            TerminalType::Alacritty,
            TerminalType::Native,
        ] {
            let json = serde_json::to_string(&tt).unwrap();
            let parsed: TerminalType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, tt);
        }
    }
}
