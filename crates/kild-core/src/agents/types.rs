//! Agent type definitions and core data structures.

use serde::{Deserialize, Serialize};

/// Supported agent types in KILD.
///
/// Each variant represents a known AI coding assistant that can be
/// spawned in a worktree session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    Amp,
    Claude,
    Kiro,
    Gemini,
    Codex,
}

impl AgentType {
    /// Get the canonical string name for this agent type.
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentType::Amp => "amp",
            AgentType::Claude => "claude",
            AgentType::Kiro => "kiro",
            AgentType::Gemini => "gemini",
            AgentType::Codex => "codex",
        }
    }

    /// Parse an agent type from a string (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "amp" => Some(AgentType::Amp),
            "claude" => Some(AgentType::Claude),
            "kiro" => Some(AgentType::Kiro),
            "gemini" => Some(AgentType::Gemini),
            "codex" => Some(AgentType::Codex),
            _ => None,
        }
    }

    /// Get all supported agent types.
    pub fn all() -> &'static [AgentType] {
        &[
            AgentType::Amp,
            AgentType::Claude,
            AgentType::Kiro,
            AgentType::Gemini,
            AgentType::Codex,
        ]
    }
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for AgentType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| {
            let supported = AgentType::all()
                .iter()
                .map(|a| a.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!("Unknown agent '{}'. Supported: {}", s, supported)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type_as_str() {
        assert_eq!(AgentType::Amp.as_str(), "amp");
        assert_eq!(AgentType::Claude.as_str(), "claude");
        assert_eq!(AgentType::Kiro.as_str(), "kiro");
        assert_eq!(AgentType::Gemini.as_str(), "gemini");
        assert_eq!(AgentType::Codex.as_str(), "codex");
    }

    #[test]
    fn test_agent_type_parse() {
        assert_eq!(AgentType::parse("claude"), Some(AgentType::Claude));
        assert_eq!(AgentType::parse("CLAUDE"), Some(AgentType::Claude));
        assert_eq!(AgentType::parse("Claude"), Some(AgentType::Claude));
        assert_eq!(AgentType::parse("kiro"), Some(AgentType::Kiro));
        assert_eq!(AgentType::parse("unknown"), None);
        assert_eq!(AgentType::parse(""), None);
    }

    #[test]
    fn test_agent_type_all() {
        let all = AgentType::all();
        assert_eq!(all.len(), 5);
        assert!(all.contains(&AgentType::Amp));
        assert!(all.contains(&AgentType::Claude));
        assert!(all.contains(&AgentType::Kiro));
        assert!(all.contains(&AgentType::Gemini));
        assert!(all.contains(&AgentType::Codex));
    }

    #[test]
    fn test_agent_type_display() {
        assert_eq!(format!("{}", AgentType::Claude), "claude");
        assert_eq!(format!("{}", AgentType::Kiro), "kiro");
    }

    #[test]
    fn test_agent_type_serde() {
        let claude = AgentType::Claude;
        let json = serde_json::to_string(&claude).unwrap();
        assert_eq!(json, "\"claude\"");

        let parsed: AgentType = serde_json::from_str("\"kiro\"").unwrap();
        assert_eq!(parsed, AgentType::Kiro);
    }

    #[test]
    fn test_agent_type_equality() {
        assert_eq!(AgentType::Claude, AgentType::Claude);
        assert_ne!(AgentType::Claude, AgentType::Kiro);
    }

    #[test]
    fn test_agent_type_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(AgentType::Claude);
        set.insert(AgentType::Kiro);
        set.insert(AgentType::Claude); // Duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_agent_type_from_str() {
        use std::str::FromStr;
        assert_eq!(AgentType::from_str("claude").unwrap(), AgentType::Claude);
        assert_eq!(AgentType::from_str("KIRO").unwrap(), AgentType::Kiro);
        assert_eq!(AgentType::from_str("Gemini").unwrap(), AgentType::Gemini);

        let err = AgentType::from_str("unknown").unwrap_err();
        assert!(err.contains("Unknown agent 'unknown'"));
        assert!(err.contains("claude"));
        assert!(err.contains("kiro"));
    }
}
