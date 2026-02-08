//! Terminal operations - delegates to backend implementations via registry.

use crate::terminal::{errors::TerminalError, registry, types::*};
use std::path::Path;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use tracing::debug;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use tracing::warn;

// Re-export common utilities for external use
pub use crate::terminal::common::escape::{
    applescript_escape, build_cd_command, escape_regex, shell_escape,
};

/// Detect the available terminal.
///
/// Checks terminals in preference order
///   macOs: (Ghostty > iTerm > Terminal.app)
///   Linux: (Alacritty with Hyprland)
pub fn detect_terminal() -> Result<TerminalType, TerminalError> {
    registry::detect_terminal()
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
/// * `window_title` - Optional unique title for window identification
///
/// # Returns
/// * `Ok(Some(window_id))` - Window ID captured successfully
/// * `Ok(None)` - Script succeeded but no window ID captured
/// * `Err(TerminalError)` - Script execution failed
#[cfg(any(target_os = "macos", target_os = "linux"))]
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

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn execute_spawn_script(
    _config: &SpawnConfig,
    _window_title: Option<&str>,
) -> Result<Option<String>, TerminalError> {
    // Terminal spawning not supported on this platform
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
/// * `terminal_type` - The type of terminal (iTerm, Terminal.app, Ghostty, Alacritty)
/// * `window_id` - The window ID or title for window identification
///
/// # Behavior
/// - If window_id is None, skips close (logs debug message)
/// - If window_id is Some, attempts to close that specific window
/// - Close failures are non-fatal and logged at warn level
/// - Returns () because close operations should never block session destruction
#[cfg(any(target_os = "macos", target_os = "linux"))]
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

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn close_terminal_window(_terminal_type: &TerminalType, _window_id: Option<&str>) {
    // Terminal closing not supported on this platform
    debug!(
        event = "core.terminal.close_not_supported",
        platform = std::env::consts::OS
    );
}

/// Focus a terminal window by terminal type and window ID.
///
/// This function delegates to the appropriate backend via the registry.
///
/// # Arguments
/// * `terminal_type` - The type of terminal (iTerm, Terminal.app, Ghostty, Alacritty)
/// * `window_id` - The window ID or title for window identification
///
/// # Returns
/// * `Ok(())` - Window was focused successfully
/// * `Err(TerminalError)` - Focus failed (window not found, permission denied, etc.)
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn focus_terminal_window(
    terminal_type: &TerminalType,
    window_id: &str,
) -> Result<(), TerminalError> {
    let resolved_type = match terminal_type {
        TerminalType::Native => registry::detect_terminal()?,
        t => t.clone(),
    };

    let backend = registry::get_backend(&resolved_type).ok_or(TerminalError::NoTerminalFound)?;
    backend.focus_window(window_id)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn focus_terminal_window(
    _terminal_type: &TerminalType,
    _window_id: &str,
) -> Result<(), TerminalError> {
    Err(TerminalError::FocusFailed {
        message: "Focus not supported on this platform".to_string(),
    })
}

/// Hide/minimize a terminal window by terminal type and window ID.
///
/// This function delegates to the appropriate backend via the registry.
///
/// # Arguments
/// * `terminal_type` - The type of terminal (iTerm, Terminal.app, Ghostty, Alacritty)
/// * `window_id` - The window ID or title for window identification
///
/// # Returns
/// * `Ok(())` - Window was hidden successfully
/// * `Err(TerminalError)` - Hide failed (window not found, permission denied, etc.)
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn hide_terminal_window(
    terminal_type: &TerminalType,
    window_id: &str,
) -> Result<(), TerminalError> {
    let resolved_type = match terminal_type {
        TerminalType::Native => registry::detect_terminal()?,
        t => t.clone(),
    };

    let backend = registry::get_backend(&resolved_type).ok_or(TerminalError::NoTerminalFound)?;
    backend.hide_window(window_id)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn hide_terminal_window(
    _terminal_type: &TerminalType,
    _window_id: &str,
) -> Result<(), TerminalError> {
    Err(TerminalError::HideFailed {
        message: "Hide not supported on this platform".to_string(),
    })
}

/// Check if a terminal window is open.
///
/// Returns `Ok(Some(true/false))` if the terminal supports window detection,
/// or `Ok(None)` if the terminal doesn't support it (use PID-based detection instead).
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn is_terminal_window_open(
    terminal_type: &TerminalType,
    window_id: &str,
) -> Result<Option<bool>, TerminalError> {
    let resolved_type = match terminal_type {
        TerminalType::Native => registry::detect_terminal()?,
        t => t.clone(),
    };

    let backend = registry::get_backend(&resolved_type).ok_or(TerminalError::NoTerminalFound)?;
    backend.is_window_open(window_id)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn is_terminal_window_open(
    _terminal_type: &TerminalType,
    _window_id: &str,
) -> Result<Option<bool>, TerminalError> {
    // Window detection not supported on this platform - use PID-based detection instead
    Ok(None)
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
        close_terminal_window(&TerminalType::Alacritty, None);
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
        assert_eq!(escape_regex("kild-session"), "kild-session");
        assert_eq!(escape_regex("session.1"), "session\\.1");
        assert_eq!(escape_regex("test[0]"), "test\\[0\\]");
        assert_eq!(escape_regex("foo*bar"), "foo\\*bar");
    }

    #[test]
    fn test_ghostty_pkill_pattern_escaping() {
        // Verify the pattern format used in close_terminal_window
        let session_id = "my-kild.test";
        let escaped = escape_regex(session_id);
        let pattern = format!("Ghostty.*{}", escaped);
        // The pattern should escape the dot to avoid matching any character
        assert_eq!(pattern, "Ghostty.*my-kild\\.test");
    }

    #[test]
    fn test_ghostty_ansi_title_escaping() {
        // Verify shell_escape works for ANSI title injection prevention
        let title_with_quotes = "my'kild";
        let escaped = shell_escape(title_with_quotes);
        // Single quotes should be properly escaped
        assert!(escaped.contains("'\"'\"'"));
    }
}
