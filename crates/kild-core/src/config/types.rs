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
//! [editor]
//! default = "nvim"
//! flags = "--nofork"
//! terminal = true
//!
//! [health]
//! idle_threshold_minutes = 10
//! history_enabled = true
//! ```

use crate::files::types::IncludeConfig;
use crate::forge::ForgeType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::debug;

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

    /// Editor configuration for `kild code`
    #[serde(default)]
    pub editor: EditorConfig,
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
            editor: <EditorConfig as Default>::default(),
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

    /// Force a specific forge backend instead of auto-detecting from remote URL.
    /// When None, detect_forge() inspects the git remote URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forge: Option<ForgeType>,
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

    /// Returns the configured forge override, if any.
    pub fn forge(&self) -> Option<ForgeType> {
        self.forge
    }
}

/// Editor configuration for `kild code`.
///
/// Controls which editor opens worktrees and how it's launched.
///
/// Fields are `Option<T>` to support proper config hierarchy merging:
/// only explicitly-set values override lower-priority configs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EditorConfig {
    /// Editor command configured in TOML.
    /// When None, runtime fallback applies ($EDITOR, then "code").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default: Option<String>,

    /// Flags passed to the editor before the worktree path.
    /// In GUI mode, split by whitespace into separate args.
    /// In terminal mode, passed as a single shell string.
    /// Example: "--new-window" for VS Code
    #[serde(default, skip_serializing_if = "Option::is_none")]
    flags: Option<String>,

    /// Whether to spawn the editor inside a terminal window
    /// via kild's terminal backend (Ghostty, iTerm, etc.).
    /// Required for terminal-based editors (nvim, vim, helix).
    /// Default: false
    #[serde(default, skip_serializing_if = "Option::is_none")]
    terminal: Option<bool>,
}

impl EditorConfig {
    /// Returns the editor command, if configured.
    pub fn default(&self) -> Option<&str> {
        self.default.as_deref()
    }

    /// Returns the editor flags, if configured.
    pub fn flags(&self) -> Option<&str> {
        self.flags.as_deref()
    }

    /// Returns whether to spawn in a terminal, defaulting to false.
    pub fn terminal(&self) -> bool {
        self.terminal.unwrap_or(false)
    }

    /// Override the editor command (used for CLI flag override).
    pub fn set_default(&mut self, editor: String) {
        self.default = Some(editor);
    }

    /// Resolve which editor to use based on priority chain:
    /// CLI override > config default > $EDITOR env var > "code" (VS Code) fallback.
    pub fn resolve_editor(&self, cli_override: Option<&str>) -> String {
        if let Some(editor) = cli_override {
            return editor.to_string();
        }
        if let Some(editor) = self.default() {
            return editor.to_string();
        }
        if let Ok(editor) = std::env::var("EDITOR") {
            return editor;
        }

        debug!(
            event = "core.config.editor_fallback",
            fallback = "code",
            "No editor configured via CLI, config, or $EDITOR — using VS Code"
        );
        "code".to_string()
    }

    /// Build a shell command string for terminal-mode editors.
    ///
    /// The worktree path is shell-escaped for safe interpretation.
    /// Flags are inserted between the editor command and the path.
    pub fn build_terminal_command(&self, editor: &str, worktree_path: &std::path::Path) -> String {
        use crate::terminal::common::escape::shell_escape;
        let escaped_path = shell_escape(&worktree_path.display().to_string());

        if let Some(flags) = self.flags() {
            format!("{} {} {}", editor, flags, escaped_path)
        } else {
            format!("{} {}", editor, escaped_path)
        }
    }

    /// Build a `Command` for GUI-mode editors.
    ///
    /// Flags are split by whitespace into separate arguments.
    /// The worktree path is added as the final argument.
    pub fn build_gui_command(
        &self,
        editor: &str,
        worktree_path: &std::path::Path,
    ) -> std::process::Command {
        let mut cmd = std::process::Command::new(editor);
        if let Some(flags) = self.flags() {
            for flag in flags.split_whitespace() {
                cmd.arg(flag);
            }
        }
        cmd.arg(worktree_path);
        cmd
    }

    /// Merge two editor configs. `other` takes precedence for set fields.
    pub fn merge(self, other: EditorConfig) -> EditorConfig {
        EditorConfig {
            default: other.default.or(self.default),
            flags: other.flags.or(self.flags),
            terminal: other.terminal.or(self.terminal),
        }
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

    #[test]
    fn test_editor_config_serialization() {
        let config = <EditorConfig as Default>::default();
        assert!(config.default().is_none());
        assert!(config.flags().is_none());
        assert!(!config.terminal());

        let toml_str = toml::to_string(&config).unwrap();
        let parsed: EditorConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.default(), config.default());
        assert_eq!(parsed.flags(), config.flags());
        assert_eq!(parsed.terminal(), config.terminal());
    }

    #[test]
    fn test_editor_config_from_toml() {
        let config: KildConfig = toml::from_str(
            r#"
[editor]
default = "nvim"
flags = "--new-window"
terminal = true
"#,
        )
        .unwrap();
        assert_eq!(config.editor.default(), Some("nvim"));
        assert_eq!(config.editor.flags(), Some("--new-window"));
        assert!(config.editor.terminal());
    }

    #[test]
    fn test_editor_config_defaults_when_missing() {
        let config: KildConfig = toml::from_str("").unwrap();
        assert!(config.editor.default().is_none());
        assert!(config.editor.flags().is_none());
        assert!(!config.editor.terminal());
    }

    #[test]
    fn test_editor_config_partial_toml() {
        let config: KildConfig = toml::from_str(
            r#"
[editor]
default = "code"
"#,
        )
        .unwrap();
        assert_eq!(config.editor.default(), Some("code"));
        assert!(config.editor.flags().is_none());
        assert!(!config.editor.terminal());
    }

    // --- EditorConfig resolve_editor / build_command tests ---

    #[test]
    fn test_resolve_editor_cli_override() {
        let config = <EditorConfig as Default>::default();
        assert_eq!(config.resolve_editor(Some("vim")), "vim");
    }

    #[test]
    fn test_resolve_editor_config_default() {
        let mut config = <EditorConfig as Default>::default();
        config.set_default("code".to_string());
        assert_eq!(config.resolve_editor(None), "code");
    }

    #[test]
    fn test_resolve_editor_cli_overrides_config() {
        let mut config = <EditorConfig as Default>::default();
        config.set_default("code".to_string());
        assert_eq!(config.resolve_editor(Some("vim")), "vim");
    }

    #[test]
    fn test_resolve_editor_fallback() {
        let config = <EditorConfig as Default>::default();
        let result = config.resolve_editor(None);
        // Result is either $EDITOR value or "code" fallback
        assert!(!result.is_empty());
    }

    #[test]
    fn test_build_terminal_command_no_flags() {
        let config = <EditorConfig as Default>::default();
        let cmd = config.build_terminal_command("vim", std::path::Path::new("/tmp/worktree"));
        assert!(cmd.contains("vim"));
        assert!(cmd.contains("/tmp/worktree"));
    }

    #[test]
    fn test_build_terminal_command_with_flags() {
        let config: EditorConfig = toml::from_str(r#"flags = "--new-window""#).unwrap();
        let cmd = config.build_terminal_command("code", std::path::Path::new("/tmp/worktree"));
        assert!(cmd.contains("code"));
        assert!(cmd.contains("--new-window"));
        assert!(cmd.contains("/tmp/worktree"));
    }

    #[test]
    fn test_build_gui_command_no_flags() {
        let config = <EditorConfig as Default>::default();
        let cmd = config.build_gui_command("code", std::path::Path::new("/tmp/worktree"));
        assert_eq!(cmd.get_program(), "code");
    }

    #[test]
    fn test_build_terminal_command_prevents_shell_injection() {
        let config = <EditorConfig as Default>::default();

        // Backtick command substitution
        let cmd = config.build_terminal_command("vim", std::path::Path::new("/tmp/`whoami`"));
        assert!(cmd.contains("'"), "backticks should be quoted");

        // $() command substitution
        let cmd = config.build_terminal_command("vim", std::path::Path::new("/tmp/$(rm -rf /)"));
        assert!(cmd.contains("'"), "$() should be quoted");

        // Semicolon injection
        let cmd = config.build_terminal_command("vim", std::path::Path::new("/tmp/; rm -rf /"));
        assert!(cmd.contains("'"), "semicolons should be quoted");
    }

    #[test]
    fn test_build_gui_command_with_flags() {
        let config: EditorConfig = toml::from_str(r#"flags = "--new-window --wait""#).unwrap();
        let cmd = config.build_gui_command("code", std::path::Path::new("/tmp/worktree"));
        assert_eq!(cmd.get_program(), "code");
        // Flags are split by whitespace, so "--new-window" and "--wait" become separate args
        let args: Vec<_> = cmd.get_args().collect();
        assert!(args.contains(&std::ffi::OsStr::new("--new-window")));
        assert!(args.contains(&std::ffi::OsStr::new("--wait")));
    }
}
