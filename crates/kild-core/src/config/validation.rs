//! Configuration validation logic.
//!
//! This module contains validation functions for configuration values,
//! ensuring they are valid before being used by the application.

use crate::agents;
use crate::config::types::KildConfig;
use crate::errors::ConfigError;

/// Valid terminal emulator names.
pub const VALID_TERMINALS: [&str; 5] = ["iterm2", "iterm", "terminal", "ghostty", "native"];

/// Validate a KildConfig, returning an error if any values are invalid.
///
/// # Validation Rules
///
/// - Agent name must be a known agent (see [`AgentType`](crate::agents::AgentType) for supported agents)
/// - Terminal preference, if set, should be a valid terminal name (warning only)
/// - Include patterns, if configured, must be valid
///
/// # Errors
///
/// Returns `ConfigError::InvalidAgent` if the default agent is not recognized.
/// Returns `ConfigError::InvalidConfiguration` if include patterns are invalid.
pub fn validate_config(config: &KildConfig) -> Result<(), ConfigError> {
    // Validate agent name
    if !agents::is_valid_agent(&config.agent.default) {
        return Err(ConfigError::InvalidAgent {
            agent: config.agent.default.clone(),
        });
    }

    // Validate terminal preference if set
    if let Some(ref terminal) = config.terminal.preferred
        && !VALID_TERMINALS.contains(&terminal.as_str())
    {
        return Err(ConfigError::InvalidConfiguration {
            message: format!(
                "Invalid terminal '{}'. Valid options: {}",
                terminal,
                VALID_TERMINALS.join(", ")
            ),
        });
    }

    // Validate include patterns if configured
    if let Some(ref include_config) = config.include_patterns
        && let Err(e) = include_config.validate()
    {
        return Err(ConfigError::InvalidConfiguration {
            message: format!("Invalid include patterns: {}", e),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{AgentConfig, KildConfig};

    #[test]
    fn test_config_validation_valid_agent() {
        let config = KildConfig::default(); // Uses "claude" which is valid
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_config_validation_invalid_agent() {
        let mut config = KildConfig::default();
        config.agent.default = "invalid-agent".to_string();

        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::InvalidAgent { .. }
        ));
    }

    #[test]
    fn test_config_validation_all_valid_agents() {
        let valid_agents = ["amp", "claude", "kiro", "gemini", "codex", "opencode"];
        for agent in valid_agents {
            let mut config = KildConfig::default();
            config.agent = AgentConfig {
                default: agent.to_string(),
                startup_command: None,
                flags: None,
            };
            assert!(
                validate_config(&config).is_ok(),
                "Agent '{}' should be valid",
                agent
            );
        }
    }

    #[test]
    fn test_valid_terminals_constant() {
        assert!(VALID_TERMINALS.contains(&"iterm2"));
        assert!(VALID_TERMINALS.contains(&"iterm"));
        assert!(VALID_TERMINALS.contains(&"terminal"));
        assert!(VALID_TERMINALS.contains(&"ghostty"));
        assert!(VALID_TERMINALS.contains(&"native"));
        assert!(!VALID_TERMINALS.contains(&"invalid"));
    }

    #[test]
    fn test_config_validation_invalid_terminal() {
        let mut config = KildConfig::default();
        config.terminal.preferred = Some("unknown-terminal".to_string());

        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::InvalidConfiguration { .. }
        ));
    }

    #[test]
    fn test_config_validation_valid_terminal() {
        let mut config = KildConfig::default();
        config.terminal.preferred = Some("ghostty".to_string());

        assert!(validate_config(&config).is_ok());
    }
}
