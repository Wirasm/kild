use crate::errors::KildError;
use crate::terminal::errors::TerminalError;

#[derive(Debug, thiserror::Error)]
pub enum EditorError {
    #[error("No supported editor found")]
    NoEditorFound,

    #[error("Editor '{editor}' not found or not executable")]
    EditorNotFound { editor: String },

    #[error("Failed to spawn editor process: {message}")]
    SpawnFailed { message: String },

    #[error("Failed to spawn terminal for editor: {source}")]
    TerminalSpawnFailed {
        #[from]
        source: TerminalError,
    },

    #[error("IO error during editor operation: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
}

impl KildError for EditorError {
    fn error_code(&self) -> &'static str {
        match self {
            EditorError::NoEditorFound => "EDITOR_NOT_FOUND",
            EditorError::EditorNotFound { .. } => "EDITOR_COMMAND_NOT_FOUND",
            EditorError::SpawnFailed { .. } => "EDITOR_SPAWN_FAILED",
            EditorError::TerminalSpawnFailed { .. } => "EDITOR_TERMINAL_SPAWN_FAILED",
            EditorError::IoError { .. } => "EDITOR_IO_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            EditorError::NoEditorFound | EditorError::EditorNotFound { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_editor_found() {
        let error = EditorError::NoEditorFound;
        assert_eq!(error.to_string(), "No supported editor found");
        assert_eq!(error.error_code(), "EDITOR_NOT_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_editor_not_found() {
        let error = EditorError::EditorNotFound {
            editor: "zed".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Editor 'zed' not found or not executable"
        );
        assert_eq!(error.error_code(), "EDITOR_COMMAND_NOT_FOUND");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_spawn_failed() {
        let error = EditorError::SpawnFailed {
            message: "process exited".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Failed to spawn editor process: process exited"
        );
        assert_eq!(error.error_code(), "EDITOR_SPAWN_FAILED");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_io_error() {
        let error = EditorError::IoError {
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
        };
        assert!(error.to_string().contains("IO error"));
        assert_eq!(error.error_code(), "EDITOR_IO_ERROR");
        assert!(!error.is_user_error());
    }

    #[test]
    fn test_error_codes_are_unique() {
        use std::collections::HashSet;
        let errors: Vec<&str> = vec![
            EditorError::NoEditorFound.error_code(),
            EditorError::EditorNotFound {
                editor: "t".to_string(),
            }
            .error_code(),
            EditorError::SpawnFailed {
                message: "t".to_string(),
            }
            .error_code(),
            EditorError::TerminalSpawnFailed {
                source: crate::terminal::errors::TerminalError::NoTerminalFound,
            }
            .error_code(),
            EditorError::IoError {
                source: std::io::Error::new(std::io::ErrorKind::Other, "t"),
            }
            .error_code(),
        ];
        let unique: HashSet<_> = errors.iter().collect();
        assert_eq!(unique.len(), errors.len(), "Error codes must be unique");
    }
}
