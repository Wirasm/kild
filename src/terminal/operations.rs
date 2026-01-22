use crate::terminal::{errors::TerminalError, types::*};
use std::path::Path;
use tracing::{debug, warn};

// AppleScript templates for terminal launching (with window ID capture)
const ITERM_SCRIPT: &str = r#"tell application "iTerm"
        set newWindow to (create window with default profile)
        set windowId to id of newWindow
        tell current session of newWindow
            write text "{command}"
        end tell
        return windowId
    end tell"#;

const TERMINAL_SCRIPT: &str = r#"tell application "Terminal"
        set newTab to do script "{command}"
        set newWindow to window of newTab
        return id of newWindow
    end tell"#;

// Ghostty doesn't support window IDs, so we set a unique title
// The title will be used to find the window when closing
const GHOSTTY_SCRIPT: &str = r#"try
        tell application "Ghostty"
            activate
            delay 0.5
        end tell
        tell application "System Events"
            -- Set unique window title via ANSI escape sequence, then execute command
            keystroke "printf '\\033]2;{window_title}\\007' && {command}"
            keystroke return
        end tell
        return "{window_title}"
    on error errMsg
        error "Failed to launch Ghostty: " & errMsg
    end try"#;

// AppleScript templates for terminal closing (with window ID support)
const ITERM_CLOSE_SCRIPT: &str = r#"tell application "iTerm"
        try
            close window id {window_id}
        on error
            -- Window may already be closed
        end try
    end tell"#;

const TERMINAL_CLOSE_SCRIPT: &str = r#"tell application "Terminal"
        try
            close window id {window_id}
        on error
            -- Window may already be closed
        end try
    end tell"#;

// Ghostty doesn't support window IDs, so we close by title match
const GHOSTTY_CLOSE_SCRIPT: &str = r#"tell application "System Events"
        tell process "Ghostty"
            try
                set targetWindows to (windows whose title contains "{window_title}")
                repeat with targetWindow in targetWindows
                    click button 1 of targetWindow
                end repeat
            on error
                -- Window may already be closed or not found
            end try
        end tell
    end tell"#;

#[cfg(target_os = "macos")]
pub fn detect_terminal() -> Result<TerminalType, TerminalError> {
    debug!(event = "terminal.detection_started");
    
    // Check for Ghostty first (user preference)
    if app_exists_macos("Ghostty") {
        debug!(event = "terminal.detected", terminal = "ghostty");
        Ok(TerminalType::Ghostty)
    } else if app_exists_macos("iTerm") {
        debug!(event = "terminal.detected", terminal = "iterm");
        Ok(TerminalType::ITerm)
    } else if app_exists_macos("Terminal") {
        debug!(event = "terminal.detected", terminal = "terminal");
        Ok(TerminalType::TerminalApp)
    } else {
        warn!(event = "terminal.none_found", checked = "Ghostty,iTerm,Terminal");
        Err(TerminalError::NoTerminalFound)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn detect_terminal() -> Result<TerminalType, TerminalError> {
    warn!(event = "terminal.platform_not_supported", platform = std::env::consts::OS);
    Err(TerminalError::NoTerminalFound)
}

pub fn build_spawn_command(config: &SpawnConfig) -> Result<Vec<String>, TerminalError> {
    if config.command.trim().is_empty() {
        return Err(TerminalError::InvalidCommand);
    }

    if !config.working_directory.exists() {
        return Err(TerminalError::WorkingDirectoryNotFound {
            path: config.working_directory.display().to_string(),
        });
    }

    let cd_command = format!(
        "cd {} && {}",
        shell_escape(&config.working_directory.display().to_string()),
        config.command
    );

    match config.terminal_type {
        TerminalType::ITerm => Ok(vec![
            "osascript".to_string(),
            "-e".to_string(),
            ITERM_SCRIPT.replace("{command}", &applescript_escape(&cd_command)),
        ]),
        TerminalType::TerminalApp => Ok(vec![
            "osascript".to_string(),
            "-e".to_string(),
            TERMINAL_SCRIPT.replace("{command}", &applescript_escape(&cd_command)),
        ]),
        TerminalType::Ghostty => Ok(vec![
            "osascript".to_string(),
            "-e".to_string(),
            // For Ghostty, we don't have a session ID yet in build_spawn_command
            // The window_title will be set when we have the session context
            GHOSTTY_SCRIPT
                .replace("{command}", &applescript_escape(&cd_command))
                .replace("{window_title}", "shards-session"),
        ]),
        TerminalType::Native => {
            // Use system default (detect and delegate)
            let detected = detect_terminal()?;
            if detected == TerminalType::Native {
                return Err(TerminalError::NoTerminalFound);
            }
            let native_config = SpawnConfig::new(detected, config.working_directory.clone(), config.command.clone());
            build_spawn_command(&native_config)
        }
    }
}

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

fn app_exists_macos(app_name: &str) -> bool {
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!(r#"tell application "System Events" to exists application process "{}""#, app_name))
        .output()
        .map(|output| {
            output.status.success() &&
            String::from_utf8_lossy(&output.stdout).trim() == "true"
        })
        .unwrap_or(false) ||
    // Also check if app exists in Applications
    std::process::Command::new("test")
        .arg("-d")
        .arg(format!("/Applications/{}.app", app_name))
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Extract the executable name from a command string
pub fn extract_command_name(command: &str) -> String {
    command.split_whitespace().next().unwrap_or(command).to_string()
}

/// Build and execute the spawn AppleScript, capturing the returned window ID
///
/// This function builds the spawn command using `build_spawn_command`, executes it
/// via osascript, and captures the window ID returned by the script.
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
    if config.command.trim().is_empty() {
        return Err(TerminalError::InvalidCommand);
    }

    if !config.working_directory.exists() {
        return Err(TerminalError::WorkingDirectoryNotFound {
            path: config.working_directory.display().to_string(),
        });
    }

    let cd_command = format!(
        "cd {} && {}",
        shell_escape(&config.working_directory.display().to_string()),
        config.command
    );

    let script = match config.terminal_type {
        TerminalType::ITerm => {
            ITERM_SCRIPT.replace("{command}", &applescript_escape(&cd_command))
        }
        TerminalType::TerminalApp => {
            TERMINAL_SCRIPT.replace("{command}", &applescript_escape(&cd_command))
        }
        TerminalType::Ghostty => {
            let title = window_title.unwrap_or("shards-session");
            GHOSTTY_SCRIPT
                .replace("{command}", &applescript_escape(&cd_command))
                .replace("{window_title}", title)
        }
        TerminalType::Native => {
            let detected = detect_terminal()?;
            if detected == TerminalType::Native {
                return Err(TerminalError::NoTerminalFound);
            }
            let native_config = SpawnConfig::new(
                detected,
                config.working_directory.clone(),
                config.command.clone(),
            );
            return execute_spawn_script(&native_config, window_title);
        }
    };

    debug!(
        event = "terminal.spawn_script_executing",
        terminal_type = %config.terminal_type,
        working_directory = %config.working_directory.display()
    );

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| TerminalError::AppleScriptExecution {
            message: format!("Failed to execute spawn script: {}", e),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TerminalError::SpawnFailed {
            message: format!("AppleScript failed: {}", stderr.trim()),
        });
    }

    let window_id = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string();

    debug!(
        event = "terminal.spawn_script_completed",
        terminal_type = %config.terminal_type,
        window_id = %window_id
    );

    if window_id.is_empty() {
        Ok(None)
    } else {
        Ok(Some(window_id))
    }
}

#[cfg(not(target_os = "macos"))]
pub fn execute_spawn_script(
    _config: &SpawnConfig,
    _window_title: Option<&str>,
) -> Result<Option<String>, TerminalError> {
    // Terminal spawning with window ID capture not yet implemented for non-macOS platforms
    debug!(event = "terminal.spawn_script_not_supported", platform = std::env::consts::OS);
    Ok(None)
}

/// Close a terminal window by terminal type and window ID
///
/// Uses AppleScript (macOS) to close the specific window identified by window_id.
/// If no window_id is provided, the close is skipped to avoid closing the wrong window.
/// This is a best-effort operation - it will not fail if the window is already closed.
///
/// # Arguments
/// * `terminal_type` - The type of terminal (iTerm, Terminal.app, Ghostty)
/// * `window_id` - The window ID (for iTerm/Terminal.app) or title (for Ghostty)
///
/// # Behavior
/// - If window_id is None, skips close (logs debug message)
/// - If window_id is Some, attempts to close that specific window
/// - AppleScript failures are non-fatal and logged as debug
#[cfg(target_os = "macos")]
pub fn close_terminal_window(
    terminal_type: &TerminalType,
    window_id: Option<&str>,
) -> Result<(), TerminalError> {
    // If no window ID, skip close to avoid closing the wrong window
    let Some(id) = window_id else {
        debug!(
            event = "terminal.close_skipped_no_id",
            terminal_type = %terminal_type,
            message = "No window ID available, skipping close to avoid closing wrong window"
        );
        return Ok(());
    };

    let script = match terminal_type {
        TerminalType::ITerm => ITERM_CLOSE_SCRIPT.replace("{window_id}", id),
        TerminalType::TerminalApp => TERMINAL_CLOSE_SCRIPT.replace("{window_id}", id),
        TerminalType::Ghostty => GHOSTTY_CLOSE_SCRIPT.replace("{window_title}", id),
        TerminalType::Native => {
            // For Native, try to detect what terminal is running
            let detected = detect_terminal()?;
            return close_terminal_window(&detected, window_id);
        }
    };

    debug!(
        event = "terminal.close_started",
        terminal_type = %terminal_type,
        window_id = %id
    );

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| TerminalError::AppleScriptExecution {
            message: format!("Failed to execute close script: {}", e),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // All AppleScript failures are non-fatal - terminal close should never block destroy.
        // Common cases: window already closed, app not running, permission issues.
        debug!(
            event = "terminal.close_failed_non_fatal",
            terminal_type = %terminal_type,
            window_id = %id,
            stderr = %stderr.trim(),
            message = "Terminal close failed - continuing with destroy"
        );
        return Ok(());
    }

    debug!(
        event = "terminal.close_completed",
        terminal_type = %terminal_type,
        window_id = %id
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn close_terminal_window(
    _terminal_type: &TerminalType,
    _window_id: Option<&str>,
) -> Result<(), TerminalError> {
    // Terminal closing not yet implemented for non-macOS platforms
    debug!(event = "terminal.close_not_supported", platform = std::env::consts::OS);
    Ok(())
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
    fn test_close_terminal_scripts_defined() {
        // Verify close scripts are non-empty
        assert!(!ITERM_CLOSE_SCRIPT.is_empty());
        assert!(!TERMINAL_CLOSE_SCRIPT.is_empty());
        assert!(!GHOSTTY_CLOSE_SCRIPT.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore] // DANGEROUS: Actually closes iTerm windows via AppleScript - run manually only
    fn test_close_terminal_window_graceful_fallback() {
        // WARNING: This test executes real AppleScript that closes the specified iTerm window!
        // Only run manually when no important iTerm windows are open.
        //
        // Closing a non-existent window ID should not error - tests graceful fallback behavior.
        let result = close_terminal_window(&TerminalType::ITerm, Some("99999"));
        // Should succeed even if no iTerm window with that ID exists
        assert!(result.is_ok());
    }

    #[test]
    fn test_close_terminal_window_skips_when_no_id() {
        // When window_id is None, close should be skipped to avoid closing wrong window
        let result = close_terminal_window(&TerminalType::ITerm, None);
        assert!(result.is_ok());

        let result = close_terminal_window(&TerminalType::TerminalApp, None);
        assert!(result.is_ok());

        let result = close_terminal_window(&TerminalType::Ghostty, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_spawn_scripts_have_window_id_return() {
        // Verify spawn scripts have the expected return statements
        assert!(ITERM_SCRIPT.contains("return windowId"));
        assert!(TERMINAL_SCRIPT.contains("return id of newWindow"));
        assert!(GHOSTTY_SCRIPT.contains("return \"{window_title}\""));
    }

    #[test]
    fn test_close_scripts_have_window_id_placeholders() {
        // Verify close scripts have the expected placeholders
        assert!(ITERM_CLOSE_SCRIPT.contains("{window_id}"));
        assert!(TERMINAL_CLOSE_SCRIPT.contains("{window_id}"));
        assert!(GHOSTTY_CLOSE_SCRIPT.contains("{window_title}"));
    }
}
