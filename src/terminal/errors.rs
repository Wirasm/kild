use crate::core::errors::ShardsError;

#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    #[error("No supported terminal found (tried: iTerm, Terminal.app)")]
    NoTerminalFound,

    #[error("Terminal '{terminal}' not found or not executable")]
    TerminalNotFound { terminal: String },

    #[error("Failed to spawn terminal process: {message}")]
    SpawnFailed { message: String },

    #[error("Working directory does not exist: {path}")]
    WorkingDirectoryNotFound { path: String },

    #[error("Command is empty or invalid")]
    InvalidCommand,

    #[error("AppleScript execution failed: {message}")]
    AppleScriptExecution { message: String },

    #[error("AppleScript failed with error: {stderr}")]
    AppleScriptFailed { stderr: String },

    #[error("IO error during terminal operation: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
}

impl ShardsError for TerminalError {
    fn error_code(&self) -> &'static str {
        match self {
            TerminalError::NoTerminalFound => "NO_TERMINAL_FOUND",
            TerminalError::TerminalNotFound { .. } => "TERMINAL_NOT_FOUND",
            TerminalError::SpawnFailed { .. } => "TERMINAL_SPAWN_FAILED",
            TerminalError::WorkingDirectoryNotFound { .. } => "WORKING_DIRECTORY_NOT_FOUND",
            TerminalError::InvalidCommand => "INVALID_COMMAND",
            TerminalError::AppleScriptExecution { .. } => "APPLESCRIPT_EXECUTION_FAILED",
            TerminalError::AppleScriptFailed { .. } => "APPLESCRIPT_FAILED",
            TerminalError::IoError { .. } => "TERMINAL_IO_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            TerminalError::NoTerminalFound
                | TerminalError::WorkingDirectoryNotFound { .. }
                | TerminalError::InvalidCommand
                | TerminalError::AppleScriptExecution { .. }
                | TerminalError::AppleScriptFailed { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_error_display() {
        let error = TerminalError::NoTerminalFound;
        assert_eq!(
            error.to_string(),
            "No supported terminal found (tried: iTerm, Terminal.app)"
        );
        assert_eq!(error.error_code(), "NO_TERMINAL_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_terminal_not_found() {
        let error = TerminalError::TerminalNotFound {
            terminal: "iterm".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Terminal 'iterm' not found or not executable"
        );
        assert_eq!(error.error_code(), "TERMINAL_NOT_FOUND");
    }

    #[test]
    fn test_spawn_failed() {
        let error = TerminalError::SpawnFailed {
            message: "Process failed".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Failed to spawn terminal process: Process failed"
        );
        assert_eq!(error.error_code(), "TERMINAL_SPAWN_FAILED");
    }

    #[test]
    fn test_working_directory_not_found() {
        let error = TerminalError::WorkingDirectoryNotFound {
            path: "/tmp/missing".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Working directory does not exist: /tmp/missing"
        );
        assert!(error.is_user_error());
    }

    #[test]
    fn test_invalid_command() {
        let error = TerminalError::InvalidCommand;
        assert_eq!(error.to_string(), "Command is empty or invalid");
        assert!(error.is_user_error());
    }
}
