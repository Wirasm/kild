use crate::errors::KildError;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Not in a git repository")]
    NotInRepository,

    #[error("Repository not found at path: {path}")]
    RepositoryNotFound { path: String },

    #[error("Branch '{branch}' already exists")]
    BranchAlreadyExists { branch: String },

    #[error("Branch '{branch}' not found")]
    BranchNotFound { branch: String },

    #[error("Worktree already exists at path: {path}")]
    WorktreeAlreadyExists { path: String },

    #[error("Worktree not found at path: {path}")]
    WorktreeNotFound { path: String },

    #[error("Failed to remove worktree at {path}: {message}")]
    WorktreeRemovalFailed { path: String, message: String },

    #[error("Invalid path: {path}: {message}")]
    InvalidPath { path: String, message: String },

    #[error("Git operation failed: {message}")]
    OperationFailed { message: String },

    #[error("Failed to fetch from remote '{remote}': {message}")]
    FetchFailed { remote: String, message: String },

    #[error("Git2 library error: {source}")]
    Git2Error {
        #[from]
        source: git2::Error,
    },

    #[error("Rebase conflict onto '{base_branch}' in worktree at {}", worktree_path.display())]
    RebaseConflict {
        base_branch: String,
        worktree_path: std::path::PathBuf,
    },

    #[error("Rebase abort failed for '{base_branch}' at {}: {message}", worktree_path.display())]
    RebaseAbortFailed {
        base_branch: String,
        worktree_path: std::path::PathBuf,
        message: String,
    },

    #[error("Failed to delete remote branch '{branch}': {message}")]
    RemoteBranchDeleteFailed { branch: String, message: String },

    #[error("Git diff failed: {message}")]
    DiffFailed { message: String },

    #[error("Git log failed: {message}")]
    LogFailed { message: String },

    #[error("IO error during git operation: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
}

impl KildError for GitError {
    fn error_code(&self) -> &'static str {
        match self {
            GitError::NotInRepository => "NOT_IN_REPOSITORY",
            GitError::RepositoryNotFound { .. } => "REPOSITORY_NOT_FOUND",
            GitError::BranchAlreadyExists { .. } => "BRANCH_ALREADY_EXISTS",
            GitError::BranchNotFound { .. } => "BRANCH_NOT_FOUND",
            GitError::WorktreeAlreadyExists { .. } => "WORKTREE_ALREADY_EXISTS",
            GitError::WorktreeNotFound { .. } => "WORKTREE_NOT_FOUND",
            GitError::WorktreeRemovalFailed { .. } => "WORKTREE_REMOVAL_FAILED",
            GitError::InvalidPath { .. } => "INVALID_PATH",
            GitError::OperationFailed { .. } => "GIT_OPERATION_FAILED",
            GitError::FetchFailed { .. } => "GIT_FETCH_FAILED",
            GitError::RebaseConflict { .. } => "GIT_REBASE_CONFLICT",
            GitError::RebaseAbortFailed { .. } => "GIT_REBASE_ABORT_FAILED",
            GitError::RemoteBranchDeleteFailed { .. } => "GIT_REMOTE_BRANCH_DELETE_FAILED",
            GitError::DiffFailed { .. } => "GIT_DIFF_FAILED",
            GitError::LogFailed { .. } => "GIT_LOG_FAILED",
            GitError::Git2Error { .. } => "GIT2_ERROR",
            GitError::IoError { .. } => "GIT_IO_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            GitError::NotInRepository
                | GitError::BranchAlreadyExists { .. }
                | GitError::BranchNotFound { .. }
                | GitError::WorktreeAlreadyExists { .. }
                // RebaseConflict is a user error: user must resolve conflicts manually.
                // RebaseAbortFailed is NOT — it's an internal failure to clean up after conflict.
                | GitError::RebaseConflict { .. }
                // RemoteBranchDeleteFailed is user-actionable (network, auth, invalid remote).
                | GitError::RemoteBranchDeleteFailed { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_error_display() {
        let error = GitError::NotInRepository;
        assert_eq!(error.to_string(), "Not in a git repository");
        assert_eq!(error.error_code(), "NOT_IN_REPOSITORY");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_branch_errors() {
        let exists_error = GitError::BranchAlreadyExists {
            branch: "main".to_string(),
        };
        assert_eq!(exists_error.to_string(), "Branch 'main' already exists");
        assert!(exists_error.is_user_error());

        let not_found_error = GitError::BranchNotFound {
            branch: "missing".to_string(),
        };
        assert_eq!(not_found_error.to_string(), "Branch 'missing' not found");
        assert!(not_found_error.is_user_error());
    }

    #[test]
    fn test_rebase_conflict_error() {
        let error = GitError::RebaseConflict {
            base_branch: "main".to_string(),
            worktree_path: std::path::PathBuf::from("/tmp/test-worktree"),
        };
        let display = error.to_string();
        assert!(display.contains("main"), "should include base_branch");
        assert!(
            display.contains("/tmp/test-worktree"),
            "should include worktree_path"
        );
        assert_eq!(error.error_code(), "GIT_REBASE_CONFLICT");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_rebase_abort_failed_error() {
        let error = GitError::RebaseAbortFailed {
            base_branch: "main".to_string(),
            worktree_path: std::path::PathBuf::from("/tmp/test-worktree"),
            message: "working tree has changes".to_string(),
        };
        let display = error.to_string();
        assert!(display.contains("main"), "should include base_branch");
        assert!(
            display.contains("/tmp/test-worktree"),
            "should include worktree_path"
        );
        assert!(
            display.contains("working tree has changes"),
            "should include message"
        );
        assert_eq!(error.error_code(), "GIT_REBASE_ABORT_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_remote_branch_delete_failed_is_user_error() {
        let error = GitError::RemoteBranchDeleteFailed {
            branch: "kild/test".to_string(),
            message: "authentication failed".to_string(),
        };
        assert_eq!(error.error_code(), "GIT_REMOTE_BRANCH_DELETE_FAILED");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_diff_failed_error() {
        let error = GitError::DiffFailed {
            message: "Failed to execute git".to_string(),
        };
        assert_eq!(error.to_string(), "Git diff failed: Failed to execute git");
        assert_eq!(error.error_code(), "GIT_DIFF_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_log_failed_error() {
        let error = GitError::LogFailed {
            message: "git log failed: no commits".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Git log failed: git log failed: no commits"
        );
        assert_eq!(error.error_code(), "GIT_LOG_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_worktree_errors() {
        let exists_error = GitError::WorktreeAlreadyExists {
            path: "/tmp/test".to_string(),
        };
        assert_eq!(
            exists_error.to_string(),
            "Worktree already exists at path: /tmp/test"
        );

        let not_found_error = GitError::WorktreeNotFound {
            path: "/tmp/missing".to_string(),
        };
        assert_eq!(
            not_found_error.to_string(),
            "Worktree not found at path: /tmp/missing"
        );
    }
}
