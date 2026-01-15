use crate::core::errors::ShardsError;

#[derive(Debug, thiserror::Error)]
pub enum HealthError {
    #[error("Failed to gather health metrics: {message}")]
    MetricsGatherFailed { message: String },
    
    #[error("Session error: {source}")]
    SessionError {
        #[from]
        source: crate::sessions::errors::SessionError,
    },
    
    #[error("Process error: {source}")]
    ProcessError {
        #[from]
        source: crate::process::errors::ProcessError,
    },
    
    #[error("IO operation failed: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
}

impl ShardsError for HealthError {
    fn error_code(&self) -> &'static str {
        match self {
            HealthError::MetricsGatherFailed { .. } => "HEALTH_METRICS_FAILED",
            HealthError::SessionError { .. } => "HEALTH_SESSION_ERROR",
            HealthError::ProcessError { .. } => "HEALTH_PROCESS_ERROR",
            HealthError::IoError { .. } => "HEALTH_IO_ERROR",
        }
    }
    
    fn is_user_error(&self) -> bool {
        false
    }
}
