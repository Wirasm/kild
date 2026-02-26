use crate::errors::KildError;

/// Note: This type intentionally does not implement `Clone` because
/// `io::Error` (in `CanonicalizationFailed`) and `GitError` (in `GitCheckFailed`)
/// are not `Clone`.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("Path is not a directory")]
    NotADirectory,

    #[error("Path is not a git repository")]
    NotAGitRepo,

    #[error("Git repository check failed: {source}")]
    GitCheckFailed {
        #[from]
        source: crate::git::GitError,
    },

    #[error("Cannot resolve path: {source}")]
    CanonicalizationFailed { source: std::io::Error },

    #[error("Project not found")]
    NotFound,

    #[error("Project already exists")]
    AlreadyExists,

    #[error("Failed to save projects: {message}")]
    SaveFailed { message: String },

    #[error("Projects file corrupted: {message}")]
    LoadCorrupted { message: String },
}

impl KildError for ProjectError {
    fn error_code(&self) -> &'static str {
        match self {
            ProjectError::NotADirectory => "PROJECT_NOT_A_DIRECTORY",
            ProjectError::NotAGitRepo => "PROJECT_NOT_GIT_REPO",
            ProjectError::GitCheckFailed { .. } => "PROJECT_GIT_CHECK_FAILED",
            ProjectError::CanonicalizationFailed { .. } => "PROJECT_CANONICALIZATION_FAILED",
            ProjectError::NotFound => "PROJECT_NOT_FOUND",
            ProjectError::AlreadyExists => "PROJECT_ALREADY_EXISTS",
            ProjectError::SaveFailed { .. } => "PROJECT_SAVE_FAILED",
            ProjectError::LoadCorrupted { .. } => "PROJECT_LOAD_CORRUPTED",
        }
    }

    fn is_user_error(&self) -> bool {
        // Exhaustive match ensures new variants force an explicit classification.
        match self {
            ProjectError::NotADirectory
            | ProjectError::NotAGitRepo
            | ProjectError::CanonicalizationFailed { .. }
            | ProjectError::NotFound
            | ProjectError::AlreadyExists => true,

            ProjectError::GitCheckFailed { .. }
            | ProjectError::SaveFailed { .. }
            | ProjectError::LoadCorrupted { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_error_display() {
        let error = ProjectError::NotADirectory;
        assert_eq!(error.to_string(), "Path is not a directory");
        assert_eq!(error.error_code(), "PROJECT_NOT_A_DIRECTORY");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_project_error_not_git_repo() {
        let error = ProjectError::NotAGitRepo;
        assert_eq!(error.to_string(), "Path is not a git repository");
        assert_eq!(error.error_code(), "PROJECT_NOT_GIT_REPO");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_project_error_git_check_failed() {
        let error = ProjectError::GitCheckFailed {
            source: crate::git::GitError::OperationFailed {
                message: "permission denied".to_string(),
            },
        };
        assert!(error.to_string().contains("permission denied"));
        assert_eq!(error.error_code(), "PROJECT_GIT_CHECK_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_project_error_not_found() {
        let error = ProjectError::NotFound;
        assert_eq!(error.to_string(), "Project not found");
        assert_eq!(error.error_code(), "PROJECT_NOT_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_project_error_already_exists() {
        let error = ProjectError::AlreadyExists;
        assert_eq!(error.to_string(), "Project already exists");
        assert_eq!(error.error_code(), "PROJECT_ALREADY_EXISTS");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_project_error_save_failed() {
        let error = ProjectError::SaveFailed {
            message: "disk full".to_string(),
        };
        assert_eq!(error.to_string(), "Failed to save projects: disk full");
        assert_eq!(error.error_code(), "PROJECT_SAVE_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_project_error_load_corrupted() {
        let error = ProjectError::LoadCorrupted {
            message: "invalid JSON".to_string(),
        };
        assert_eq!(error.to_string(), "Projects file corrupted: invalid JSON");
        assert_eq!(error.error_code(), "PROJECT_LOAD_CORRUPTED");
        assert!(!error.is_user_error());
    }
}
