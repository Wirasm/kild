//! Aether CLI agent backend implementation.

use crate::agents::traits::AgentBackend;

/// Backend implementation for Aether CLI.
pub struct AetherBackend;

impl AgentBackend for AetherBackend {
    fn name(&self) -> &'static str {
        "aether"
    }

    fn display_name(&self) -> &'static str {
        "Aether CLI"
    }

    fn is_available(&self) -> bool {
        which::which("aether").is_ok()
    }

    fn default_command(&self) -> &'static str {
        "aether"
    }

    fn process_patterns(&self) -> Vec<String> {
        vec!["aether".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aether_backend_name() {
        let backend = AetherBackend;
        assert_eq!(backend.name(), "aether");
    }

    #[test]
    fn test_aether_backend_display_name() {
        let backend = AetherBackend;
        assert_eq!(backend.display_name(), "Aether CLI");
    }

    #[test]
    fn test_aether_backend_default_command() {
        let backend = AetherBackend;
        assert_eq!(backend.default_command(), "aether");
    }

    #[test]
    fn test_aether_backend_process_patterns() {
        let backend = AetherBackend;
        let patterns = backend.process_patterns();
        assert!(patterns.contains(&"aether".to_string()));
    }
}
