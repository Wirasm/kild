use crate::errors::KildError;
use crate::projects::errors::ProjectError;
use crate::sessions::errors::SessionError;
use kild_config::ConfigError;

#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("Command not implemented: {0}")]
    NotImplemented(String),
}

impl KildError for DispatchError {
    fn error_code(&self) -> &'static str {
        match self {
            DispatchError::Session(e) => e.error_code(),
            DispatchError::Project(e) => e.error_code(),
            DispatchError::Config(e) => e.error_code(),
            DispatchError::NotImplemented(_) => "DISPATCH_NOT_IMPLEMENTED",
        }
    }

    fn is_user_error(&self) -> bool {
        match self {
            DispatchError::Session(e) => e.is_user_error(),
            DispatchError::Project(e) => e.is_user_error(),
            DispatchError::Config(e) => e.is_user_error(),
            DispatchError::NotImplemented(_) => false,
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
    fn test_dispatch_error_from_config_error() {
        let config_err = kild_config::ConfigError::ConfigParseError {
            message: "invalid TOML".to_string(),
        };
        let dispatch_err = DispatchError::from(config_err);
        assert_eq!(dispatch_err.error_code(), "CONFIG_PARSE_ERROR");
        assert!(dispatch_err.is_user_error());
        assert_eq!(
            dispatch_err.to_string(),
            "Failed to parse config file: invalid TOML"
        );
    }

    #[test]
    fn test_dispatch_error_config_delegates_error_code() {
        let err = DispatchError::Config(kild_config::ConfigError::InvalidAgent {
            agent: "bad".to_string(),
            supported_agents: crate::agents::supported_agents_string(),
        });
        assert_eq!(err.error_code(), "INVALID_AGENT");
    }

    #[test]
    fn test_dispatch_error_config_io_is_not_user_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = DispatchError::Config(kild_config::ConfigError::IoError { source: io_err });
        assert!(!err.is_user_error());
    }

    #[test]
    fn test_dispatch_error_not_implemented() {
        let err = DispatchError::NotImplemented("AddProject".to_string());
        assert_eq!(err.error_code(), "DISPATCH_NOT_IMPLEMENTED");
        assert!(!err.is_user_error());
        assert_eq!(err.to_string(), "Command not implemented: AddProject");
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
        // GitCheckFailed is NOT a user error in ProjectError
        let err = DispatchError::Project(ProjectError::GitCheckFailed {
            source: crate::git::GitError::OperationFailed {
                message: "permission denied".to_string(),
            },
        });
        assert!(!err.is_user_error());
    }
}
