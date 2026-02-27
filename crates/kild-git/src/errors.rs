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

    #[error("Merge analysis failed: {message}")]
    MergeAnalysisFailed { message: String },

    #[error("Git log failed: {message}")]
    LogFailed { message: String },

    #[error("IO error during git operation: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_error_display() {
        let error = GitError::NotInRepository;
        assert_eq!(error.to_string(), "Not in a git repository");
    }

    #[test]
    fn test_branch_errors() {
        let exists_error = GitError::BranchAlreadyExists {
            branch: "main".to_string(),
        };
        assert_eq!(exists_error.to_string(), "Branch 'main' already exists");

        let not_found_error = GitError::BranchNotFound {
            branch: "missing".to_string(),
        };
        assert_eq!(not_found_error.to_string(), "Branch 'missing' not found");
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
