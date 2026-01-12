use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum TerminalType {
    ITerm,
    TerminalApp,
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
    ) -> Self {
        Self {
            terminal_type,
            command_executed,
            working_directory,
        }
    }
}

impl std::fmt::Display for TerminalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalType::ITerm => write!(f, "iterm"),
            TerminalType::TerminalApp => write!(f, "terminal"),
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
    fn test_spawn_result() {
        let result = SpawnResult::new(
            TerminalType::ITerm,
            "cc".to_string(),
            PathBuf::from("/path/to/worktree"),
        );

        assert_eq!(result.terminal_type, TerminalType::ITerm);
        assert_eq!(result.command_executed, "cc");
        assert_eq!(result.working_directory, PathBuf::from("/path/to/worktree"));
    }
}
