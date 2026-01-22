use crate::errors::ShardsError;

#[derive(Debug, thiserror::Error)]
pub enum CleanupError {
    #[error("Not in a git repository")]
    NotInRepository,

    #[error("No orphaned resources found")]
    NoOrphanedResources,

    #[error("Failed to scan for orphaned branches: {message}")]
    BranchScanFailed { message: String },

    #[error("Failed to scan for orphaned worktrees: {message}")]
    WorktreeScanFailed { message: String },

    #[error("Failed to cleanup resource '{name}': {message}")]
    CleanupFailed { name: String, message: String },

    #[error("Permission denied during cleanup: {path}")]
    PermissionDenied { path: String },

    #[error("Git operation failed: {source}")]
    GitError {
        #[from]
        source: crate::git::errors::GitError,
    },

    #[error("Session operation failed: {source}")]
    SessionError {
        #[from]
        source: crate::sessions::errors::SessionError,
    },

    #[error("IO error during cleanup: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },

    #[error("Cleanup strategy '{strategy}' failed: {source}")]
    StrategyFailed {
        strategy: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl ShardsError for CleanupError {
    fn error_code(&self) -> &'static str {
        match self {
            CleanupError::NotInRepository => "CLEANUP_NOT_IN_REPOSITORY",
            CleanupError::NoOrphanedResources => "CLEANUP_NO_ORPHANED_RESOURCES",
            CleanupError::BranchScanFailed { .. } => "CLEANUP_BRANCH_SCAN_FAILED",
            CleanupError::WorktreeScanFailed { .. } => "CLEANUP_WORKTREE_SCAN_FAILED",
            CleanupError::CleanupFailed { .. } => "CLEANUP_OPERATION_FAILED",
            CleanupError::PermissionDenied { .. } => "CLEANUP_PERMISSION_DENIED",
            CleanupError::GitError { .. } => "CLEANUP_GIT_ERROR",
            CleanupError::SessionError { .. } => "CLEANUP_SESSION_ERROR",
            CleanupError::IoError { .. } => "CLEANUP_IO_ERROR",
            CleanupError::StrategyFailed { .. } => "CLEANUP_STRATEGY_FAILED",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            CleanupError::NotInRepository
                | CleanupError::NoOrphanedResources
                | CleanupError::PermissionDenied { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cleanup_error_display() {
        let error = CleanupError::NotInRepository;
        assert_eq!(error.to_string(), "Not in a git repository");
        assert_eq!(error.error_code(), "CLEANUP_NOT_IN_REPOSITORY");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_cleanup_failed_error() {
        let error = CleanupError::CleanupFailed {
            name: "test-branch".to_string(),
            message: "Branch is locked".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Failed to cleanup resource 'test-branch': Branch is locked"
        );
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_permission_denied_error() {
        let error = CleanupError::PermissionDenied {
            path: "/tmp/test".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Permission denied during cleanup: /tmp/test"
        );
        assert!(error.is_user_error());
    }
}
