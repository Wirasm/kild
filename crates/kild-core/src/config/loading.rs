//! Configuration loading and merging logic.
//!
//! This module handles loading configuration from files and merging
//! configurations from different sources (user config, project config).
//!
//! # Configuration Hierarchy
//!
//! Configuration is loaded in the following order (later sources override earlier ones):
//! 1. **Hardcoded defaults** - Built-in fallback values
//! 2. **User config** - `~/.kild/config.toml` (global user preferences)
//! 3. **Project config** - `./.kild/config.toml` (project-specific overrides)
//! 4. **CLI arguments** - Command-line flags (highest priority)

use crate::agents;
use crate::config::types::{AgentConfig, GitConfig, HealthConfig, KildConfig, TerminalConfig};
use crate::config::validation::validate_config;
use crate::files::types::IncludeConfig;
use std::fs;
use std::path::PathBuf;

/// Check if an error is a "file not found" error.
fn is_file_not_found(e: &(dyn std::error::Error + 'static)) -> bool {
    if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
        return io_err.kind() == std::io::ErrorKind::NotFound;
    }

    let err_str = e.to_string();
    err_str.contains("No such file or directory") || err_str.contains("cannot find the path")
}

/// Load configuration from the hierarchy of config files.
///
/// Loads and merges configuration from:
/// 1. Default values
/// 2. User config (`~/.kild/config.toml`)
/// 3. Project config (`./.kild/config.toml`)
///
/// # Errors
///
/// Returns an error if validation fails. Missing config files are not errors.
pub fn load_hierarchy() -> Result<KildConfig, Box<dyn std::error::Error>> {
    let mut config = KildConfig::default();

    // Load user config (file not found is expected, parse errors fail)
    match load_user_config() {
        Ok(user_config) => config = merge_configs(config, user_config),
        Err(e) if !is_file_not_found(e.as_ref()) => return Err(e),
        Err(_) => {} // File not found - continue with defaults
    }

    // Load project config (file not found is expected, parse errors fail)
    match load_project_config() {
        Ok(project_config) => config = merge_configs(config, project_config),
        Err(e) if !is_file_not_found(e.as_ref()) => return Err(e),
        Err(_) => {} // File not found - continue with merged config
    }

    // Validate the final configuration
    validate_config(&config)?;

    Ok(config)
}

/// Load the user configuration from ~/.kild/config.toml.
fn load_user_config() -> Result<KildConfig, Box<dyn std::error::Error>> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let config_path = home_dir.join(".kild").join("config.toml");
    load_config_file(&config_path)
}

/// Load the project configuration from ./.kild/config.toml.
fn load_project_config() -> Result<KildConfig, Box<dyn std::error::Error>> {
    let config_path = std::env::current_dir()?.join(".kild").join("config.toml");
    load_config_file(&config_path)
}

/// Load a configuration file from the given path.
fn load_config_file(path: &PathBuf) -> Result<KildConfig, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read config file '{}': {}", path.display(), e))?;
    let config: KildConfig = toml::from_str(&content)
        .map_err(|e| format!("Failed to parse config file '{}': {}", path.display(), e))?;
    Ok(config)
}

/// Merge include pattern configurations.
///
/// When both configs have patterns, combines and deduplicates them.
/// Override config wins for enabled and max_file_size settings.
fn merge_include_patterns(
    base: Option<IncludeConfig>,
    override_config: Option<IncludeConfig>,
) -> Option<IncludeConfig> {
    match (base, override_config) {
        (Some(base_config), Some(override_cfg)) => {
            // Both configs present - merge patterns and use override settings
            let mut merged_patterns = base_config.patterns;
            for pattern in override_cfg.patterns {
                if !merged_patterns.contains(&pattern) {
                    merged_patterns.push(pattern);
                }
            }
            Some(IncludeConfig {
                patterns: merged_patterns,
                enabled: override_cfg.enabled,
                max_file_size: override_cfg.max_file_size.or(base_config.max_file_size),
            })
        }
        (None, Some(override_cfg)) => Some(override_cfg),
        (Some(base_config), None) => Some(base_config),
        (None, None) => None,
    }
}

/// Merge two configurations, with override_config taking precedence.
///
/// For optional fields, override values replace base values only if present.
/// For collections (like agents HashMap), entries are merged with override taking precedence.
pub fn merge_configs(base: KildConfig, override_config: KildConfig) -> KildConfig {
    KildConfig {
        agent: AgentConfig {
            // Always use override agent if it was explicitly set in the config file
            // We can't distinguish between explicit "claude" and default "claude" here,
            // so we always prefer the override config's agent setting
            default: override_config.agent.default,
            startup_command: override_config
                .agent
                .startup_command
                .or(base.agent.startup_command),
            flags: override_config.agent.flags.or(base.agent.flags),
        },
        terminal: TerminalConfig {
            preferred: override_config
                .terminal
                .preferred
                .or(base.terminal.preferred),
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
        include_patterns: merge_include_patterns(
            base.include_patterns,
            override_config.include_patterns,
        ),
        health: HealthConfig {
            idle_threshold_minutes: override_config
                .health
                .idle_threshold_minutes
                .or(base.health.idle_threshold_minutes),
            refresh_interval_secs: override_config
                .health
                .refresh_interval_secs
                .or(base.health.refresh_interval_secs),
            history_enabled: override_config.health.history_enabled || base.health.history_enabled,
            history_retention_days: override_config
                .health
                .history_retention_days
                .or(base.health.history_retention_days),
        },
        git: GitConfig {
            remote: override_config.git.remote.or(base.git.remote),
            base_branch: override_config.git.base_branch.or(base.git.base_branch),
            fetch_before_create: override_config
                .git
                .fetch_before_create
                .or(base.git.fetch_before_create),
        },
    }
}

/// Get the command to run for a specific agent.
///
/// Resolution order:
/// 1. Agent-specific settings from `[agents.<name>]` section
/// 2. Global agent config from `[agent]` section
/// 3. Built-in default command for the agent
///
/// # Errors
///
/// Returns an error if no command can be determined for the agent (unknown agent
/// with no configured startup_command).
pub fn get_agent_command(
    config: &KildConfig,
    agent_name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    // Check agent-specific settings first
    if let Some(agent_settings) = config.agents.get(agent_name)
        && let Some(command) = &agent_settings.startup_command
    {
        let mut full_command = command.clone();
        if let Some(flags) = &agent_settings.flags {
            full_command.push(' ');
            full_command.push_str(flags);
        }
        return Ok(full_command);
    }

    // Fall back to global agent config or built-in default
    let base_command = if let Some(cmd) = &config.agent.startup_command {
        cmd.as_str()
    } else {
        agents::get_default_command(agent_name).ok_or_else(|| {
            format!(
                "No command found for agent '{}'. Configure a startup_command in your config file \
                or use a known agent ({}).",
                agent_name,
                agents::supported_agents_string()
            )
        })?
    };

    let mut full_command = base_command.to_string();
    if let Some(flags) = &config.agent.flags {
        full_command.push(' ');
        full_command.push_str(flags);
    }

    Ok(full_command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::AgentSettings;
    use std::env;
    use std::fs;

    #[test]
    fn test_get_agent_command_defaults() {
        let config = KildConfig::default();

        assert_eq!(get_agent_command(&config, "claude").unwrap(), "claude");
        assert_eq!(get_agent_command(&config, "kiro").unwrap(), "kiro-cli chat");
        assert_eq!(get_agent_command(&config, "gemini").unwrap(), "gemini");
    }

    #[test]
    fn test_get_agent_command_unknown_agent_fails() {
        let config = KildConfig::default();

        let result = get_agent_command(&config, "unknown");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No command found"));
    }

    #[test]
    fn test_get_agent_command_with_flags() {
        let mut config = KildConfig::default();
        config.agent.flags = Some("--yolo".to_string());

        assert_eq!(
            get_agent_command(&config, "claude").unwrap(),
            "claude --yolo"
        );
    }

    #[test]
    fn test_get_agent_command_specific_agent() {
        let mut config = KildConfig::default();
        let agent_settings = AgentSettings {
            startup_command: Some("cc".to_string()),
            flags: Some("--dangerous".to_string()),
        };
        config.agents.insert("claude".to_string(), agent_settings);

        assert_eq!(
            get_agent_command(&config, "claude").unwrap(),
            "cc --dangerous"
        );
        assert_eq!(get_agent_command(&config, "kiro").unwrap(), "kiro-cli chat");
    }

    #[test]
    fn test_get_agent_command_unknown_with_custom_command() {
        let mut config = KildConfig::default();
        let agent_settings = AgentSettings {
            startup_command: Some("my-custom-agent".to_string()),
            flags: None,
        };
        config.agents.insert("custom".to_string(), agent_settings);

        // Unknown agent with configured command should succeed
        assert_eq!(
            get_agent_command(&config, "custom").unwrap(),
            "my-custom-agent"
        );
    }

    #[test]
    fn test_config_hierarchy_integration() {
        // Create temporary directories for testing
        let temp_dir = env::temp_dir().join("kild_config_test");
        let user_config_dir = temp_dir.join("user");
        let project_config_dir = temp_dir.join("project");

        // Clean up any existing test directories
        let _ = fs::remove_dir_all(&temp_dir);

        // Create test directories
        fs::create_dir_all(&user_config_dir).unwrap();
        fs::create_dir_all(&project_config_dir.join(".kild")).unwrap();

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
        fs::write(
            project_config_dir.join(".kild").join("config.toml"),
            project_config_content,
        )
        .unwrap();

        // Test loading user config
        let user_config = load_config_file(&user_config_dir.join("config.toml")).unwrap();
        assert_eq!(user_config.agent.default, "kiro");
        assert_eq!(
            user_config.agent.startup_command,
            Some("kiro-cli chat".to_string())
        );
        assert_eq!(user_config.terminal.preferred, Some("iterm2".to_string()));

        // Test loading project config
        let project_config =
            load_config_file(&project_config_dir.join(".kild").join("config.toml")).unwrap();
        assert_eq!(project_config.agent.default, "claude");
        assert_eq!(project_config.agent.flags, Some("--yolo".to_string()));

        // Test merging configs (project overrides user)
        let merged = merge_configs(user_config, project_config);
        assert_eq!(merged.agent.default, "claude"); // Overridden by project
        assert_eq!(
            merged.agent.startup_command,
            Some("kiro-cli chat".to_string())
        ); // From user
        assert_eq!(merged.agent.flags, Some("--yolo".to_string())); // From project
        assert_eq!(merged.terminal.preferred, Some("iterm2".to_string())); // From user

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_toml_parsing_edge_cases() {
        // Test empty config
        let empty_config: KildConfig = toml::from_str("").unwrap();
        assert_eq!(empty_config.agent.default, "claude");

        // Test partial config
        let partial_config: KildConfig = toml::from_str(
            r#"
[terminal]
preferred = "iterm2"
"#,
        )
        .unwrap();
        assert_eq!(partial_config.agent.default, "claude"); // Should use default
        assert_eq!(
            partial_config.terminal.preferred,
            Some("iterm2".to_string())
        );

        // Test invalid TOML should fail
        let invalid_result: Result<KildConfig, _> = toml::from_str("invalid toml [[[");
        assert!(invalid_result.is_err());
    }

    #[test]
    fn test_health_config_from_toml() {
        let config: KildConfig = toml::from_str(
            r#"
[health]
idle_threshold_minutes = 5
history_enabled = true
"#,
        )
        .unwrap();
        assert_eq!(config.health.idle_threshold_minutes(), 5);
        assert!(config.health.history_enabled);
        // Defaults should still apply for unspecified fields
        assert_eq!(config.health.refresh_interval_secs(), 5);
        assert_eq!(config.health.history_retention_days(), 7);
    }

    #[test]
    fn test_health_config_merge() {
        let user_config: KildConfig = toml::from_str(
            r#"
[health]
idle_threshold_minutes = 15
history_retention_days = 30
"#,
        )
        .unwrap();

        // Project config with only history_enabled set
        let project_config: KildConfig = toml::from_str(
            r#"
[health]
history_enabled = true
"#,
        )
        .unwrap();

        let merged = merge_configs(user_config, project_config);

        // User-set values should be preserved when project doesn't override
        assert_eq!(merged.health.idle_threshold_minutes(), 15);
        assert_eq!(merged.health.history_retention_days(), 30);
        // Project-set values should be used
        assert!(merged.health.history_enabled);
    }

    #[test]
    fn test_terminal_config_merge_always_takes_override() {
        // Documents current behavior: terminal spawn_delay_ms and max_retry_attempts
        // always take the override config's value (even if it's the default).
        // This is a known limitation - user config values can be overwritten by
        // project config defaults when project config lacks a [terminal] section.
        let user_config: KildConfig = toml::from_str(
            r#"
[terminal]
spawn_delay_ms = 2000
max_retry_attempts = 10
"#,
        )
        .unwrap();

        // Project config with no terminal section - will have serde defaults (1000, 5)
        let project_config: KildConfig = toml::from_str(
            r#"
[agent]
default = "claude"
"#,
        )
        .unwrap();

        let merged = merge_configs(user_config, project_config);

        // Current behavior: project config's defaults (1000, 5) override user's (2000, 10)
        // This documents the limitation rather than testing ideal behavior
        assert_eq!(
            merged.terminal.spawn_delay_ms, 1000,
            "current behavior: override config always wins, even if it's a default"
        );
        assert_eq!(
            merged.terminal.max_retry_attempts, 5,
            "current behavior: override config always wins, even if it's a default"
        );
    }

    #[test]
    fn test_include_patterns_merge_combines_arrays() {
        let user_config: KildConfig = toml::from_str(
            r#"
[include_patterns]
patterns = [".env*", "user-specific/**"]
"#,
        )
        .unwrap();

        let project_config: KildConfig = toml::from_str(
            r#"
[include_patterns]
patterns = [".env*", "project-specific/**"]
"#,
        )
        .unwrap();

        let merged = merge_configs(user_config, project_config);
        let patterns = &merged.include_patterns.unwrap().patterns;

        // Base patterns come first, then override patterns (deduplicated)
        assert_eq!(patterns.len(), 3);
        assert!(patterns.contains(&".env*".to_string()));
        assert!(patterns.contains(&"user-specific/**".to_string()));
        assert!(patterns.contains(&"project-specific/**".to_string()));
    }

    #[test]
    fn test_include_patterns_merge_override_wins_for_enabled() {
        let user_config: KildConfig = toml::from_str(
            r#"
[include_patterns]
enabled = true
patterns = [".env*"]
"#,
        )
        .unwrap();

        let project_config: KildConfig = toml::from_str(
            r#"
[include_patterns]
enabled = false
patterns = []
"#,
        )
        .unwrap();

        let merged = merge_configs(user_config, project_config);
        let include = merged.include_patterns.unwrap();

        assert!(!include.enabled); // Project disabled wins
        assert!(include.patterns.contains(&".env*".to_string())); // But patterns still merged
    }

    #[test]
    fn test_include_patterns_default_has_patterns() {
        let config = KildConfig::default();
        let include = config
            .include_patterns
            .expect("default config should have include_patterns set");

        assert!(include.enabled);
        assert_eq!(include.patterns.len(), 4);
        assert!(include.patterns.contains(&".env*".to_string()));
        assert!(include.patterns.contains(&"*.local.json".to_string()));
        assert!(include.patterns.contains(&".claude/**".to_string()));
        assert!(include.patterns.contains(&".cursor/**".to_string()));
    }

    #[test]
    fn test_include_patterns_user_only_preserved() {
        let user_config: KildConfig = toml::from_str(
            r#"
[include_patterns]
patterns = [".env*", "custom/**"]
"#,
        )
        .unwrap();

        // Project config without include_patterns section
        let project_config: KildConfig = toml::from_str(
            r#"
[agent]
default = "claude"
"#,
        )
        .unwrap();

        let merged = merge_configs(user_config, project_config);
        let include = merged.include_patterns.unwrap();

        // User patterns should be preserved
        assert!(include.patterns.contains(&".env*".to_string()));
        assert!(include.patterns.contains(&"custom/**".to_string()));
    }

    #[test]
    fn test_include_patterns_max_file_size_merge() {
        let user_config: KildConfig = toml::from_str(
            r#"
[include_patterns]
patterns = [".env*"]
max_file_size = "10MB"
"#,
        )
        .unwrap();

        let project_config: KildConfig = toml::from_str(
            r#"
[include_patterns]
patterns = ["*.local.json"]
"#,
        )
        .unwrap();

        let merged = merge_configs(user_config, project_config);
        let include = merged.include_patterns.unwrap();

        // max_file_size from user should be preserved when project doesn't specify
        assert_eq!(include.max_file_size, Some("10MB".to_string()));
    }

    #[test]
    fn test_include_patterns_defaults_merge_with_user_config() {
        // Default config (simulating no files loaded)
        let base = KildConfig::default();

        // User config adds one pattern
        let user_config: KildConfig = toml::from_str(
            r#"
[include_patterns]
patterns = ["custom.txt"]
"#,
        )
        .unwrap();

        let merged = merge_configs(base, user_config);
        let patterns = &merged.include_patterns.unwrap().patterns;

        // Should have defaults (4) + user pattern (1) = 5
        assert_eq!(patterns.len(), 5);
        assert!(patterns.contains(&".env*".to_string())); // from default
        assert!(patterns.contains(&"*.local.json".to_string())); // from default
        assert!(patterns.contains(&".claude/**".to_string())); // from default
        assert!(patterns.contains(&".cursor/**".to_string())); // from default
        assert!(patterns.contains(&"custom.txt".to_string())); // from user
    }

    #[test]
    fn test_include_patterns_empty_array_merges_with_base() {
        // Document behavior: empty array in override does NOT clear base patterns
        // This is intentional - to disable patterns entirely, set enabled = false
        let user_config: KildConfig = toml::from_str(
            r#"
[include_patterns]
patterns = [".env*", "user/**"]
"#,
        )
        .unwrap();

        // Project explicitly sets empty patterns
        let project_config: KildConfig = toml::from_str(
            r#"
[include_patterns]
patterns = []
"#,
        )
        .unwrap();

        let merged = merge_configs(user_config, project_config);
        let include = merged.include_patterns.unwrap();

        // Empty array from project means "no additional patterns"
        // User patterns are preserved (merge semantics, not replace)
        assert_eq!(include.patterns.len(), 2);
        assert!(include.patterns.contains(&".env*".to_string()));
        assert!(include.patterns.contains(&"user/**".to_string()));
    }

    #[test]
    fn test_include_patterns_merge_preserves_order() {
        let user_config: KildConfig = toml::from_str(
            r#"
[include_patterns]
patterns = ["a", "b", "c"]
"#,
        )
        .unwrap();

        let project_config: KildConfig = toml::from_str(
            r#"
[include_patterns]
patterns = ["b", "d", "e"]
"#,
        )
        .unwrap();

        let merged = merge_configs(user_config, project_config);
        let patterns = &merged.include_patterns.unwrap().patterns;

        // Base order preserved, then new patterns appended (b is deduplicated)
        assert_eq!(
            patterns,
            &vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string(),
                "e".to_string(),
            ]
        );
    }

    #[test]
    fn test_include_patterns_max_file_size_override() {
        let user_config: KildConfig = toml::from_str(
            r#"
[include_patterns]
patterns = [".env*"]
max_file_size = "10MB"
"#,
        )
        .unwrap();

        let project_config: KildConfig = toml::from_str(
            r#"
[include_patterns]
patterns = ["*.json"]
max_file_size = "5MB"
"#,
        )
        .unwrap();

        let merged = merge_configs(user_config, project_config);
        let include = merged.include_patterns.unwrap();

        // Project override should win
        assert_eq!(include.max_file_size, Some("5MB".to_string()));
    }

    #[test]
    fn test_include_patterns_deserializes_with_defaults() {
        // Empty config should use serde default for include_patterns
        let config: KildConfig = toml::from_str("").unwrap();

        let include = config
            .include_patterns
            .expect("empty config should have default include_patterns");
        assert!(include.enabled);
        assert_eq!(include.patterns.len(), 4);
    }

    #[test]
    fn test_include_patterns_field_level_defaults() {
        // Partial include_patterns should use field-level defaults for patterns
        let config: KildConfig = toml::from_str(
            r#"
[include_patterns]
enabled = false
"#,
        )
        .unwrap();

        let include = config.include_patterns.unwrap();
        assert!(!include.enabled); // explicitly set
        assert_eq!(include.patterns.len(), 4); // should use field default
        assert!(include.patterns.contains(&".env*".to_string()));
    }

    #[test]
    fn test_git_config_merge() {
        let user_config: KildConfig = toml::from_str(
            r#"
[git]
remote = "upstream"
base_branch = "develop"
"#,
        )
        .unwrap();

        let project_config: KildConfig = toml::from_str(
            r#"
[git]
base_branch = "main"
"#,
        )
        .unwrap();

        let merged = merge_configs(user_config, project_config);
        // Project overrides base_branch
        assert_eq!(merged.git.base_branch(), "main");
        // User's "upstream" is preserved because project doesn't set remote
        assert_eq!(merged.git.remote(), "upstream");
    }

    #[test]
    fn test_git_config_merge_defaults_preserved() {
        let base = KildConfig::default();
        let override_config: KildConfig = toml::from_str(
            r#"
[agent]
default = "claude"
"#,
        )
        .unwrap();

        let merged = merge_configs(base, override_config);
        // Git defaults should be preserved when override doesn't specify git section
        assert_eq!(merged.git.remote(), "origin");
        assert_eq!(merged.git.base_branch(), "main");
        assert!(merged.git.fetch_before_create());
    }
}
