//! # Configuration System
//!
//! Hierarchical TOML configuration system for KILD CLI.
//!
//! ## Configuration Hierarchy
//!
//! Configuration is loaded in the following order (later sources override earlier ones):
//! 1. **Hardcoded defaults** - Built-in fallback values
//! 2. **User config** - `~/.kild/config.toml` (global user preferences)
//! 3. **Project config** - `./.kild/config.toml` (project-specific overrides)
//! 4. **CLI arguments** - Command-line flags (highest priority)
//!
//! ## Usage Example
//!
//! ```toml
//! # ~/.kild/config.toml
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
//! ```rust,no_run
//! use kild_core::config::KildConfig;
//!
//! // Handle config errors explicitly - don't silently fall back to defaults
//! fn example() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = KildConfig::load_hierarchy()?;
//!     let agent_command = config.get_agent_command("claude")?;
//!     Ok(())
//! }
//! ```

pub mod defaults;
pub mod loading;
pub mod types;
pub mod validation;

// Public API exports
pub use types::{
    AgentConfig, AgentSettings, Config, EditorConfig, GitConfig, HealthConfig, KildConfig,
    TerminalConfig, UiConfig, VALID_NAV_MODIFIERS,
};
pub use validation::{VALID_TERMINALS, validate_config};

// Convenience methods delegating to submodule functions
impl KildConfig {
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

    /// Whether daemon mode is the default for new sessions.
    ///
    /// When true, `kild create` uses daemon unless `--no-daemon` is passed.
    pub fn is_daemon_enabled(&self) -> bool {
        self.daemon.enabled()
    }

    /// Whether to auto-start the daemon if not running.
    pub fn daemon_auto_start(&self) -> bool {
        self.daemon.auto_start()
    }
}
