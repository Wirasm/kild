use std::error::Error;

/// Base trait for all application errors
pub trait KildError: Error + Send + Sync + 'static {
    /// Error code for programmatic handling
    fn error_code(&self) -> &'static str;

    /// Whether this error should be logged as an error or warning
    fn is_user_error(&self) -> bool {
        false
    }
}

/// Common result type for the application
pub type KildResult<T> = Result<T, Box<dyn KildError>>;

impl KildError for kild_git::GitError {
    fn error_code(&self) -> &'static str {
        match self {
            kild_git::GitError::NotInRepository => "NOT_IN_REPOSITORY",
            kild_git::GitError::RepositoryNotFound { .. } => "REPOSITORY_NOT_FOUND",
            kild_git::GitError::BranchAlreadyExists { .. } => "BRANCH_ALREADY_EXISTS",
            kild_git::GitError::BranchNotFound { .. } => "BRANCH_NOT_FOUND",
            kild_git::GitError::WorktreeAlreadyExists { .. } => "WORKTREE_ALREADY_EXISTS",
            kild_git::GitError::WorktreeNotFound { .. } => "WORKTREE_NOT_FOUND",
            kild_git::GitError::WorktreeRemovalFailed { .. } => "WORKTREE_REMOVAL_FAILED",
            kild_git::GitError::InvalidPath { .. } => "INVALID_PATH",
            kild_git::GitError::OperationFailed { .. } => "GIT_OPERATION_FAILED",
            kild_git::GitError::FetchFailed { .. } => "GIT_FETCH_FAILED",
            kild_git::GitError::RebaseConflict { .. } => "GIT_REBASE_CONFLICT",
            kild_git::GitError::RebaseAbortFailed { .. } => "GIT_REBASE_ABORT_FAILED",
            kild_git::GitError::RemoteBranchDeleteFailed { .. } => {
                "GIT_REMOTE_BRANCH_DELETE_FAILED"
            }
            kild_git::GitError::DiffFailed { .. } => "GIT_DIFF_FAILED",
            kild_git::GitError::MergeAnalysisFailed { .. } => "GIT_MERGE_ANALYSIS_FAILED",
            kild_git::GitError::LogFailed { .. } => "GIT_LOG_FAILED",
            kild_git::GitError::Git2Error { .. } => "GIT2_ERROR",
            kild_git::GitError::IoError { .. } => "GIT_IO_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            kild_git::GitError::NotInRepository
                | kild_git::GitError::BranchAlreadyExists { .. }
                | kild_git::GitError::BranchNotFound { .. }
                | kild_git::GitError::WorktreeAlreadyExists { .. }
                | kild_git::GitError::RebaseConflict { .. }
                | kild_git::GitError::RemoteBranchDeleteFailed { .. }
        )
    }
}

impl KildError for kild_config::ConfigError {
    fn error_code(&self) -> &'static str {
        match self {
            kild_config::ConfigError::ConfigParseError { .. } => "CONFIG_PARSE_ERROR",
            kild_config::ConfigError::InvalidAgent { .. } => "INVALID_AGENT",
            kild_config::ConfigError::InvalidConfiguration { .. } => "INVALID_CONFIGURATION",
            kild_config::ConfigError::IoError { .. } => "CONFIG_IO_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            kild_config::ConfigError::ConfigParseError { .. }
                | kild_config::ConfigError::InvalidAgent { .. }
                | kild_config::ConfigError::InvalidConfiguration { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kild_result() {
        let _result: KildResult<i32> = Ok(42);
    }

    #[test]
    fn test_config_error_display() {
        use crate::agents::supported_agents_string;
        let error = kild_config::ConfigError::InvalidAgent {
            agent: "unknown".to_string(),
            supported_agents: supported_agents_string(),
        };
        let msg = error.to_string();
        // Verify message format
        assert!(msg.starts_with("Invalid agent 'unknown'. Supported agents: "));
        // Verify all valid agents are listed
        assert!(msg.contains("amp"), "Error should list amp");
        assert!(msg.contains("claude"), "Error should list claude");
        assert!(msg.contains("kiro"), "Error should list kiro");
        assert!(msg.contains("gemini"), "Error should list gemini");
        assert!(msg.contains("codex"), "Error should list codex");
        // Verify removed agents are NOT listed
        assert!(
            !msg.contains("aether"),
            "Error should NOT list removed agent aether"
        );
        // Verify error trait methods
        assert_eq!(error.error_code(), "INVALID_AGENT");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_config_parse_error() {
        let error = kild_config::ConfigError::ConfigParseError {
            message: "invalid TOML syntax".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Failed to parse config file: invalid TOML syntax"
        );
        assert_eq!(error.error_code(), "CONFIG_PARSE_ERROR");
        assert!(error.is_user_error());
    }
}
