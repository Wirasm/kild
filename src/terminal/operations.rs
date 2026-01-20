use crate::terminal::{errors::TerminalError, types::*};
use std::path::Path;
use tracing::{debug, warn};

// AppleScript templates for terminal launching
const ITERM_SCRIPT: &str = r#"tell application "iTerm"
        create window with default profile
        tell current session of current window
            write text "{}"
        end tell
    end tell"#;

const TERMINAL_SCRIPT: &str = r#"tell application "Terminal"
        do script "{}"
    end tell"#;

const GHOSTTY_SCRIPT: &str = r#"try
        tell application "Ghostty"
            activate
            delay 0.5
        end tell
        tell application "System Events"
            keystroke "{}"
            keystroke return
        end tell
    on error errMsg
        error "Failed to launch Ghostty: " & errMsg
    end try"#;

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
            ITERM_SCRIPT.replace("{}", &applescript_escape(&cd_command)),
        ]),
        TerminalType::TerminalApp => Ok(vec![
            "osascript".to_string(),
            "-e".to_string(),
            TERMINAL_SCRIPT.replace("{}", &applescript_escape(&cd_command)),
        ]),
        TerminalType::Ghostty => Ok(vec![
            "osascript".to_string(),
            "-e".to_string(),
            GHOSTTY_SCRIPT.replace("{}", &applescript_escape(&cd_command)),
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
}
