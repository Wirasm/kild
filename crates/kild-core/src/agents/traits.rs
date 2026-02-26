//! Agent backend trait definition.

/// Trait defining the interface for agent backends.
///
/// Each supported agent (Claude, Kiro, Gemini, etc.) implements this trait
/// to provide agent-specific behavior like command construction and process detection.
pub trait AgentBackend: Send + Sync {
    /// The canonical name of this agent (e.g., "claude", "kiro").
    fn name(&self) -> &'static str;

    /// The display name for this agent (e.g., "Claude Code", "Kiro CLI").
    fn display_name(&self) -> &'static str;

    /// Check if this agent's CLI is installed and available in PATH.
    fn is_available(&self) -> bool;

    /// Get the default command to launch this agent.
    fn default_command(&self) -> &'static str;

    /// Get process name patterns for detection.
    ///
    /// Returns patterns that can match against process names when detecting
    /// running agent instances. Handles quirks like Claude showing version
    /// as process name.
    fn process_patterns(&self) -> Vec<String>;

    /// Returns the CLI flags for "yolo mode" (full autonomy, skip all permission prompts).
    /// Returns `None` if the agent doesn't support autonomous mode.
    fn yolo_flags(&self) -> Option<&'static str> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockBackend;

    impl AgentBackend for MockBackend {
        fn name(&self) -> &'static str {
            "mock"
        }

        fn display_name(&self) -> &'static str {
            "Mock Agent"
        }

        fn is_available(&self) -> bool {
            true
        }

        fn default_command(&self) -> &'static str {
            "mock-cli"
        }

        fn process_patterns(&self) -> Vec<String> {
            vec!["mock".to_string(), "mock-cli".to_string()]
        }
    }

    #[test]
    fn mock_agent_backend_delegates_name_display_and_yolo_correctly() {
        let backend = MockBackend;
        assert_eq!(backend.name(), "mock");
        assert_eq!(backend.display_name(), "Mock Agent");
        assert!(backend.is_available());
        assert_eq!(backend.default_command(), "mock-cli");
        assert_eq!(backend.yolo_flags(), None);
    }
}
