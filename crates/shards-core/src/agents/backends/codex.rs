//! Codex CLI agent backend implementation.

use crate::agents::traits::AgentBackend;

/// Backend implementation for OpenAI Codex CLI.
pub struct CodexBackend;

impl AgentBackend for CodexBackend {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn display_name(&self) -> &'static str {
        "Codex CLI"
    }

    fn is_available(&self) -> bool {
        which::which("codex").is_ok()
    }

    fn default_command(&self) -> &'static str {
        "codex"
    }

    fn process_patterns(&self) -> Vec<String> {
        vec!["codex".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codex_backend_name() {
        let backend = CodexBackend;
        assert_eq!(backend.name(), "codex");
    }

    #[test]
    fn test_codex_backend_display_name() {
        let backend = CodexBackend;
        assert_eq!(backend.display_name(), "Codex CLI");
    }

    #[test]
    fn test_codex_backend_default_command() {
        let backend = CodexBackend;
        assert_eq!(backend.default_command(), "codex");
    }

    #[test]
    fn test_codex_backend_process_patterns() {
        let backend = CodexBackend;
        let patterns = backend.process_patterns();
        assert!(patterns.contains(&"codex".to_string()));
    }
}
