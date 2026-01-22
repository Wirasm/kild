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

// Note: Ghostty window closing uses 'pkill -f' to match process command line
// arguments (including the session title) because AppleScript window title
// matching via System Events doesn't work reliably for Ghostty windows.

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
        warn!(
            event = "terminal.none_found",
            checked = "Ghostty,iTerm,Terminal"
        );
        Err(TerminalError::NoTerminalFound)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn detect_terminal() -> Result<TerminalType, TerminalError> {
    warn!(
        event = "terminal.platform_not_supported",
        platform = std::env::consts::OS
    );
    Err(TerminalError::NoTerminalFound)
}

/// Build a shell command that changes to the working directory and executes the command
fn build_cd_command(working_directory: &std::path::Path, command: &str) -> String {
    format!(
        "cd {} && {}",
        shell_escape(&working_directory.display().to_string()),
        command
    )
}

/// Escape special regex characters for use in pkill -f pattern
fn escape_regex(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        match c {
            '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' | '\\' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

pub fn build_spawn_command(config: &SpawnConfig) -> Result<Vec<String>, TerminalError> {
    config.validate()?;

    let cd_command = build_cd_command(&config.working_directory, &config.command);

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
                config.working_directory.clone(),
                config.command.clone(),
            );
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
    command
        .split_whitespace()
        .next()
        .unwrap_or(command)
        .to_string()
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
    config.validate()?;

    let cd_command = build_cd_command(&config.working_directory, &config.command);

    // Ghostty on macOS requires 'open -na Ghostty.app --args' to spawn new windows
    if config.terminal_type == TerminalType::Ghostty {
        let title = window_title.unwrap_or("shards-session");
        // Shell-escape the title to prevent injection if it contains special characters
        let escaped_title = shell_escape(title);
        // Set window title via ANSI escape sequence (OSC 2) for later process identification.
        // Format: \033]2;title\007 - ESC ] 2 ; title BEL
        // This title is embedded in the command line, allowing pkill -f to match it.
        let ghostty_command = format!(
            "printf '\\033]2;'{}'\\007' && {}",
            escaped_title, cd_command
        );

        debug!(
            event = "terminal.spawn_ghostty_starting",
            terminal_type = %config.terminal_type,
            working_directory = %config.working_directory.display(),
            window_title = %title
        );

        let status = std::process::Command::new("open")
            .arg("-na")
            .arg("Ghostty.app")
            .arg("--args")
            .arg("-e")
            .arg("sh")
            .arg("-c")
            .arg(&ghostty_command)
            .status()
            .map_err(|e| TerminalError::SpawnFailed {
                message: format!(
                    "Failed to spawn Ghostty (title='{}', cwd='{}', cmd='{}'): {}",
                    title,
                    config.working_directory.display(),
                    config.command,
                    e
                ),
            })?;

        if !status.success() {
            return Err(TerminalError::SpawnFailed {
                message: format!(
                    "Ghostty launch failed with exit code: {:?} (title='{}', cwd='{}', cmd='{}')",
                    status.code(),
                    title,
                    config.working_directory.display(),
                    config.command
                ),
            });
        }

        debug!(
            event = "terminal.spawn_ghostty_launched",
            terminal_type = %config.terminal_type,
            window_title = %title,
            message = "open command completed successfully, Ghostty window should be visible"
        );

        // Return window_title as identifier for close_terminal_window
        return Ok(Some(title.to_string()));
    }

    let script = match config.terminal_type {
        TerminalType::ITerm => ITERM_SCRIPT.replace("{command}", &applescript_escape(&cd_command)),
        TerminalType::TerminalApp => {
            TERMINAL_SCRIPT.replace("{command}", &applescript_escape(&cd_command))
        }
        TerminalType::Ghostty => unreachable!("Handled above"),
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

    let window_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

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
    debug!(
        event = "terminal.spawn_script_not_supported",
        platform = std::env::consts::OS
    );
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

    // Ghostty: AppleScript window title matching doesn't work reliably.
    // Instead, find and kill the Ghostty process by matching command line args.
    if *terminal_type == TerminalType::Ghostty {
        debug!(
            event = "terminal.close_ghostty_pkill",
            window_title = %id
        );

        // Escape regex metacharacters in the window title to avoid matching wrong processes
        let escaped_id = escape_regex(id);
        // Use pkill to kill Ghostty processes that contain our session identifier
        let result = std::process::Command::new("pkill")
            .arg("-f")
            .arg(format!("Ghostty.*{}", escaped_id))
            .output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    debug!(
                        event = "terminal.close_ghostty_completed",
                        window_title = %id
                    );
                } else {
                    // Log at warn level so this appears in production logs
                    // This is expected if the terminal was manually closed by the user
                    warn!(
                        event = "terminal.close_ghostty_no_match",
                        window_title = %id,
                        message = "No matching Ghostty process found - terminal may have been closed manually"
                    );
                }
            }
            Err(e) => {
                // Log at warn level so this appears in production logs
                warn!(
                    event = "terminal.close_ghostty_failed",
                    window_title = %id,
                    error = %e,
                    message = "pkill command failed - terminal window may remain open"
                );
            }
        }
        return Ok(());
    }

    let script = match terminal_type {
        TerminalType::ITerm => ITERM_CLOSE_SCRIPT.replace("{window_id}", id),
        TerminalType::TerminalApp => TERMINAL_CLOSE_SCRIPT.replace("{window_id}", id),
        TerminalType::Ghostty => unreachable!("Handled above"),
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
        // Log at warn level so this appears in production logs for debugging.
        warn!(
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
    debug!(
        event = "terminal.close_not_supported",
        platform = std::env::consts::OS
    );
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
        // We can't create a real directory with quotes, so just verify the escaping logic
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
        // Note: Ghostty uses pkill instead of AppleScript, so no script to check
        assert!(!ITERM_CLOSE_SCRIPT.is_empty());
        assert!(!TERMINAL_CLOSE_SCRIPT.is_empty());
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
        // Note: Ghostty uses direct execution (not AppleScript), so no script to check
        assert!(ITERM_SCRIPT.contains("return windowId"));
        assert!(TERMINAL_SCRIPT.contains("return id of newWindow"));
    }

    #[test]
    fn test_close_scripts_have_window_id_placeholders() {
        // Verify close scripts have the expected placeholders
        // Note: Ghostty uses pkill instead of AppleScript, so no script to check
        assert!(ITERM_CLOSE_SCRIPT.contains("{window_id}"));
        assert!(TERMINAL_CLOSE_SCRIPT.contains("{window_id}"));
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
