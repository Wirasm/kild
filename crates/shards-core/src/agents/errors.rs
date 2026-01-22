//! Agent-specific error types.

use crate::errors::ShardsError;

/// Errors that can occur during agent operations.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Unknown agent '{name}'. Supported: claude, kiro, gemini, codex, aether")]
    UnknownAgent { name: String },

    #[error("Agent '{name}' CLI is not installed or not in PATH")]
    AgentNotAvailable { name: String },
}

impl ShardsError for AgentError {
    fn error_code(&self) -> &'static str {
        match self {
            AgentError::UnknownAgent { .. } => "UNKNOWN_AGENT",
            AgentError::AgentNotAvailable { .. } => "AGENT_NOT_AVAILABLE",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(
            self,
            AgentError::UnknownAgent { .. } | AgentError::AgentNotAvailable { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unknown_agent_error_display() {
        let error = AgentError::UnknownAgent {
            name: "unknown".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Unknown agent 'unknown'. Supported: claude, kiro, gemini, codex, aether"
        );
        assert_eq!(error.error_code(), "UNKNOWN_AGENT");
        assert!(error.is_user_error());
    }

    #[test]
    fn test_agent_not_available_error_display() {
        let error = AgentError::AgentNotAvailable {
            name: "claude".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Agent 'claude' CLI is not installed or not in PATH"
        );
        assert_eq!(error.error_code(), "AGENT_NOT_AVAILABLE");
        assert!(error.is_user_error());
    }
}
