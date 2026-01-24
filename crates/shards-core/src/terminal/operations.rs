//! Terminal operations - delegates to backend implementations via registry.

use crate::terminal::{errors::TerminalError, registry, types::*};
use std::path::Path;

#[cfg(not(target_os = "macos"))]
use tracing::debug;
#[cfg(target_os = "macos")]
use tracing::warn;

// Re-export common utilities for backward compatibility and external use
pub use crate::terminal::common::escape::{
    applescript_escape, build_cd_command, escape_regex, shell_escape,
};

/// Detect the available terminal.
///
/// Checks terminals in preference order (Ghostty > iTerm > Terminal.app)
/// and returns the first available one.
#[cfg(target_os = "macos")]
pub fn detect_terminal() -> Result<TerminalType, TerminalError> {
    registry::detect_terminal()
}

#[cfg(not(target_os = "macos"))]
pub fn detect_terminal() -> Result<TerminalType, TerminalError> {
    registry::detect_terminal()
}

/// Build the spawn command arguments for a terminal.
///
/// Note: This function is kept for backward compatibility, but the actual
/// spawn logic is now in the backend implementations.
#[deprecated(
    since = "0.2.0",
    note = "Use execute_spawn_script() instead. This function duplicates backend logic and will be removed in a future version."
)]
pub fn build_spawn_command(config: &SpawnConfig) -> Result<Vec<String>, TerminalError> {
    config.validate()?;

    let cd_command = build_cd_command(config.working_directory(), config.command());

    match config.terminal_type() {
        TerminalType::ITerm => Ok(vec![
            "osascript".to_string(),
            "-e".to_string(),
            format!(
                r#"tell application "iTerm"
        set newWindow to (create window with default profile)
        set windowId to id of newWindow
        tell current session of newWindow
            write text "{}"
        end tell
        return windowId
    end tell"#,
                applescript_escape(&cd_command)
            ),
        ]),
        TerminalType::TerminalApp => Ok(vec![
            "osascript".to_string(),
            "-e".to_string(),
            format!(
                r#"tell application "Terminal"
        set newTab to do script "{}"
        set newWindow to window of newTab
        return id of newWindow
    end tell"#,
                applescript_escape(&cd_command)
            ),
        ]),
        TerminalType::Ghostty => {
            // On macOS, the ghostty CLI spawns headless processes, not GUI windows.
            // Must use 'open -na Ghostty.app --args' where:
            //   -n opens a new instance, -a specifies the application
            // Arguments after --args are passed to Ghostty's -e flag for command execution.
            Ok(vec![
                "open".to_string(),
                "-na".to_string(),
                "Ghostty.app".to_string(),
                "--args".to_string(),
                "-e".to_string(),
                "sh".to_string(),
                "-c".to_string(),
                cd_command,
            ])
        }
        TerminalType::Native => {
            // Use system default (detect and delegate)
            let detected = detect_terminal()?;
            if detected == TerminalType::Native {
                return Err(TerminalError::NoTerminalFound);
            }
            let native_config = SpawnConfig::new(
                detected,
                config.working_directory().to_path_buf(),
                config.command().to_string(),
            );
            #[allow(deprecated)]
            build_spawn_command(&native_config)
        }
    }
}

/// Validate that a working directory exists and is a directory.
pub fn validate_working_directory(path: &Path) -> Result<(), TerminalError> {
    if !path.exists() {
        return Err(TerminalError::WorkingDirectoryNotFound {
            path: path.display().to_string(),
        });
    }

    if !path.is_dir() {
        return Err(TerminalError::WorkingDirectoryNotFound {
            path: path.display().to_string(),
        });
    }

    Ok(())
}

/// Extract the executable name from a command string.
pub fn extract_command_name(command: &str) -> String {
    command
        .split_whitespace()
        .next()
        .unwrap_or(command)
        .to_string()
}

/// Build and execute the spawn script, capturing the returned window ID.
///
/// This function delegates to the appropriate backend via the registry.
///
/// # Arguments
/// * `config` - The spawn configuration
/// * `window_title` - Optional unique title for Ghostty (used as "window ID")
///
/// # Returns
/// * `Ok(Some(window_id))` - Window ID captured successfully
/// * `Ok(None)` - Script succeeded but no window ID captured
/// * `Err(TerminalError)` - Script execution failed
#[cfg(target_os = "macos")]
pub fn execute_spawn_script(
    config: &SpawnConfig,
    window_title: Option<&str>,
) -> Result<Option<String>, TerminalError> {
    config.validate()?;

    // Resolve Native to actual terminal type
    let terminal_type = match config.terminal_type() {
        TerminalType::Native => registry::detect_terminal()?,
        t => t.clone(),
    };

    let backend = registry::get_backend(&terminal_type).ok_or(TerminalError::NoTerminalFound)?;

    // Create config with resolved terminal type
    let resolved_config = SpawnConfig::new(
        terminal_type,
        config.working_directory().to_path_buf(),
        config.command().to_string(),
    );

    backend.execute_spawn(&resolved_config, window_title)
}

#[cfg(not(target_os = "macos"))]
pub fn execute_spawn_script(
    _config: &SpawnConfig,
    _window_title: Option<&str>,
) -> Result<Option<String>, TerminalError> {
    // Terminal spawning with window ID capture not yet implemented for non-macOS platforms
    debug!(
        event = "core.terminal.spawn_script_not_supported",
        platform = std::env::consts::OS
    );
    Ok(None)
}

/// Close a terminal window by terminal type and window ID (fire-and-forget).
///
/// This function delegates to the appropriate backend via the registry.
///
/// # Arguments
/// * `terminal_type` - The type of terminal (iTerm, Terminal.app, Ghostty)
/// * `window_id` - The window ID (for iTerm/Terminal.app) or title (for Ghostty)
///
/// # Behavior
/// - If window_id is None, skips close (logs debug message)
/// - If window_id is Some, attempts to close that specific window
/// - Close failures are non-fatal and logged at warn level
/// - Returns () because close operations should never block session destruction
#[cfg(target_os = "macos")]
pub fn close_terminal_window(terminal_type: &TerminalType, window_id: Option<&str>) {
    // Resolve Native to actual terminal type
    let resolved_type = match terminal_type {
        TerminalType::Native => match registry::detect_terminal() {
            Ok(t) => t,
            Err(e) => {
                warn!(
                    event = "core.terminal.close_detect_failed",
                    error = %e,
                    "Could not detect terminal type during close - window may remain open"
                );
                return;
            }
        },
        t => t.clone(),
    };

    let Some(backend) = registry::get_backend(&resolved_type) else {
        warn!(
            event = "core.terminal.close_no_backend",
            terminal_type = %resolved_type,
            "No backend found for terminal type - window may remain open"
        );
        return;
    };

    backend.close_window(window_id);
}

#[cfg(not(target_os = "macos"))]
pub fn close_terminal_window(_terminal_type: &TerminalType, _window_id: Option<&str>) {
    // Terminal closing not yet implemented for non-macOS platforms
    debug!(
        event = "core.terminal.close_not_supported",
        platform = std::env::consts::OS
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_terminal() {
        // This test depends on the system, so we just ensure it doesn't panic
        let _result = detect_terminal();
    }

    #[allow(deprecated)]
    #[test]
    fn test_build_spawn_command_iterm() {
        let config = SpawnConfig::new(
            TerminalType::ITerm,
            std::env::current_dir().unwrap(),
            "cc".to_string(),
        );

        let result = build_spawn_command(&config);
        assert!(result.is_ok());

        let command = result.unwrap();
        assert_eq!(command[0], "osascript");
        assert!(command[2].contains("iTerm"));
        assert!(command[2].contains("cc"));
    }

    #[allow(deprecated)]
    #[test]
    fn test_build_spawn_command_terminal_app() {
        let config = SpawnConfig::new(
            TerminalType::TerminalApp,
            std::env::current_dir().unwrap(),
            "kiro-cli".to_string(),
        );

        let result = build_spawn_command(&config);
        assert!(result.is_ok());

        let command = result.unwrap();
        assert_eq!(command[0], "osascript");
        assert!(command[2].contains("Terminal"));
        assert!(command[2].contains("kiro-cli"));
    }

    #[allow(deprecated)]
    #[test]
    fn test_build_spawn_command_ghostty() {
        let config = SpawnConfig::new(
            TerminalType::Ghostty,
            std::env::current_dir().unwrap(),
            "claude".to_string(),
        );

        let result = build_spawn_command(&config);
        assert!(result.is_ok());

        let command = result.unwrap();
        // On macOS, Ghostty requires: open -na Ghostty.app --args -e sh -c "..."
        assert_eq!(command[0], "open");
        assert_eq!(command[1], "-na");
        assert_eq!(command[2], "Ghostty.app");
        assert_eq!(command[3], "--args");
        assert_eq!(command[4], "-e");
        assert_eq!(command[5], "sh");
        assert_eq!(command[6], "-c");
        assert!(command[7].contains("claude"));
    }

    #[allow(deprecated)]
    #[test]
    fn test_build_spawn_command_ghostty_with_spaces() {
        let config = SpawnConfig::new(
            TerminalType::Ghostty,
            std::env::current_dir().unwrap(),
            "kiro-cli chat --verbose".to_string(),
        );

        let result = build_spawn_command(&config);
        assert!(result.is_ok());

        let command = result.unwrap();
        // Command should be in the shell command argument (index 7)
        assert!(command[7].contains("kiro-cli chat --verbose"));
    }

    #[test]
    fn test_build_spawn_command_ghostty_path_with_single_quote() {
        // Test that paths with single quotes are properly escaped
        let escaped = shell_escape("/Users/foo's dir/project");
        // The escaping should handle the single quote correctly
        assert!(escaped.contains("foo"));
        assert!(escaped.contains("dir"));
        // Should use the shell escaping pattern for single quotes
        assert!(escaped.contains("'\"'\"'"));
    }

    #[test]
    fn test_shell_escape_handles_metacharacters() {
        // Verify shell escaping handles various special characters
        assert_eq!(shell_escape("path with spaces"), "'path with spaces'");
        assert_eq!(shell_escape("$HOME/dir"), "'$HOME/dir'");
        assert_eq!(shell_escape("dir;rm -rf /"), "'dir;rm -rf /'");
        assert_eq!(shell_escape("$(whoami)"), "'$(whoami)'");
        assert_eq!(shell_escape("`id`"), "'`id`'");
    }

    #[allow(deprecated)]
    #[test]
    fn test_build_spawn_command_empty_command() {
        let config = SpawnConfig::new(
            TerminalType::ITerm,
            std::env::current_dir().unwrap(),
            "".to_string(),
        );

        let result = build_spawn_command(&config);
        assert!(matches!(result, Err(TerminalError::InvalidCommand)));
    }

    #[allow(deprecated)]
    #[test]
    fn test_build_spawn_command_nonexistent_directory() {
        let config = SpawnConfig::new(
            TerminalType::ITerm,
            PathBuf::from("/nonexistent/directory"),
            "echo hello".to_string(),
        );

        let result = build_spawn_command(&config);
        assert!(matches!(
            result,
            Err(TerminalError::WorkingDirectoryNotFound { .. })
        ));
    }

    #[test]
    fn test_validate_working_directory() {
        let current_dir = std::env::current_dir().unwrap();
        assert!(validate_working_directory(&current_dir).is_ok());

        let nonexistent = PathBuf::from("/nonexistent/directory");
        assert!(validate_working_directory(&nonexistent).is_err());
    }

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("hello"), "'hello'");
        assert_eq!(shell_escape("hello world"), "'hello world'");
        assert_eq!(shell_escape("hello'world"), "'hello'\"'\"'world'");
    }

    #[test]
    fn test_applescript_escape() {
        assert_eq!(applescript_escape("hello"), "hello");
        assert_eq!(applescript_escape("hello\"world"), "hello\\\"world");
        assert_eq!(applescript_escape("hello\\world"), "hello\\\\world");
        assert_eq!(applescript_escape("hello\nworld"), "hello\\nworld");
    }

    #[test]
    fn test_extract_command_name() {
        assert_eq!(extract_command_name("kiro-cli chat"), "kiro-cli");
        assert_eq!(extract_command_name("claude-code"), "claude-code");
        assert_eq!(extract_command_name("  cc  "), "cc");
        assert_eq!(extract_command_name("echo hello world"), "echo");
    }

    #[test]
    fn test_close_terminal_window_skips_when_no_id() {
        // When window_id is None, close should be skipped to avoid closing wrong window
        // close_terminal_window returns () - just verify it doesn't panic
        close_terminal_window(&TerminalType::ITerm, None);
        close_terminal_window(&TerminalType::TerminalApp, None);
        close_terminal_window(&TerminalType::Ghostty, None);
    }

    #[test]
    fn test_build_cd_command() {
        let path = PathBuf::from("/tmp/test");
        let command = "echo hello";
        let result = build_cd_command(&path, command);
        assert!(result.contains("cd '/tmp/test'"));
        assert!(result.contains("&& echo hello"));
    }

    #[test]
    fn test_build_cd_command_with_spaces() {
        let path = PathBuf::from("/tmp/test with spaces");
        let command = "claude code";
        let result = build_cd_command(&path, command);
        assert!(result.contains("cd '/tmp/test with spaces'"));
        assert!(result.contains("&& claude code"));
    }

    #[test]
    fn test_escape_regex_simple() {
        assert_eq!(escape_regex("hello"), "hello");
        assert_eq!(escape_regex("hello-world"), "hello-world");
        assert_eq!(escape_regex("hello_world_123"), "hello_world_123");
    }

    #[test]
    fn test_escape_regex_metacharacters() {
        // Test all regex metacharacters are escaped
        assert_eq!(escape_regex("."), "\\.");
        assert_eq!(escape_regex("*"), "\\*");
        assert_eq!(escape_regex("+"), "\\+");
        assert_eq!(escape_regex("?"), "\\?");
        assert_eq!(escape_regex("("), "\\(");
        assert_eq!(escape_regex(")"), "\\)");
        assert_eq!(escape_regex("["), "\\[");
        assert_eq!(escape_regex("]"), "\\]");
        assert_eq!(escape_regex("{"), "\\{");
        assert_eq!(escape_regex("}"), "\\}");
        assert_eq!(escape_regex("|"), "\\|");
        assert_eq!(escape_regex("^"), "\\^");
        assert_eq!(escape_regex("$"), "\\$");
        assert_eq!(escape_regex("\\"), "\\\\");
    }

    #[test]
    fn test_escape_regex_mixed() {
        // Test realistic session identifiers with potential metacharacters
        assert_eq!(escape_regex("shards-session"), "shards-session");
        assert_eq!(escape_regex("session.1"), "session\\.1");
        assert_eq!(escape_regex("test[0]"), "test\\[0\\]");
        assert_eq!(escape_regex("foo*bar"), "foo\\*bar");
    }

    #[test]
    fn test_ghostty_pkill_pattern_escaping() {
        // Verify the pattern format used in close_terminal_window
        let session_id = "my-shard.test";
        let escaped = escape_regex(session_id);
        let pattern = format!("Ghostty.*{}", escaped);
        // The pattern should escape the dot to avoid matching any character
        assert_eq!(pattern, "Ghostty.*my-shard\\.test");
    }

    #[test]
    fn test_ghostty_ansi_title_escaping() {
        // Verify shell_escape works for ANSI title injection prevention
        let title_with_quotes = "my'shard";
        let escaped = shell_escape(title_with_quotes);
        // Single quotes should be properly escaped
        assert!(escaped.contains("'\"'\"'"));
    }
}
