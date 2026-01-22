//! # Configuration System
//!
//! Hierarchical TOML configuration system for Shards CLI.
//!
//! ## Configuration Hierarchy
//!
//! Configuration is loaded in the following order (later sources override earlier ones):
//! 1. **Hardcoded defaults** - Built-in fallback values
//! 2. **User config** - `~/.shards/config.toml` (global user preferences)
//! 3. **Project config** - `./shards/config.toml` (project-specific overrides)
//! 4. **CLI arguments** - Command-line flags (highest priority)
//!
//! ## Usage Example
//!
//! ```toml
//! # ~/.shards/config.toml
//! [agent]
//! default = "kiro"
//! startup_command = "kiro-cli chat"
//! flags = ""
//!
//! [terminal]
//! preferred = "iterm2"
//!
//! [agents.claude]
//! startup_command = "claude"
//! flags = "--yolo"
//! ```
//!
//! ## Loading Configuration
//!
//! ```rust
//! use shards::core::config::ShardsConfig;
//!
//! let config = ShardsConfig::load_hierarchy().unwrap_or_default();
//! let agent_command = config.get_agent_command("claude");
//! ```

use std::path::PathBuf;
use std::collections::HashMap;
use crate::files::types::IncludeConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use tracing;

#[derive(Debug, Clone)]
pub struct Config {
    pub shards_dir: PathBuf,
    pub log_level: String,
    pub default_port_count: u16,
    pub base_port_range: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShardsConfig {
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub terminal: TerminalConfig,
    #[serde(default)]
    pub agents: HashMap<String, AgentSettings>,
    #[serde(default)]
    pub include_patterns: Option<IncludeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_agent")]
    pub default: String,
    #[serde(default)]
    pub startup_command: Option<String>,
    #[serde(default)]
    pub flags: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    #[serde(default)]
    pub preferred: Option<String>,
    #[serde(default)]
    pub spawn_delay_ms: u64,
    #[serde(default)]
    pub max_retry_attempts: u32,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            preferred: None,
            spawn_delay_ms: 1000,
            max_retry_attempts: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    #[serde(default)]
    pub startup_command: Option<String>,
    #[serde(default)]
    pub flags: Option<String>,
}

fn default_agent() -> String {
    "claude".to_string()
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            default: default_agent(),
            startup_command: None,
            flags: None,
        }
    }
}

impl ShardsConfig {
    pub fn load_hierarchy() -> Result<Self, Box<dyn std::error::Error>> {
        let mut config = ShardsConfig::default();
        
        // Load user config
        if let Ok(user_config) = Self::load_user_config() {
            config = Self::merge_configs(config, user_config);
        }
        
        // Load project config
        if let Ok(project_config) = Self::load_project_config() {
            config = Self::merge_configs(config, project_config);
        }
        
        // Validate the final configuration
        config.validate()?;
        
        Ok(config)
    }
    
    /// Validate configuration values
    pub fn validate(&self) -> Result<(), crate::core::errors::ConfigError> {
        // Validate agent name
        let valid_agents = ["claude", "kiro", "gemini", "codex", "aether"];
        if !valid_agents.contains(&self.agent.default.as_str()) {
            return Err(crate::core::errors::ConfigError::InvalidAgent {
                agent: self.agent.default.clone(),
            });
        }
        
        // Validate terminal preference if set
        if let Some(ref terminal) = self.terminal.preferred {
            let valid_terminals = ["iterm2", "iterm", "terminal", "ghostty", "native"];
            if !valid_terminals.contains(&terminal.as_str()) {
                // Don't error on invalid terminal, just warn via logging
                tracing::warn!(
                    event = "config.invalid_terminal_preference",
                    terminal = terminal,
                    valid_terminals = ?valid_terminals
                );
            }
        }
        
        // Validate include patterns if configured
        if let Some(ref include_config) = self.include_patterns
            && let Err(e) = include_config.validate() {
                return Err(crate::core::errors::ConfigError::InvalidConfiguration {
                    message: format!("Invalid include patterns: {}", e),
                });
            }
        
        Ok(())
    }
    
    fn load_user_config() -> Result<ShardsConfig, Box<dyn std::error::Error>> {
        let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
        let config_path = home_dir.join(".shards").join("config.toml");
        Self::load_config_file(&config_path)
    }
    
    fn load_project_config() -> Result<ShardsConfig, Box<dyn std::error::Error>> {
        let config_path = std::env::current_dir()?.join("shards").join("config.toml");
        Self::load_config_file(&config_path)
    }
    
    fn load_config_file(path: &PathBuf) -> Result<ShardsConfig, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: ShardsConfig = toml::from_str(&content)?;
        Ok(config)
    }
    
    fn merge_configs(base: ShardsConfig, override_config: ShardsConfig) -> ShardsConfig {
        ShardsConfig {
            agent: AgentConfig {
                // Always use override agent if it was explicitly set in the config file
                // We can't distinguish between explicit "claude" and default "claude" here,
                // so we always prefer the override config's agent setting
                default: override_config.agent.default,
                startup_command: override_config.agent.startup_command.or(base.agent.startup_command),
                flags: override_config.agent.flags.or(base.agent.flags),
            },
            terminal: TerminalConfig {
                preferred: override_config.terminal.preferred.or(base.terminal.preferred),
                spawn_delay_ms: override_config.terminal.spawn_delay_ms,
                max_retry_attempts: override_config.terminal.max_retry_attempts,
            },
            agents: {
                let mut merged = base.agents;
                for (key, value) in override_config.agents {
                    merged.insert(key, value);
                }
                merged
            },
            include_patterns: override_config.include_patterns.or(base.include_patterns),
        }
    }
    
    pub fn get_agent_command(&self, agent_name: &str) -> String {
        // Check agent-specific settings first
        if let Some(agent_settings) = self.agents.get(agent_name)
            && let Some(command) = &agent_settings.startup_command {
                let mut full_command = command.clone();
                if let Some(flags) = &agent_settings.flags {
                    full_command.push(' ');
                    full_command.push_str(flags);
                }
                return full_command;
            }
        
        // Fall back to global agent config
        let base_command = self.agent.startup_command.as_deref().unwrap_or(
            match agent_name {
                "claude" => "claude",
                "kiro" => "kiro-cli chat",
                "gemini" => "gemini",
                "codex" => "codex",
                "aether" => "aether",
                _ => agent_name,
            }
        );
        
        let mut full_command = base_command.to_string();
        if let Some(flags) = &self.agent.flags {
            full_command.push(' ');
            full_command.push_str(flags);
        }
        
        full_command
    }
}

impl Default for Config {
    fn default() -> Self {
        let home_dir = dirs::home_dir().expect("Could not find home directory");

        Self {
            shards_dir: home_dir.join(".shards"),
            log_level: std::env::var("SHARDS_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            default_port_count: std::env::var("SHARDS_DEFAULT_PORT_COUNT")
                .ok()
                .and_then(|s| s.parse::<u16>().ok())
                .filter(|&count| count > 0 && count <= 1000)
                .unwrap_or_else(|| {
                    if let Ok(val) = std::env::var("SHARDS_DEFAULT_PORT_COUNT") {
                        eprintln!("Warning: Invalid SHARDS_DEFAULT_PORT_COUNT '{}', using default 10", val);
                    }
                    10
                }),
            base_port_range: std::env::var("SHARDS_BASE_PORT_RANGE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3000),
        }
    }
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn worktrees_dir(&self) -> PathBuf {
        self.shards_dir.join("worktrees")
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.shards_dir.join("sessions")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::env;

    #[test]
    fn test_config_default() {
        let config = Config::new();
        assert!(config.shards_dir.to_string_lossy().contains(".shards"));
        assert_eq!(config.log_level, "info");
    }

    #[test]
    fn test_config_paths() {
        let config = Config::new();
        assert!(
            config
                .worktrees_dir()
                .to_string_lossy()
                .contains("worktrees")
        );
        assert!(
            config
                .sessions_dir()
                .to_string_lossy()
                .contains("sessions")
        );
    }

    #[test]
    fn test_shards_config_default() {
        let config = ShardsConfig::default();
        assert_eq!(config.agent.default, "claude");
        assert!(config.agent.startup_command.is_none());
        assert!(config.terminal.preferred.is_none());
        assert!(config.agents.is_empty());
    }

    #[test]
    fn test_get_agent_command_defaults() {
        let config = ShardsConfig::default();
        
        assert_eq!(config.get_agent_command("claude"), "claude");
        assert_eq!(config.get_agent_command("kiro"), "kiro-cli chat");
        assert_eq!(config.get_agent_command("gemini"), "gemini");
        assert_eq!(config.get_agent_command("unknown"), "unknown");
    }

    #[test]
    fn test_get_agent_command_with_flags() {
        let mut config = ShardsConfig::default();
        config.agent.flags = Some("--yolo".to_string());
        
        assert_eq!(config.get_agent_command("claude"), "claude --yolo");
    }

    #[test]
    fn test_get_agent_command_specific_agent() {
        let mut config = ShardsConfig::default();
        let agent_settings = AgentSettings {
            startup_command: Some("cc".to_string()),
            flags: Some("--dangerous".to_string()),
        };
        config.agents.insert("claude".to_string(), agent_settings);
        
        assert_eq!(config.get_agent_command("claude"), "cc --dangerous");
        assert_eq!(config.get_agent_command("kiro"), "kiro-cli chat");
    }

    #[test]
    fn test_config_validation_valid_agent() {
        let config = ShardsConfig::default(); // Uses "claude" which is valid
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_invalid_agent() {
        let mut config = ShardsConfig::default();
        config.agent.default = "invalid-agent".to_string();
        
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), crate::core::errors::ConfigError::InvalidAgent { .. }));
    }

    #[test]
    fn test_config_hierarchy_integration() {
        // Create temporary directories for testing
        let temp_dir = env::temp_dir().join("shards_config_test");
        let user_config_dir = temp_dir.join("user");
        let project_config_dir = temp_dir.join("project");
        
        // Clean up any existing test directories
        let _ = fs::remove_dir_all(&temp_dir);
        
        // Create test directories
        fs::create_dir_all(&user_config_dir).unwrap();
        fs::create_dir_all(&project_config_dir.join("shards")).unwrap();
        
        // Create user config
        let user_config_content = r#"
[agent]
default = "kiro"
startup_command = "kiro-cli chat"

[terminal]
preferred = "iterm2"
"#;
        fs::write(user_config_dir.join("config.toml"), user_config_content).unwrap();
        
        // Create project config that overrides some settings
        let project_config_content = r#"
[agent]
default = "claude"
flags = "--yolo"
"#;
        fs::write(project_config_dir.join("shards").join("config.toml"), project_config_content).unwrap();
        
        // Test loading user config
        let user_config = ShardsConfig::load_config_file(&user_config_dir.join("config.toml")).unwrap();
        assert_eq!(user_config.agent.default, "kiro");
        assert_eq!(user_config.agent.startup_command, Some("kiro-cli chat".to_string()));
        assert_eq!(user_config.terminal.preferred, Some("iterm2".to_string()));
        
        // Test loading project config
        let project_config = ShardsConfig::load_config_file(&project_config_dir.join("shards").join("config.toml")).unwrap();
        assert_eq!(project_config.agent.default, "claude");
        assert_eq!(project_config.agent.flags, Some("--yolo".to_string()));
        
        // Test merging configs (project overrides user)
        let merged = ShardsConfig::merge_configs(user_config, project_config);
        assert_eq!(merged.agent.default, "claude"); // Overridden by project
        assert_eq!(merged.agent.startup_command, Some("kiro-cli chat".to_string())); // From user
        assert_eq!(merged.agent.flags, Some("--yolo".to_string())); // From project
        assert_eq!(merged.terminal.preferred, Some("iterm2".to_string())); // From user
        
        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_toml_parsing_edge_cases() {
        // Test empty config
        let empty_config: ShardsConfig = toml::from_str("").unwrap();
        assert_eq!(empty_config.agent.default, "claude");
        
        // Test partial config
        let partial_config: ShardsConfig = toml::from_str(r#"
[terminal]
preferred = "iterm2"
"#).unwrap();
        assert_eq!(partial_config.agent.default, "claude"); // Should use default
        assert_eq!(partial_config.terminal.preferred, Some("iterm2".to_string()));
        
        // Test invalid TOML should fail
        let invalid_result: Result<ShardsConfig, _> = toml::from_str("invalid toml [[[");
        assert!(invalid_result.is_err());
    }
}
