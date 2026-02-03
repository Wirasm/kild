//! OpenCode CLI agent backend implementation.

use crate::agents::traits::AgentBackend;

/// Backend implementation for OpenCode CLI.
pub struct OpenCodeBackend;

impl AgentBackend for OpenCodeBackend {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn display_name(&self) -> &'static str {
        "OpenCode"
    }

    fn is_available(&self) -> bool {
        which::which("opencode").is_ok()
    }

    fn default_command(&self) -> &'static str {
        "opencode"
    }

    fn process_patterns(&self) -> Vec<String> {
        vec!["opencode".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opencode_backend_name() {
        let backend = OpenCodeBackend;
        assert_eq!(backend.name(), "opencode");
    }

    #[test]
    fn test_opencode_backend_display_name() {
        let backend = OpenCodeBackend;
        assert_eq!(backend.display_name(), "OpenCode");
    }

    #[test]
    fn test_opencode_backend_default_command() {
        let backend = OpenCodeBackend;
        assert_eq!(backend.default_command(), "opencode");
    }

    #[test]
    fn test_opencode_backend_process_patterns() {
        let backend = OpenCodeBackend;
        let patterns = backend.process_patterns();
        assert!(patterns.contains(&"opencode".to_string()));
    }
}
