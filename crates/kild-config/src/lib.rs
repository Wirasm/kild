//! # kild-config
//!
//! TOML configuration types, loading, validation, and keybindings for KILD.
//!
//! Single source of truth for all `KildConfig`, `Config`, and `Keybindings` types.
//! Depends only on `kild-paths` and `kild-protocol`.

mod agent_data;
mod defaults;
mod loading;
mod validation;

pub mod errors;
pub mod include_config;
pub mod keybindings;
pub mod types;

// Public API re-exports
pub use errors::ConfigError;
pub use include_config::{CopyOptions, IncludeConfig, PatternRule, default_include_patterns};
pub use keybindings::{Keybindings, NavigationKeybindings, TerminalKeybindings};
pub use loading::{get_agent_command, load_hierarchy, merge_configs};
pub use types::{
    AgentConfig, AgentSettings, Config, DaemonRuntimeConfig, EditorConfig, GitConfig, HealthConfig,
    KildConfig, TerminalConfig, UiConfig,
};
pub use validation::{VALID_TERMINALS, validate_config};

impl Keybindings {
    /// Load keybindings from the user/project hierarchy.
    ///
    /// Never returns an error — parse failures warn and fall back to defaults.
    pub fn load_hierarchy() -> Self {
        keybindings::load_hierarchy()
    }
}

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
    pub fn validate(&self) -> Result<(), ConfigError> {
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
