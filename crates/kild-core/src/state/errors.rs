use crate::errors::KildError;
use crate::projects::errors::ProjectError;
use crate::sessions::errors::SessionError;

#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error("Config error: {0}")]
    Config(String),
}

impl KildError for DispatchError {
    fn error_code(&self) -> &'static str {
        match self {
            DispatchError::Session(e) => e.error_code(),
            DispatchError::Project(e) => e.error_code(),
            DispatchError::Config(_) => "DISPATCH_CONFIG_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        match self {
            DispatchError::Session(e) => e.is_user_error(),
            DispatchError::Project(e) => e.is_user_error(),
            DispatchError::Config(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatch_error_from_session_error() {
        let session_err = SessionError::NotFound {
            name: "test".to_string(),
        };
        let dispatch_err = DispatchError::from(session_err);
        assert_eq!(dispatch_err.error_code(), "SESSION_NOT_FOUND");
        assert!(dispatch_err.is_user_error());
        assert_eq!(dispatch_err.to_string(), "Session 'test' not found");
    }

    #[test]
    fn test_dispatch_error_from_project_error() {
        let project_err = ProjectError::NotFound;
        let dispatch_err = DispatchError::from(project_err);
        assert_eq!(dispatch_err.error_code(), "PROJECT_NOT_FOUND");
        assert!(dispatch_err.is_user_error());
        assert_eq!(dispatch_err.to_string(), "Project not found");
    }

    #[test]
    fn test_dispatch_error_config() {
        let err = DispatchError::Config("bad config".to_string());
        assert_eq!(err.error_code(), "DISPATCH_CONFIG_ERROR");
        assert!(err.is_user_error());
        assert_eq!(err.to_string(), "Config error: bad config");
    }

    #[test]
    fn test_dispatch_error_session_delegates_error_code() {
        let err = DispatchError::Session(SessionError::AlreadyExists {
            name: "feature".to_string(),
        });
        assert_eq!(err.error_code(), "SESSION_ALREADY_EXISTS");
    }

    #[test]
    fn test_dispatch_error_session_delegates_is_user_error() {
        // IoError is NOT a user error in SessionError
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = DispatchError::Session(SessionError::IoError { source: io_err });
        assert!(!err.is_user_error());
    }

    #[test]
    fn test_dispatch_error_project_delegates_is_user_error() {
        // GitCommandFailed is NOT a user error in ProjectError
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "git not found");
        let err = DispatchError::Project(ProjectError::GitCommandFailed { source: io_err });
        assert!(!err.is_user_error());
    }
}
