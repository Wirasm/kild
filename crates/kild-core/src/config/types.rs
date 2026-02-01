//! Configuration type definitions for KILD CLI.
//!
//! This module contains all configuration struct definitions used throughout
//! the KILD CLI. These types are serialized/deserialized from TOML config files.
//!
//! # Example Configuration
//!
//! ```toml
//! [agent]
//! default = "claude"
//! startup_command = "claude"
//! flags = "--yolo"
//!
//! [terminal]
//! preferred = "iterm2"
//!
//! [agents.kiro]
//! startup_command = "kiro-cli chat"
//! flags = "--trust-all-tools"
//!
//! [health]
//! idle_threshold_minutes = 10
//! history_enabled = true
//! ```

use crate::files::types::IncludeConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Runtime configuration for the KILD CLI.
///
/// This struct holds paths and settings that are derived from environment
/// variables and system defaults, not from config files.
#[derive(Debug, Clone)]
pub struct Config {
    /// Base directory for all KILD data (default: ~/.kild)
    pub kild_dir: PathBuf,
    /// Log level for the application
    pub log_level: String,
    /// Default number of ports to allocate per session
    pub default_port_count: u16,
    /// Base port range for session port allocation
    pub base_port_range: u16,
}

/// Main configuration loaded from TOML config files.
///
/// This is the primary configuration structure that gets loaded from:
/// 1. User config: `~/.kild/config.toml`
/// 2. Project config: `./.kild/config.toml`
///
/// Project config values override user config values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KildConfig {
    /// Global agent configuration
    #[serde(default)]
    pub agent: AgentConfig,

    /// Terminal preferences
    #[serde(default)]
    pub terminal: TerminalConfig,

    /// Per-agent settings that override global agent config
    #[serde(default)]
    pub agents: HashMap<String, AgentSettings>,

    /// File inclusion patterns for worktrees
    #[serde(default = "default_include_patterns_option")]
    pub include_patterns: Option<IncludeConfig>,

    /// Health monitoring configuration
    #[serde(default)]
    pub health: HealthConfig,

    /// Git configuration for worktree creation
    #[serde(default)]
    pub git: GitConfig,
}

impl Default for KildConfig {
    fn default() -> Self {
        Self {
            agent: AgentConfig::default(),
            terminal: TerminalConfig::default(),
            agents: HashMap::default(),
            include_patterns: default_include_patterns_option(),
            health: HealthConfig::default(),
            git: GitConfig::default(),
        }
    }
}

/// Git configuration for worktree creation.
///
/// Controls how new worktrees are branched — which remote to fetch from
/// and which branch to use as the base for new kild branches.
///
/// Fields are `Option<T>` to support proper config hierarchy merging:
/// only explicitly-set values override lower-priority configs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitConfig {
    /// Remote name to fetch from before creating worktrees.
    /// Default: "origin"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,

    /// Base branch to fetch and create new worktrees from.
    /// Default: "main"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,

    /// Whether to fetch the base branch from remote before creating a worktree.
    /// Default: true
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetch_before_create: Option<bool>,
}

impl GitConfig {
    /// Returns the remote name, defaulting to "origin".
    pub fn remote(&self) -> &str {
        self.remote.as_deref().unwrap_or("origin")
    }

    /// Returns the base branch, defaulting to "main".
    pub fn base_branch(&self) -> &str {
        self.base_branch.as_deref().unwrap_or("main")
    }

    /// Returns whether to fetch before creating worktrees, defaulting to true.
    pub fn fetch_before_create(&self) -> bool {
        self.fetch_before_create.unwrap_or(true)
    }
}

/// Returns default include config wrapped in Option for serde default.
fn default_include_patterns_option() -> Option<IncludeConfig> {
    Some(IncludeConfig::default())
}

/// Health monitoring configuration.
///
/// Controls how session health is monitored and reported.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HealthConfig {
    /// Threshold in minutes before a session is considered idle.
    /// Default: 10 minutes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_threshold_minutes: Option<u64>,

    /// Interval in seconds between health check refreshes.
    /// Default: 5 seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_interval_secs: Option<u64>,

    /// Whether to track session history.
    #[serde(default)]
    pub history_enabled: bool,

    /// Number of days to retain session history.
    /// Default: 7 days.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_retention_days: Option<u64>,
}

/// Global agent configuration.
///
/// Defines the default agent and global settings that apply to all agents
/// unless overridden by per-agent settings in `[agents.<name>]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Default agent to use when none is specified.
    /// Must be one of: claude, kiro, gemini, codex, aether.
    #[serde(default = "super::defaults::default_agent")]
    pub default: String,

    /// Global startup command (used if no agent-specific command).
    #[serde(default)]
    pub startup_command: Option<String>,

    /// Global flags to append to agent commands.
    #[serde(default)]
    pub flags: Option<String>,
}

/// Terminal configuration.
///
/// Controls which terminal emulator to use and spawn behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    /// Preferred terminal emulator.
    /// Options: iterm2, iterm, terminal, ghostty, native.
    #[serde(default)]
    pub preferred: Option<String>,

    /// Delay in milliseconds after spawning a terminal.
    /// Default: 1000ms.
    #[serde(default = "super::defaults::default_spawn_delay_ms")]
    pub spawn_delay_ms: u64,

    /// Maximum retry attempts for process discovery after terminal spawn.
    /// Default: 5.
    #[serde(default = "super::defaults::default_max_retry_attempts")]
    pub max_retry_attempts: u32,
}

/// Per-agent settings that override global agent config.
///
/// Used in `[agents.<name>]` sections of the config file.
///
/// # Example
///
/// ```toml
/// [agents.claude]
/// startup_command = "cc"
/// flags = "--dangerous"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    /// Agent-specific startup command.
    #[serde(default)]
    pub startup_command: Option<String>,

    /// Agent-specific flags to append to the command.
    #[serde(default)]
    pub flags: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kild_config_serialization() {
        let config = KildConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: KildConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.agent.default, parsed.agent.default);
    }

    #[test]
    fn test_health_config_serialization() {
        let config = HealthConfig {
            idle_threshold_minutes: Some(15),
            refresh_interval_secs: Some(10),
            history_enabled: true,
            history_retention_days: Some(30),
        };
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("idle_threshold_minutes = 15"));
        assert!(toml_str.contains("history_enabled = true"));
    }

    #[test]
    fn test_agent_settings_deserialize() {
        let toml_str = r#"
startup_command = "custom-cmd"
flags = "--custom-flag"
"#;
        let settings: AgentSettings = toml::from_str(toml_str).unwrap();
        assert_eq!(settings.startup_command, Some("custom-cmd".to_string()));
        assert_eq!(settings.flags, Some("--custom-flag".to_string()));
    }

    #[test]
    fn test_git_config_serialization() {
        let config = GitConfig::default();
        assert_eq!(config.remote(), "origin");
        assert_eq!(config.base_branch(), "main");
        assert!(config.fetch_before_create());

        let toml_str = toml::to_string(&config).unwrap();
        let parsed: GitConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.remote(), config.remote());
        assert_eq!(parsed.base_branch(), config.base_branch());
        assert_eq!(parsed.fetch_before_create(), config.fetch_before_create());
    }

    #[test]
    fn test_git_config_from_toml() {
        let config: KildConfig = toml::from_str(
            r#"
[git]
remote = "upstream"
base_branch = "develop"
fetch_before_create = false
"#,
        )
        .unwrap();
        assert_eq!(config.git.remote(), "upstream");
        assert_eq!(config.git.base_branch(), "develop");
        assert!(!config.git.fetch_before_create());
    }

    #[test]
    fn test_git_config_defaults_when_missing() {
        let config: KildConfig = toml::from_str("").unwrap();
        assert_eq!(config.git.remote(), "origin");
        assert_eq!(config.git.base_branch(), "main");
        assert!(config.git.fetch_before_create());
    }

    #[test]
    fn test_git_config_partial_toml() {
        let config: KildConfig = toml::from_str(
            r#"
[git]
base_branch = "develop"
"#,
        )
        .unwrap();
        assert_eq!(config.git.remote(), "origin"); // default via accessor
        assert_eq!(config.git.base_branch(), "develop"); // specified
        assert!(config.git.fetch_before_create()); // default via accessor
    }
}
