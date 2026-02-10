use crate::errors::KildError;

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

    #[error("Invalid session structure: {field}")]
    InvalidStructure { field: String },

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

    #[error(
        "Invalid process metadata: process_id, process_name, and process_start_time must all be present or all absent"
    )]
    InvalidProcessMetadata,

    #[error("Invalid agent status: '{status}'. Valid: working, idle, waiting, done, error")]
    InvalidAgentStatus { status: String },

    #[error("Configuration error: {message}")]
    ConfigError { message: String },

    #[error(
        "Cannot complete '{name}' with uncommitted changes. Use 'kild destroy --force' to remove."
    )]
    UncommittedChanges { name: String },

    #[error("Daemon error: {message}")]
    DaemonError { message: String },

    #[error("Daemon auto-start failed: {source}")]
    DaemonAutoStartFailed {
        #[from]
        source: crate::daemon::errors::DaemonAutoStartError,
    },

    #[error("Agent '{agent}' does not support session resume. Only 'claude' supports --resume.")]
    ResumeUnsupported { agent: String },

    #[error(
        "No previous session ID found for '{branch}'. Cannot resume — this kild was created before resume support was added."
    )]
    ResumeNoSessionId { branch: String },
}

impl KildError for SessionError {
    fn error_code(&self) -> &'static str {
        match self {
            SessionError::AlreadyExists { .. } => "SESSION_ALREADY_EXISTS",
            SessionError::NotFound { .. } => "SESSION_NOT_FOUND",
            SessionError::WorktreeNotFound { .. } => "WORKTREE_NOT_FOUND",
            SessionError::InvalidName => "INVALID_SESSION_NAME",
            SessionError::InvalidCommand => "INVALID_COMMAND",
            SessionError::InvalidStructure { .. } => "INVALID_SESSION_STRUCTURE",
            SessionError::InvalidPortCount => "INVALID_PORT_COUNT",
            SessionError::PortRangeExhausted => "PORT_RANGE_EXHAUSTED",
            SessionError::PortAllocationFailed { .. } => "PORT_ALLOCATION_FAILED",
            SessionError::GitError { .. } => "GIT_ERROR",
            SessionError::TerminalError { .. } => "TERMINAL_ERROR",
            SessionError::IoError { .. } => "IO_ERROR",
            SessionError::ProcessNotFound { .. } => "PROCESS_NOT_FOUND",
            SessionError::ProcessKillFailed { .. } => "PROCESS_KILL_FAILED",
            SessionError::ProcessAccessDenied { .. } => "PROCESS_ACCESS_DENIED",
            SessionError::InvalidProcessMetadata => "INVALID_PROCESS_METADATA",
            SessionError::InvalidAgentStatus { .. } => "INVALID_AGENT_STATUS",
            SessionError::ConfigError { .. } => "CONFIG_ERROR",
            SessionError::UncommittedChanges { .. } => "SESSION_UNCOMMITTED_CHANGES",
            SessionError::DaemonError { .. } => "DAEMON_ERROR",
            SessionError::DaemonAutoStartFailed { .. } => "DAEMON_AUTO_START_FAILED",
            SessionError::ResumeUnsupported { .. } => "RESUME_UNSUPPORTED",
            SessionError::ResumeNoSessionId { .. } => "RESUME_NO_SESSION_ID",
        }
    }

    fn is_user_error(&self) -> bool {
        if let SessionError::DaemonAutoStartFailed { source } = self {
            return source.is_user_error();
        }
        matches!(
            self,
            SessionError::AlreadyExists { .. }
                | SessionError::NotFound { .. }
                | SessionError::WorktreeNotFound { .. }
                | SessionError::InvalidName
                | SessionError::InvalidCommand
                | SessionError::InvalidStructure { .. }
                | SessionError::InvalidPortCount
                | SessionError::PortRangeExhausted
                | SessionError::PortAllocationFailed { .. }
                | SessionError::InvalidProcessMetadata
                | SessionError::InvalidAgentStatus { .. }
                | SessionError::ConfigError { .. }
                | SessionError::UncommittedChanges { .. }
                | SessionError::ResumeUnsupported { .. }
                | SessionError::ResumeNoSessionId { .. }
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

    #[test]
    fn test_uncommitted_changes_error() {
        let error = SessionError::UncommittedChanges {
            name: "my-branch".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Cannot complete 'my-branch' with uncommitted changes. Use 'kild destroy --force' to remove."
        );
        assert_eq!(error.error_code(), "SESSION_UNCOMMITTED_CHANGES");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_invalid_agent_status_error() {
        let error = SessionError::InvalidAgentStatus {
            status: "bogus".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Invalid agent status: 'bogus'. Valid: working, idle, waiting, done, error"
        );
        assert_eq!(error.error_code(), "INVALID_AGENT_STATUS");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_resume_unsupported_error() {
        let error = SessionError::ResumeUnsupported {
            agent: "kiro".to_string(),
        };
        assert!(error.to_string().contains("kiro"));
        assert!(
            error
                .to_string()
                .contains("does not support session resume")
        );
        assert_eq!(error.error_code(), "RESUME_UNSUPPORTED");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_resume_no_session_id_error() {
        let error = SessionError::ResumeNoSessionId {
            branch: "my-feature".to_string(),
        };
        assert!(error.to_string().contains("my-feature"));
        assert!(error.to_string().contains("No previous session ID"));
        assert_eq!(error.error_code(), "RESUME_NO_SESSION_ID");
        assert!(error.is_user_error());
    }
}
