//! Agent name/command data for config validation and defaults.
//!
//! This is an intentional, documented duplication of agent data from
//! `kild-core/src/agents/types.rs`. The alternative — moving `AgentType`
//! to `kild-protocol` — would expand protocol scope beyond IPC types.
//! The duplicated data is 6 agent name/command pairs, trivial to maintain.
//!
//! Keep in sync with `crates/kild-core/src/agents/types.rs:AgentType`.

/// Agent name + default command pairs.
/// Keep in sync with kild-core/src/agents/types.rs:AgentType.
const AGENT_DATA: &[(&str, &str)] = &[
    ("amp", "amp"),
    ("claude", "claude"),
    ("codex", "codex"),
    ("gemini", "gemini"),
    ("kiro", "kiro-cli chat"),
    ("opencode", "opencode"),
];

const DEFAULT_AGENT: &str = "claude";

pub fn is_valid_agent(name: &str) -> bool {
    AGENT_DATA.iter().any(|(n, _)| name.eq_ignore_ascii_case(n))
}

pub fn default_agent_name() -> &'static str {
    DEFAULT_AGENT
}

pub fn get_default_command(name: &str) -> Option<&'static str> {
    AGENT_DATA
        .iter()
        .find(|(n, _)| name.eq_ignore_ascii_case(n))
        .map(|(_, cmd)| *cmd)
}

pub fn supported_agents_string() -> String {
    let mut names: Vec<&str> = AGENT_DATA.iter().map(|(n, _)| *n).collect();
    names.sort();
    names.join(", ")
}
