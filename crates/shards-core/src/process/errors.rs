use crate::errors::ShardsError;

#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("Process '{pid}' not found")]
    NotFound { pid: u32 },

    #[error("Failed to kill process '{pid}': {message}")]
    KillFailed { pid: u32, message: String },

    #[error("Access denied for process '{pid}'")]
    AccessDenied { pid: u32 },

    #[error("System error: {message}")]
    SystemError { message: String },

    #[error("Invalid PID: {pid}")]
    InvalidPid { pid: u32 },

    #[error("PID '{pid}' has been reused (expected: {expected}, actual: {actual})")]
    PidReused {
        pid: u32,
        expected: String,
        actual: String,
    },

    #[error("PID file error at '{path}': {message}")]
    PidFileError {
        path: std::path::PathBuf,
        message: String,
    },
}

impl ShardsError for ProcessError {
    fn error_code(&self) -> &'static str {
        match self {
            ProcessError::NotFound { .. } => "PROCESS_NOT_FOUND",
            ProcessError::KillFailed { .. } => "PROCESS_KILL_FAILED",
            ProcessError::AccessDenied { .. } => "PROCESS_ACCESS_DENIED",
            ProcessError::SystemError { .. } => "PROCESS_SYSTEM_ERROR",
            ProcessError::InvalidPid { .. } => "PROCESS_INVALID_PID",
            ProcessError::PidReused { .. } => "PROCESS_PID_REUSED",
            ProcessError::PidFileError { .. } => "PROCESS_PID_FILE_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            ProcessError::NotFound { .. }
                | ProcessError::AccessDenied { .. }
                | ProcessError::InvalidPid { .. }
        )
    }
}
