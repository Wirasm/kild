//! Gemini CLI agent backend implementation.

use crate::agents::traits::AgentBackend;

/// Backend implementation for Gemini CLI.
pub struct GeminiBackend;

impl AgentBackend for GeminiBackend {
    fn name(&self) -> &'static str {
        "gemini"
    }

    fn display_name(&self) -> &'static str {
        "Gemini CLI"
    }

    fn is_available(&self) -> bool {
        which::which("gemini").is_ok()
    }

    fn default_command(&self) -> &'static str {
        "gemini"
    }

    fn process_patterns(&self) -> Vec<String> {
        vec!["gemini".to_string(), "gemini-cli".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_backend_name() {
        let backend = GeminiBackend;
        assert_eq!(backend.name(), "gemini");
    }

    #[test]
    fn test_gemini_backend_display_name() {
        let backend = GeminiBackend;
        assert_eq!(backend.display_name(), "Gemini CLI");
    }

    #[test]
    fn test_gemini_backend_default_command() {
        let backend = GeminiBackend;
        assert_eq!(backend.default_command(), "gemini");
    }

    #[test]
    fn test_gemini_backend_process_patterns() {
        let backend = GeminiBackend;
        let patterns = backend.process_patterns();
        assert!(patterns.contains(&"gemini".to_string()));
        assert!(patterns.contains(&"gemini-cli".to_string()));
    }
}
