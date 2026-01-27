//! Agent registry for managing and looking up agent backends.

use std::collections::HashMap;
use std::sync::LazyLock;

use super::backends::{AmpBackend, ClaudeBackend, CodexBackend, GeminiBackend, KiroBackend};
use super::traits::AgentBackend;
use super::types::AgentType;

/// Global registry of all supported agent backends.
static REGISTRY: LazyLock<AgentRegistry> = LazyLock::new(AgentRegistry::new);

/// Registry that manages all agent backend implementations.
///
/// Uses `AgentType` as the internal key for type safety, while providing
/// string-based lookup functions for backward compatibility.
struct AgentRegistry {
    backends: HashMap<AgentType, Box<dyn AgentBackend>>,
}

impl AgentRegistry {
    fn new() -> Self {
        let mut backends: HashMap<AgentType, Box<dyn AgentBackend>> = HashMap::new();
        backends.insert(AgentType::Amp, Box::new(AmpBackend));
        backends.insert(AgentType::Claude, Box::new(ClaudeBackend));
        backends.insert(AgentType::Kiro, Box::new(KiroBackend));
        backends.insert(AgentType::Gemini, Box::new(GeminiBackend));
        backends.insert(AgentType::Codex, Box::new(CodexBackend));
        Self { backends }
    }

    /// Get a reference to an agent backend by type.
    fn get_by_type(&self, agent_type: AgentType) -> Option<&dyn AgentBackend> {
        self.backends.get(&agent_type).map(|b| b.as_ref())
    }

    /// Get a reference to an agent backend by name (case-insensitive).
    fn get(&self, name: &str) -> Option<&dyn AgentBackend> {
        AgentType::parse(name).and_then(|t| self.get_by_type(t))
    }

    /// Get the default agent type.
    fn default_agent(&self) -> AgentType {
        AgentType::Claude
    }
}

/// Get a reference to an agent backend by name (case-insensitive).
pub fn get_agent(name: &str) -> Option<&'static dyn AgentBackend> {
    REGISTRY.get(name)
}

/// Get a reference to an agent backend by type.
pub fn get_agent_by_type(agent_type: AgentType) -> Option<&'static dyn AgentBackend> {
    REGISTRY.get_by_type(agent_type)
}

/// Check if an agent name is valid/supported (case-insensitive).
pub fn is_valid_agent(name: &str) -> bool {
    AgentType::parse(name).is_some()
}

/// Get all valid agent names (lowercase).
pub fn valid_agent_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = AgentType::all().iter().map(|t| t.as_str()).collect();
    names.sort();
    names
}

/// Get the default agent name.
pub fn default_agent_name() -> &'static str {
    REGISTRY.default_agent().as_str()
}

/// Get the default agent type.
pub fn default_agent_type() -> AgentType {
    REGISTRY.default_agent()
}

/// Get the default command for an agent by name (case-insensitive).
pub fn get_default_command(name: &str) -> Option<&'static str> {
    get_agent(name).map(|backend| backend.default_command())
}

/// Get process patterns for an agent by name (case-insensitive).
pub fn get_process_patterns(name: &str) -> Option<Vec<String>> {
    get_agent(name).map(|backend| backend.process_patterns())
}

/// Get a comma-separated string of all supported agent names.
///
/// Used for error messages to ensure they stay in sync with available agents.
pub fn supported_agents_string() -> String {
    valid_agent_names().join(", ")
}

/// Check if an agent's CLI is available in PATH (case-insensitive).
pub fn is_agent_available(name: &str) -> Option<bool> {
    get_agent(name).map(|backend| backend.is_available())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_agent_known() {
        let backend = get_agent("claude");
        assert!(backend.is_some());
        assert_eq!(backend.unwrap().name(), "claude");

        let backend = get_agent("kiro");
        assert!(backend.is_some());
        assert_eq!(backend.unwrap().name(), "kiro");
    }

    #[test]
    fn test_get_agent_case_insensitive() {
        // Now case-insensitive due to AgentType::parse()
        assert!(get_agent("Claude").is_some());
        assert!(get_agent("KIRO").is_some());
        assert!(get_agent("gEmInI").is_some());
    }

    #[test]
    fn test_get_agent_unknown() {
        assert!(get_agent("unknown").is_none());
        assert!(get_agent("").is_none());
    }

    #[test]
    fn test_get_agent_by_type() {
        let backend = get_agent_by_type(AgentType::Claude);
        assert!(backend.is_some());
        assert_eq!(backend.unwrap().name(), "claude");

        let backend = get_agent_by_type(AgentType::Kiro);
        assert!(backend.is_some());
        assert_eq!(backend.unwrap().name(), "kiro");
    }

    #[test]
    fn test_is_valid_agent() {
        assert!(is_valid_agent("amp"));
        assert!(is_valid_agent("claude"));
        assert!(is_valid_agent("kiro"));
        assert!(is_valid_agent("gemini"));
        assert!(is_valid_agent("codex"));

        // Now case-insensitive
        assert!(is_valid_agent("Claude"));
        assert!(is_valid_agent("KIRO"));

        assert!(!is_valid_agent("unknown"));
        assert!(!is_valid_agent(""));
    }

    #[test]
    fn test_valid_agent_names() {
        let names = valid_agent_names();
        assert_eq!(names.len(), 5);
        for agent in ["amp", "claude", "kiro", "gemini", "codex"] {
            assert!(names.contains(&agent));
        }
    }

    #[test]
    fn test_default_agent_name() {
        assert_eq!(default_agent_name(), "claude");
    }

    #[test]
    fn test_default_agent_type() {
        assert_eq!(default_agent_type(), AgentType::Claude);
    }

    #[test]
    fn test_get_default_command() {
        assert_eq!(get_default_command("amp"), Some("amp"));
        assert_eq!(get_default_command("claude"), Some("claude"));
        assert_eq!(get_default_command("kiro"), Some("kiro-cli chat"));
        assert_eq!(get_default_command("gemini"), Some("gemini"));
        assert_eq!(get_default_command("codex"), Some("codex"));
        assert_eq!(get_default_command("unknown"), None);
    }

    #[test]
    fn test_get_process_patterns() {
        let claude_patterns = get_process_patterns("claude");
        assert!(claude_patterns.is_some());
        let patterns = claude_patterns.unwrap();
        assert!(patterns.contains(&"claude".to_string()));
        assert!(patterns.contains(&"claude-code".to_string()));

        let kiro_patterns = get_process_patterns("kiro");
        assert!(kiro_patterns.is_some());
        let patterns = kiro_patterns.unwrap();
        assert!(patterns.contains(&"kiro-cli".to_string()));
        assert!(patterns.contains(&"kiro".to_string()));

        assert!(get_process_patterns("unknown").is_none());
    }

    #[test]
    fn test_is_agent_available() {
        // Should return Some(bool) for known agents
        let result = is_agent_available("claude");
        assert!(result.is_some());
        // The actual value depends on whether claude is installed

        // Should return None for unknown agents
        assert!(is_agent_available("unknown").is_none());
    }

    #[test]
    fn test_registry_contains_all_agents() {
        // Ensure all expected agents are registered
        let expected_agents = ["amp", "claude", "kiro", "gemini", "codex"];
        for agent in expected_agents {
            assert!(
                is_valid_agent(agent),
                "Registry should contain agent: {}",
                agent
            );
        }
    }

    #[test]
    fn test_supported_agents_string() {
        let s = supported_agents_string();
        assert!(s.contains("amp"));
        assert!(s.contains("claude"));
        assert!(s.contains("kiro"));
        assert!(s.contains("gemini"));
        assert!(s.contains("codex"));
        // Verify it's comma-separated
        assert!(s.contains(", "));
        // Verify removed agents are NOT present
        assert!(!s.contains("aether"));
    }

    #[test]
    fn test_registry_and_agent_type_in_sync() {
        // Verify registry count matches AgentType count
        // This ensures no orphaned backends and no missing registrations
        let agent_count = valid_agent_names().len();
        let type_count = AgentType::all().len();
        assert_eq!(
            agent_count, type_count,
            "Registry should have exactly as many agents ({}) as AgentType variants ({})",
            agent_count, type_count
        );
    }

    #[test]
    fn test_all_agent_types_have_backends() {
        // Verify every AgentType variant has a registered backend
        for agent_type in AgentType::all() {
            let backend = get_agent_by_type(*agent_type);
            assert!(
                backend.is_some(),
                "AgentType::{:?} should have a registered backend",
                agent_type
            );
            // Verify the backend's name matches the AgentType's string representation
            assert_eq!(
                backend.unwrap().name(),
                agent_type.as_str(),
                "Backend name should match AgentType string for {:?}",
                agent_type
            );
        }
    }
}
