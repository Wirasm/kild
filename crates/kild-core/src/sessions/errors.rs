use crate::errors::KildError;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(
        "Kild '{name}' already exists.\n  Resume: kild open {name}\n  Remove: kild destroy {name}"
    )]
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
        "Cannot complete '{name}' with uncommitted changes.\n   Inspect first: git -C $(kild cd {name}) diff\n   If you are an agent, do NOT force-destroy without checking the kild first.\n   Use 'kild destroy --force {name}' to remove anyway (changes will be lost)."
    )]
    UncommittedChanges { name: String },

    #[error(
        "Cannot complete '{name}': no PR found for this branch.\n   If the work landed, push the branch and create a PR first.\n   To remove the kild without completing, use 'kild destroy {name}'."
    )]
    NoPrFound { name: String },

    #[error("Daemon error: {message}")]
    DaemonError { message: String },

    #[error(
        "Daemon PTY exited immediately (exit code: {exit_code:?}). Last output:\n{scrollback_tail}"
    )]
    DaemonPtyExitedEarly {
        exit_code: Option<i32>,
        scrollback_tail: String,
    },

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

    #[error("No agent team found for '{name}'. Session is not daemon-managed or has no teammates.")]
    NoTeammates { name: String },

    #[error(
        "Pane %0 is the leader session for '{branch}'. Use 'kild stop {branch}' to stop the leader."
    )]
    LeaderPaneStop { branch: String },

    #[error(
        "Pane '{pane_id}' not found in session '{branch}'. Use 'kild teammates {branch}' to list panes."
    )]
    PaneNotFound { pane_id: String, branch: String },
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
            SessionError::NoPrFound { .. } => "SESSION_NO_PR_FOUND",
            SessionError::DaemonError { .. } => "DAEMON_ERROR",
            SessionError::DaemonPtyExitedEarly { .. } => "DAEMON_PTY_EXITED_EARLY",
            SessionError::DaemonAutoStartFailed { .. } => "DAEMON_AUTO_START_FAILED",
            SessionError::ResumeUnsupported { .. } => "RESUME_UNSUPPORTED",
            SessionError::ResumeNoSessionId { .. } => "RESUME_NO_SESSION_ID",
            SessionError::NoTeammates { .. } => "SESSION_NO_TEAMMATES",
            SessionError::PaneNotFound { .. } => "SESSION_PANE_NOT_FOUND",
            SessionError::LeaderPaneStop { .. } => "SESSION_LEADER_PANE_STOP",
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
                | SessionError::NoPrFound { .. }
                | SessionError::ResumeUnsupported { .. }
                | SessionError::ResumeNoSessionId { .. }
                | SessionError::NoTeammates { .. }
                | SessionError::PaneNotFound { .. }
                | SessionError::LeaderPaneStop { .. }
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
        let display = error.to_string();
        assert!(display.contains("Kild 'test' already exists"));
        assert!(display.contains("kild open test"));
        assert!(display.contains("kild destroy test"));
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
        assert!(
            error
                .to_string()
                .contains("Cannot complete 'my-branch' with uncommitted changes")
        );
        assert!(
            error
                .to_string()
                .contains("do NOT force-destroy without checking")
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

    #[test]
    fn test_no_pr_found_error() {
        let error = SessionError::NoPrFound {
            name: "my-feature".to_string(),
        };
        assert!(error.to_string().contains("Cannot complete 'my-feature'"));
        assert!(error.to_string().contains("no PR found"));
        assert!(error.to_string().contains("kild destroy my-feature"));
        assert_eq!(error.error_code(), "SESSION_NO_PR_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_daemon_pty_exited_early_error() {
        let error = SessionError::DaemonPtyExitedEarly {
            exit_code: Some(1),
            scrollback_tail: "Error: agent failed to start".to_string(),
        };
        assert!(error.to_string().contains("exit code: Some(1)"));
        assert!(error.to_string().contains("agent failed to start"));
        assert_eq!(error.error_code(), "DAEMON_PTY_EXITED_EARLY");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_worktree_not_found_error() {
        let error = SessionError::WorktreeNotFound {
            path: std::path::PathBuf::from("/tmp/missing"),
        };
        assert!(error.to_string().contains("/tmp/missing"));
        assert_eq!(error.error_code(), "WORKTREE_NOT_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_invalid_structure_error() {
        let error = SessionError::InvalidStructure {
            field: "session ID is empty".to_string(),
        };
        assert!(error.to_string().contains("session ID is empty"));
        assert_eq!(error.error_code(), "INVALID_SESSION_STRUCTURE");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_invalid_port_count_error() {
        let error = SessionError::InvalidPortCount;
        assert!(error.to_string().contains("must be greater than 0"));
        assert_eq!(error.error_code(), "INVALID_PORT_COUNT");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_port_range_exhausted_error() {
        let error = SessionError::PortRangeExhausted;
        assert!(error.to_string().contains("no available ports"));
        assert_eq!(error.error_code(), "PORT_RANGE_EXHAUSTED");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_port_allocation_failed_error() {
        let error = SessionError::PortAllocationFailed {
            message: "conflict".to_string(),
        };
        assert!(error.to_string().contains("conflict"));
        assert_eq!(error.error_code(), "PORT_ALLOCATION_FAILED");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_git_error_wrapping() {
        let git_err = crate::git::errors::GitError::NotInRepository;
        let error = SessionError::GitError { source: git_err };
        assert!(error.to_string().contains("Git operation failed"));
        assert_eq!(error.error_code(), "GIT_ERROR");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_terminal_error_wrapping() {
        let term_err = crate::terminal::errors::TerminalError::NoTerminalFound;
        let error = SessionError::TerminalError { source: term_err };
        assert!(error.to_string().contains("Terminal operation failed"));
        assert_eq!(error.error_code(), "TERMINAL_ERROR");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_io_error_wrapping() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let error = SessionError::IoError { source: io_err };
        assert!(error.to_string().contains("IO operation failed"));
        assert_eq!(error.error_code(), "IO_ERROR");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_process_not_found_error() {
        let error = SessionError::ProcessNotFound { pid: 12345 };
        assert!(error.to_string().contains("12345"));
        assert_eq!(error.error_code(), "PROCESS_NOT_FOUND");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_process_kill_failed_error() {
        let error = SessionError::ProcessKillFailed {
            pid: 99,
            message: "permission denied".to_string(),
        };
        assert!(error.to_string().contains("99"));
        assert!(error.to_string().contains("permission denied"));
        assert_eq!(error.error_code(), "PROCESS_KILL_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_process_access_denied_error() {
        let error = SessionError::ProcessAccessDenied { pid: 42 };
        assert!(error.to_string().contains("42"));
        assert_eq!(error.error_code(), "PROCESS_ACCESS_DENIED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_invalid_process_metadata_error() {
        let error = SessionError::InvalidProcessMetadata;
        assert!(
            error
                .to_string()
                .contains("must all be present or all absent")
        );
        assert_eq!(error.error_code(), "INVALID_PROCESS_METADATA");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_config_error() {
        let error = SessionError::ConfigError {
            message: "bad toml".to_string(),
        };
        assert!(error.to_string().contains("bad toml"));
        assert_eq!(error.error_code(), "CONFIG_ERROR");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_daemon_error() {
        let error = SessionError::DaemonError {
            message: "connection refused".to_string(),
        };
        assert!(error.to_string().contains("connection refused"));
        assert_eq!(error.error_code(), "DAEMON_ERROR");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_no_teammates_error() {
        let error = SessionError::NoTeammates {
            name: "auth".to_string(),
        };
        assert!(error.to_string().contains("auth"));
        assert!(error.to_string().contains("no teammates"));
        assert_eq!(error.error_code(), "SESSION_NO_TEAMMATES");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_pane_not_found_error() {
        let error = SessionError::PaneNotFound {
            pane_id: "%2".to_string(),
            branch: "auth".to_string(),
        };
        assert!(error.to_string().contains("%2"));
        assert!(error.to_string().contains("auth"));
        assert!(error.to_string().contains("kild teammates auth"));
        assert_eq!(error.error_code(), "SESSION_PANE_NOT_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_leader_pane_stop_error() {
        let error = SessionError::LeaderPaneStop {
            branch: "auth".to_string(),
        };
        assert!(error.to_string().contains("%0"));
        assert!(error.to_string().contains("auth"));
        assert!(error.to_string().contains("kild stop auth"));
        assert_eq!(error.error_code(), "SESSION_LEADER_PANE_STOP");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_daemon_auto_start_failed_delegates_is_user_error() {
        let disabled_error = SessionError::DaemonAutoStartFailed {
            source: crate::daemon::errors::DaemonAutoStartError::Disabled,
        };
        assert_eq!(disabled_error.error_code(), "DAEMON_AUTO_START_FAILED");
        assert!(disabled_error.is_user_error());

        let spawn_error = SessionError::DaemonAutoStartFailed {
            source: crate::daemon::errors::DaemonAutoStartError::SpawnFailed {
                message: "failed".to_string(),
            },
        };
        assert_eq!(spawn_error.error_code(), "DAEMON_AUTO_START_FAILED");
        assert!(!spawn_error.is_user_error());
    }
}
