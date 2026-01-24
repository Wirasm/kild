//! Default implementations for configuration types.
//!
//! This module contains all `Default` implementations and helper functions
//! for providing default values in serde deserialization.

use crate::agents;
use crate::config::types::{AgentConfig, Config, HealthConfig, TerminalConfig};
use std::path::PathBuf;

/// Returns the default agent name.
///
/// Used by serde `#[serde(default = "...")]` attribute.
pub fn default_agent() -> String {
    agents::default_agent_name().to_string()
}

/// Returns the default spawn delay in milliseconds (1000ms).
///
/// This is the base delay between retry attempts when searching for
/// spawned agent processes. The retry loop uses exponential backoff,
/// so 1000ms provides a reasonable starting point.
///
/// Used by serde `#[serde(default = "...")]` attribute.
pub fn default_spawn_delay_ms() -> u64 {
    1000
}

/// Returns the default max retry attempts (5).
///
/// Combined with the spawn delay and exponential backoff, 5 attempts
/// provides approximately 30 seconds total wait time for process discovery
/// after terminal spawn.
///
/// Used by serde `#[serde(default = "...")]` attribute.
pub fn default_max_retry_attempts() -> u32 {
    5
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

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            preferred: None,
            spawn_delay_ms: 1000,
            max_retry_attempts: 5,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let shards_dir = match dirs::home_dir() {
            Some(home) => home.join(".shards"),
            None => {
                eprintln!(
                    "Warning: Could not find home directory. Set HOME environment variable. \
                    Using fallback directory."
                );
                std::env::temp_dir().join(".shards")
            }
        };

        Self {
            shards_dir,
            log_level: std::env::var("SHARDS_LOG_LEVEL").unwrap_or("info".to_string()),
            default_port_count: parse_default_port_count(),
            base_port_range: parse_base_port_range(),
        }
    }
}

/// Parse SHARDS_DEFAULT_PORT_COUNT env var with validation and warnings.
fn parse_default_port_count() -> u16 {
    let Ok(val) = std::env::var("SHARDS_DEFAULT_PORT_COUNT") else {
        return 10;
    };

    match val.parse::<u16>() {
        Ok(count) if count > 0 && count <= 1000 => count,
        _ => {
            eprintln!(
                "Warning: Invalid SHARDS_DEFAULT_PORT_COUNT '{}', using default 10",
                val
            );
            10
        }
    }
}

/// Parse SHARDS_BASE_PORT_RANGE env var with proper warning on invalid values.
fn parse_base_port_range() -> u16 {
    let Ok(val) = std::env::var("SHARDS_BASE_PORT_RANGE") else {
        return 3000;
    };

    match val.parse::<u16>() {
        Ok(port) => port,
        Err(_) => {
            eprintln!(
                "Warning: Invalid SHARDS_BASE_PORT_RANGE '{}', using default 3000",
                val
            );
            3000
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

impl HealthConfig {
    /// Returns the idle threshold in minutes, defaulting to 10.
    pub fn idle_threshold_minutes(&self) -> u64 {
        self.idle_threshold_minutes.unwrap_or(10)
    }

    /// Returns the refresh interval in seconds, defaulting to 5.
    pub fn refresh_interval_secs(&self) -> u64 {
        self.refresh_interval_secs.unwrap_or(5)
    }

    /// Returns the history retention in days, defaulting to 7.
    pub fn history_retention_days(&self) -> u64 {
        self.history_retention_days.unwrap_or(7)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::ShardsConfig;

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
        assert!(config.sessions_dir().to_string_lossy().contains("sessions"));
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
    fn test_health_config_defaults() {
        let config = ShardsConfig::default();
        assert_eq!(config.health.idle_threshold_minutes(), 10);
        assert_eq!(config.health.refresh_interval_secs(), 5);
        assert!(!config.health.history_enabled);
        assert_eq!(config.health.history_retention_days(), 7);
    }

    #[test]
    fn test_terminal_config_default() {
        let config = TerminalConfig::default();
        assert!(config.preferred.is_none());
        assert_eq!(config.spawn_delay_ms, 1000);
        assert_eq!(config.max_retry_attempts, 5);
    }

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert_eq!(config.default, "claude");
        assert!(config.startup_command.is_none());
        assert!(config.flags.is_none());
    }

    #[test]
    fn test_terminal_config_serde_defaults() {
        // Test that TOML deserialization with missing fields uses correct defaults
        let toml_str = r#"
[terminal]
preferred = "ghostty"
"#;
        let config: ShardsConfig = toml::from_str(toml_str).unwrap();

        // These should be the documented defaults, NOT 0
        assert_eq!(
            config.terminal.spawn_delay_ms, 1000,
            "spawn_delay_ms should default to 1000, not 0"
        );
        assert_eq!(
            config.terminal.max_retry_attempts, 5,
            "max_retry_attempts should default to 5, not 0"
        );
        assert_eq!(config.terminal.preferred, Some("ghostty".to_string()));
    }

    #[test]
    fn test_terminal_config_empty_section_serde_defaults() {
        // Test with completely empty terminal section
        let toml_str = r#"
[agent]
default = "claude"
"#;
        let config: ShardsConfig = toml::from_str(toml_str).unwrap();

        assert_eq!(
            config.terminal.spawn_delay_ms, 1000,
            "spawn_delay_ms should default to 1000 when terminal section is missing"
        );
        assert_eq!(
            config.terminal.max_retry_attempts, 5,
            "max_retry_attempts should default to 5 when terminal section is missing"
        );
    }

    #[test]
    fn test_terminal_config_explicit_zero_preserved() {
        // Verify that explicit zero values in config are preserved, not overridden to defaults
        let toml_str = r#"
[terminal]
spawn_delay_ms = 0
max_retry_attempts = 0
"#;
        let config: ShardsConfig = toml::from_str(toml_str).unwrap();

        // Explicit 0 should be preserved - serde default only applies to missing fields
        assert_eq!(
            config.terminal.spawn_delay_ms, 0,
            "explicit zero should be preserved, not overridden to default"
        );
        assert_eq!(
            config.terminal.max_retry_attempts, 0,
            "explicit zero should be preserved, not overridden to default"
        );
    }
}
