//! Default implementations for configuration types.
//!
//! This module contains all `Default` implementations and helper functions
//! for providing default values in serde deserialization.

use crate::agent_data;
use crate::types::{AgentConfig, Config, HealthConfig};
use kild_paths::KildPaths;
use std::path::PathBuf;
use tracing::warn;

/// Returns the default agent name.
///
/// Used by serde `#[serde(default = "...")]` attribute.
pub fn default_agent() -> String {
    agent_data::default_agent_name().to_string()
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

impl Default for Config {
    fn default() -> Self {
        let paths = KildPaths::resolve().unwrap_or_else(|e| {
            let fallback = std::env::temp_dir().join(".kild");
            warn!(
                event = "core.config.home_dir_fallback",
                error = %e,
                fallback = %fallback.display(),
            );
            eprintln!(
                "Warning: Could not find home directory. Set HOME environment variable. \
                Using fallback directory: {}",
                fallback.display()
            );
            KildPaths::from_dir(fallback)
        });

        Self {
            paths,
            log_level: std::env::var("KILD_LOG_LEVEL").unwrap_or("info".to_string()),
            default_port_count: parse_default_port_count(),
            base_port_range: parse_base_port_range(),
        }
    }
}

/// Parse KILD_DEFAULT_PORT_COUNT env var with validation and warnings.
fn parse_default_port_count() -> u16 {
    let Ok(val) = std::env::var("KILD_DEFAULT_PORT_COUNT") else {
        return 10;
    };

    match val.parse::<u16>() {
        Ok(count) if count > 0 && count <= 1000 => count,
        _ => {
            eprintln!(
                "Warning: Invalid KILD_DEFAULT_PORT_COUNT '{}', using default 10",
                val
            );
            10
        }
    }
}

/// Parse KILD_BASE_PORT_RANGE env var with proper warning on invalid values.
fn parse_base_port_range() -> u16 {
    let Ok(val) = std::env::var("KILD_BASE_PORT_RANGE") else {
        return 3000;
    };

    match val.parse::<u16>() {
        Ok(port) => port,
        Err(_) => {
            eprintln!(
                "Warning: Invalid KILD_BASE_PORT_RANGE '{}', using default 3000",
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
        self.paths().worktrees_dir()
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.paths().sessions_dir()
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
    use crate::types::KildConfig;

    #[test]
    fn test_config_default() {
        let config = Config::new();
        assert!(config.kild_dir().to_string_lossy().contains(".kild"));
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
    fn test_kild_config_default() {
        let config = KildConfig::default();
        assert_eq!(config.agent.default, "claude");
        assert!(config.agent.startup_command.is_none());
        assert!(config.terminal.preferred.is_none());
        assert!(config.agents.is_empty());
    }

    #[test]
    fn test_health_config_defaults() {
        let config = KildConfig::default();
        assert_eq!(config.health.idle_threshold_minutes(), 10);
        assert_eq!(config.health.refresh_interval_secs(), 5);
        assert!(!config.health.history_enabled);
        assert_eq!(config.health.history_retention_days(), 7);
    }

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert_eq!(config.default, "claude");
        assert!(config.startup_command.is_none());
        assert!(config.flags.is_none());
    }
}
