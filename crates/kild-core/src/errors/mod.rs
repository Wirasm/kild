use std::error::Error;

// Re-export ConfigError from kild-config for backward compatibility
pub use kild_config::ConfigError;

/// Base trait for all application errors
pub trait KildError: Error + Send + Sync + 'static {
    /// Error code for programmatic handling
    fn error_code(&self) -> &'static str;

    /// Whether this error should be logged as an error or warning
    fn is_user_error(&self) -> bool {
        false
    }
}

/// Common result type for the application
pub type KildResult<T> = Result<T, Box<dyn KildError>>;

impl KildError for ConfigError {
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
    fn test_kild_result() {
        let _result: KildResult<i32> = Ok(42);
    }

    #[test]
    fn test_config_error_display() {
        use crate::agents::supported_agents_string;
        let error = ConfigError::InvalidAgent {
            agent: "unknown".to_string(),
            supported_agents: supported_agents_string(),
        };
        let msg = error.to_string();
        // Verify message format
        assert!(msg.starts_with("Invalid agent 'unknown'. Supported agents: "));
        // Verify all valid agents are listed
        assert!(msg.contains("amp"), "Error should list amp");
        assert!(msg.contains("claude"), "Error should list claude");
        assert!(msg.contains("kiro"), "Error should list kiro");
        assert!(msg.contains("gemini"), "Error should list gemini");
        assert!(msg.contains("codex"), "Error should list codex");
        // Verify removed agents are NOT listed
        assert!(
            !msg.contains("aether"),
            "Error should NOT list removed agent aether"
        );
        // Verify error trait methods
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
