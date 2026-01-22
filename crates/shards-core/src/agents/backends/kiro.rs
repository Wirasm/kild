//! Kiro CLI agent backend implementation.

use crate::agents::traits::AgentBackend;

/// Backend implementation for Kiro CLI.
pub struct KiroBackend;

impl AgentBackend for KiroBackend {
    fn name(&self) -> &'static str {
        "kiro"
    }

    fn display_name(&self) -> &'static str {
        "Kiro CLI"
    }

    fn is_available(&self) -> bool {
        which::which("kiro-cli").is_ok()
    }

    fn default_command(&self) -> &'static str {
        "kiro-cli chat"
    }

    fn process_patterns(&self) -> Vec<String> {
        vec!["kiro-cli".to_string(), "kiro".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kiro_backend_name() {
        let backend = KiroBackend;
        assert_eq!(backend.name(), "kiro");
    }

    #[test]
    fn test_kiro_backend_display_name() {
        let backend = KiroBackend;
        assert_eq!(backend.display_name(), "Kiro CLI");
    }

    #[test]
    fn test_kiro_backend_default_command() {
        let backend = KiroBackend;
        assert_eq!(backend.default_command(), "kiro-cli chat");
    }

    #[test]
    fn test_kiro_backend_process_patterns() {
        let backend = KiroBackend;
        let patterns = backend.process_patterns();
        assert!(patterns.contains(&"kiro-cli".to_string()));
        assert!(patterns.contains(&"kiro".to_string()));
    }
}
