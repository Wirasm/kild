//! Claude Code agent backend implementation.

use crate::agents::traits::AgentBackend;

/// Backend implementation for Claude Code.
pub struct ClaudeBackend;

impl AgentBackend for ClaudeBackend {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn is_available(&self) -> bool {
        which::which("claude").is_ok()
    }

    fn default_command(&self) -> &'static str {
        "claude"
    }

    fn process_patterns(&self) -> Vec<String> {
        vec!["claude".to_string(), "claude-code".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_backend_name() {
        let backend = ClaudeBackend;
        assert_eq!(backend.name(), "claude");
    }

    #[test]
    fn test_claude_backend_display_name() {
        let backend = ClaudeBackend;
        assert_eq!(backend.display_name(), "Claude Code");
    }

    #[test]
    fn test_claude_backend_default_command() {
        let backend = ClaudeBackend;
        assert_eq!(backend.default_command(), "claude");
    }

    #[test]
    fn test_claude_backend_process_patterns() {
        let backend = ClaudeBackend;
        let patterns = backend.process_patterns();
        assert!(patterns.contains(&"claude".to_string()));
        assert!(patterns.contains(&"claude-code".to_string()));
    }

    #[test]
    fn test_claude_backend_command_patterns() {
        let backend = ClaudeBackend;
        let patterns = backend.command_patterns();
        assert_eq!(patterns, vec!["claude".to_string()]);
    }
}
