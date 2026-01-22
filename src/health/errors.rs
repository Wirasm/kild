use crate::core::errors::ShardsError;

#[derive(Debug, thiserror::Error)]
pub enum HealthError {
    #[error("Health metrics collection failed: {message}")]
    MetricsGatherFailed { message: String },

    #[error("Health check failed due to session error: {source}")]
    SessionError {
        #[from]
        source: crate::sessions::errors::SessionError,
    },

    #[error("Health check failed due to process error: {source}")]
    ProcessError {
        #[from]
        source: crate::process::errors::ProcessError,
    },

    #[error("Health check failed due to I/O error: {source}")]
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
