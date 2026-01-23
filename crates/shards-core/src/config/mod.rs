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
//! use shards_core::config::ShardsConfig;
//!
//! let config = ShardsConfig::load_hierarchy().unwrap_or_default();
//! let agent_command = config.get_agent_command("claude");
//! ```

pub mod defaults;
pub mod loading;
pub mod types;
pub mod validation;

// Public API exports
pub use types::{AgentConfig, AgentSettings, Config, HealthConfig, ShardsConfig, TerminalConfig};
pub use validation::{VALID_TERMINALS, validate_config};

// Backward-compatible delegation for ShardsConfig methods
impl ShardsConfig {
    /// Load configuration from the hierarchy of config files.
    ///
    /// See [`loading::load_hierarchy`] for details.
    pub fn load_hierarchy() -> Result<Self, Box<dyn std::error::Error>> {
        loading::load_hierarchy()
    }

    /// Validate the configuration.
    ///
    /// See [`validation::validate_config`] for details.
    pub fn validate(&self) -> Result<(), crate::errors::ConfigError> {
        validation::validate_config(self)
    }

    /// Get the command to run for a specific agent.
    ///
    /// See [`loading::get_agent_command`] for details.
    pub fn get_agent_command(
        &self,
        agent_name: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        loading::get_agent_command(self, agent_name)
    }
}
