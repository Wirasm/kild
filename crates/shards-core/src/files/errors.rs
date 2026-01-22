use crate::errors::ShardsError;

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("Invalid glob pattern '{pattern}': {message}")]
    InvalidPattern { pattern: String, message: String },

    #[error("File copy failed from '{source_path}' to '{dest_path}': {error_msg}")]
    CopyFailed {
        source_path: String,
        dest_path: String,
        error_msg: String,
    },

    #[error("File not found: '{path}'")]
    FileNotFound { path: String },

    #[error("Permission denied accessing '{path}'")]
    PermissionDenied { path: String },

    #[error("File too large: '{path}' ({size} bytes exceeds limit)")]
    FileTooLarge { path: String, size: u64 },

    #[error("IO error during file operation: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },

    #[error("Pattern validation failed: {message}")]
    ValidationError { message: String },
}

impl ShardsError for FileError {
    fn error_code(&self) -> &'static str {
        match self {
            FileError::InvalidPattern { .. } => "FILE_INVALID_PATTERN",
            FileError::CopyFailed { .. } => "FILE_COPY_FAILED",
            FileError::FileNotFound { .. } => "FILE_NOT_FOUND",
            FileError::PermissionDenied { .. } => "FILE_PERMISSION_DENIED",
            FileError::FileTooLarge { .. } => "FILE_TOO_LARGE",
            FileError::IoError { .. } => "FILE_IO_ERROR",
            FileError::ValidationError { .. } => "FILE_VALIDATION_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            FileError::InvalidPattern { .. }
                | FileError::ValidationError { .. }
                | FileError::FileTooLarge { .. }
        )
    }
}
