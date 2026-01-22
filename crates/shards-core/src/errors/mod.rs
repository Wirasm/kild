use std::error::Error;

/// Base trait for all application errors
pub trait ShardsError: Error + Send + Sync + 'static {
    /// Error code for programmatic handling
    fn error_code(&self) -> &'static str;

    /// Whether this error should be logged as an error or warning
    fn is_user_error(&self) -> bool {
        false
    }
}

/// Common result type for the application
pub type ShardsResult<T> = Result<T, Box<dyn ShardsError>>;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Config file not found at '{path}'")]
    ConfigNotFound { path: String },

    #[error("Failed to parse config file: {message}")]
    ConfigParseError { message: String },

    #[error("Invalid agent '{agent}'. Supported agents: claude, kiro, gemini, codex, aether")]
    InvalidAgent { agent: String },

    #[error("Invalid configuration: {message}")]
    InvalidConfiguration { message: String },

    #[error("IO error reading config: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
}

impl ShardsError for ConfigError {
    fn error_code(&self) -> &'static str {
        match self {
            ConfigError::ConfigNotFound { .. } => "CONFIG_NOT_FOUND",
            ConfigError::ConfigParseError { .. } => "CONFIG_PARSE_ERROR",
            ConfigError::InvalidAgent { .. } => "INVALID_AGENT",
            ConfigError::InvalidConfiguration { .. } => "INVALID_CONFIGURATION",
            ConfigError::IoError { .. } => "CONFIG_IO_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            ConfigError::ConfigParseError { .. }
                | ConfigError::InvalidAgent { .. }
                | ConfigError::InvalidConfiguration { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shards_result() {
        let _result: ShardsResult<i32> = Ok(42);
    }

    #[test]
    fn test_config_error_display() {
        let error = ConfigError::InvalidAgent {
            agent: "unknown".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Invalid agent 'unknown'. Supported agents: claude, kiro, gemini, codex, aether"
        );
        assert_eq!(error.error_code(), "INVALID_AGENT");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_config_parse_error() {
        let error = ConfigError::ConfigParseError {
            message: "invalid TOML syntax".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Failed to parse config file: invalid TOML syntax"
        );
        assert_eq!(error.error_code(), "CONFIG_PARSE_ERROR");
        assert!(error.is_user_error());
    }
}
