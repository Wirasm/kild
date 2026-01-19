use crate::core::errors::ShardsError;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session '{name}' already exists")]
    AlreadyExists { name: String },

    #[error("Session '{name}' not found")]
    NotFound { name: String },

    #[error("Worktree not found at path: {path}")]
    WorktreeNotFound { path: std::path::PathBuf },

    #[error("Invalid session name: cannot be empty")]
    InvalidName,

    #[error("Invalid command: cannot be empty")]
    InvalidCommand,

    #[error("Invalid port count: must be greater than 0")]
    InvalidPortCount,

    #[error("Port range exhausted: no available ports in the configured range")]
    PortRangeExhausted,

    #[error("Port allocation failed: {message}")]
    PortAllocationFailed { message: String },

    #[error("Git operation failed: {source}")]
    GitError {
        #[from]
        source: crate::git::errors::GitError,
    },

    #[error("Terminal operation failed: {source}")]
    TerminalError {
        #[from]
        source: crate::terminal::errors::TerminalError,
    },

    #[error("IO operation failed: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },

    #[error("Process '{pid}' not found")]
    ProcessNotFound { pid: u32 },

    #[error("Failed to kill process '{pid}': {message}")]
    ProcessKillFailed { pid: u32, message: String },

    #[error("Access denied for process '{pid}'")]
    ProcessAccessDenied { pid: u32 },
}

impl ShardsError for SessionError {
    fn error_code(&self) -> &'static str {
        match self {
            SessionError::AlreadyExists { .. } => "SESSION_ALREADY_EXISTS",
            SessionError::NotFound { .. } => "SESSION_NOT_FOUND",
            SessionError::WorktreeNotFound { .. } => "WORKTREE_NOT_FOUND",
            SessionError::InvalidName => "INVALID_SESSION_NAME",
            SessionError::InvalidCommand => "INVALID_COMMAND",
            SessionError::InvalidPortCount => "INVALID_PORT_COUNT",
            SessionError::PortRangeExhausted => "PORT_RANGE_EXHAUSTED",
            SessionError::PortAllocationFailed { .. } => "PORT_ALLOCATION_FAILED",
            SessionError::GitError { .. } => "GIT_ERROR",
            SessionError::TerminalError { .. } => "TERMINAL_ERROR",
            SessionError::IoError { .. } => "IO_ERROR",
            SessionError::ProcessNotFound { .. } => "PROCESS_NOT_FOUND",
            SessionError::ProcessKillFailed { .. } => "PROCESS_KILL_FAILED",
            SessionError::ProcessAccessDenied { .. } => "PROCESS_ACCESS_DENIED",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            SessionError::AlreadyExists { .. }
                | SessionError::NotFound { .. }
                | SessionError::WorktreeNotFound { .. }
                | SessionError::InvalidName
                | SessionError::InvalidCommand
                | SessionError::InvalidPortCount
                | SessionError::PortRangeExhausted
                | SessionError::PortAllocationFailed { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_error_display() {
        let error = SessionError::AlreadyExists {
            name: "test".to_string(),
        };
        assert_eq!(error.to_string(), "Session 'test' already exists");
        assert_eq!(error.error_code(), "SESSION_ALREADY_EXISTS");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_session_error_not_found() {
        let error = SessionError::NotFound {
            name: "missing".to_string(),
        };
        assert_eq!(error.to_string(), "Session 'missing' not found");
        assert_eq!(error.error_code(), "SESSION_NOT_FOUND");
    }

    #[test]
    fn test_validation_errors() {
        let name_error = SessionError::InvalidName;
        assert_eq!(
            name_error.to_string(),
            "Invalid session name: cannot be empty"
        );
        assert!(name_error.is_user_error());

        let cmd_error = SessionError::InvalidCommand;
        assert_eq!(cmd_error.to_string(), "Invalid command: cannot be empty");
        assert!(cmd_error.is_user_error());
    }
}
