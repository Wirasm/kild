//! AppleScript execution utilities for terminal backends.
//!
//! Two layers of helpers:
//! - **Low-level**: `execute_spawn_script`, `close_applescript_window`, `focus_applescript_window`,
//!   `hide_applescript_window` — execute a fully-formed AppleScript string via osascript.
//! - **High-level**: `spawn_via_applescript`, `close_via_applescript`, `focus_via_applescript`,
//!   `hide_via_applescript` — accept a script template with `{window_id}` or `{command}`
//!   placeholders, perform substitution, then delegate to the low-level helpers.

use crate::terminal::errors::TerminalError;
#[cfg(target_os = "macos")]
use tracing::{debug, warn};

/// Execute an AppleScript and return the stdout as window ID.
#[cfg(target_os = "macos")]
pub fn execute_spawn_script(
    script: &str,
    terminal_name: &str,
) -> Result<Option<String>, TerminalError> {
    debug!(
        event = "core.terminal.applescript_executing",
        terminal = terminal_name
    );

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| TerminalError::AppleScriptExecution {
            message: format!("Failed to execute osascript for {}: {}", terminal_name, e),
        })?;

    if !output.status.success() {
        let stderr = super::helpers::stderr_lossy(&output);
        return Err(TerminalError::SpawnFailed {
            message: format!("{} AppleScript failed: {}", terminal_name, stderr),
        });
    }

    let window_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    debug!(
        event = "core.terminal.applescript_completed",
        terminal = terminal_name,
        window_id = %window_id
    );

    let result = if window_id.is_empty() {
        None
    } else {
        Some(window_id)
    };
    Ok(result)
}

/// Close a window via AppleScript (fire-and-forget, errors logged).
#[cfg(target_os = "macos")]
pub fn close_applescript_window(script: &str, terminal_name: &str, window_id: &str) {
    debug!(
        event = "core.terminal.close_started",
        terminal = terminal_name,
        window_id = %window_id
    );

    let output = match std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            warn!(
                event = "core.terminal.close_failed",
                terminal = terminal_name,
                window_id = %window_id,
                error = %e,
                message = "Failed to execute osascript"
            );
            return;
        }
    };

    if output.status.success() {
        debug!(
            event = "core.terminal.close_completed",
            terminal = terminal_name,
            window_id = %window_id
        );
        return;
    }

    let stderr = super::helpers::stderr_lossy(&output);
    warn!(
        event = "core.terminal.close_failed",
        terminal = terminal_name,
        window_id = %window_id,
        stderr = %stderr,
        message = "AppleScript close failed - window may remain open"
    );
}

#[cfg(not(target_os = "macos"))]
pub fn execute_spawn_script(
    _script: &str,
    _terminal_name: &str,
) -> Result<Option<String>, TerminalError> {
    Ok(None)
}

#[cfg(not(target_os = "macos"))]
pub fn close_applescript_window(_script: &str, _terminal_name: &str, _window_id: &str) {}

/// Focus a window via AppleScript.
///
/// Unlike `close_applescript_window` which is fire-and-forget, this returns a Result
/// so callers can report focus failures to the user.
#[cfg(target_os = "macos")]
pub fn focus_applescript_window(
    script: &str,
    terminal_name: &str,
    window_id: &str,
) -> Result<(), TerminalError> {
    use tracing::{error, info};

    debug!(
        event = "core.terminal.focus_started",
        terminal = terminal_name,
        window_id = %window_id
    );

    let output = match std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            error!(
                event = "core.terminal.focus_failed",
                terminal = terminal_name,
                window_id = %window_id,
                error = %e
            );
            return Err(TerminalError::FocusFailed {
                message: format!(
                    "Failed to execute osascript for {} focus: {}",
                    terminal_name, e
                ),
            });
        }
    };

    if output.status.success() {
        info!(
            event = "core.terminal.focus_completed",
            terminal = terminal_name,
            window_id = %window_id
        );
        return Ok(());
    }

    let stderr = super::helpers::stderr_lossy(&output);
    warn!(
        event = "core.terminal.focus_failed",
        terminal = terminal_name,
        window_id = %window_id,
        stderr = %stderr
    );
    Err(TerminalError::FocusFailed {
        message: format!(
            "{} focus failed for window {}: {}",
            terminal_name, window_id, stderr
        ),
    })
}

#[cfg(not(target_os = "macos"))]
pub fn focus_applescript_window(
    _script: &str,
    _terminal_name: &str,
    _window_id: &str,
) -> Result<(), TerminalError> {
    Err(TerminalError::FocusFailed {
        message: "Focus not supported on this platform".to_string(),
    })
}

/// Hide/minimize a window via AppleScript.
///
/// Like `focus_applescript_window`, this returns a Result so callers can report
/// hide failures to the user.
#[cfg(target_os = "macos")]
pub fn hide_applescript_window(
    script: &str,
    terminal_name: &str,
    window_id: &str,
) -> Result<(), TerminalError> {
    use tracing::{error, info};

    debug!(
        event = "core.terminal.hide_started",
        terminal = terminal_name,
        window_id = %window_id
    );

    let output = match std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            error!(
                event = "core.terminal.hide_failed",
                terminal = terminal_name,
                window_id = %window_id,
                error = %e
            );
            return Err(TerminalError::HideFailed {
                message: format!(
                    "Failed to execute osascript for {} hide: {}",
                    terminal_name, e
                ),
            });
        }
    };

    if output.status.success() {
        info!(
            event = "core.terminal.hide_completed",
            terminal = terminal_name,
            window_id = %window_id
        );
        return Ok(());
    }

    let stderr = super::helpers::stderr_lossy(&output);
    warn!(
        event = "core.terminal.hide_failed",
        terminal = terminal_name,
        window_id = %window_id,
        stderr = %stderr
    );
    Err(TerminalError::HideFailed {
        message: format!(
            "{} hide failed for window {}: {}",
            terminal_name, window_id, stderr
        ),
    })
}

#[cfg(not(target_os = "macos"))]
pub fn hide_applescript_window(
    _script: &str,
    _terminal_name: &str,
    _window_id: &str,
) -> Result<(), TerminalError> {
    Err(TerminalError::HideFailed {
        message: "Hide not supported on this platform".to_string(),
    })
}

// --- High-level helpers ---
// These combine script template substitution with the low-level helpers above,
// reducing boilerplate in AppleScript-based terminal backends (iTerm, Terminal.app).

/// Spawn a terminal window using an AppleScript template with `{command}` placeholder.
///
/// Builds a `cd <dir> && <cmd>` shell command from `config`, escapes it for AppleScript,
/// substitutes it into the script template, then executes via `execute_spawn_script`.
#[cfg(target_os = "macos")]
pub fn spawn_via_applescript(
    spawn_script: &str,
    terminal_name: &str,
    config: &crate::terminal::types::SpawnConfig,
) -> Result<Option<String>, TerminalError> {
    let cd_command = super::escape::build_cd_command(config.working_directory(), config.command());
    let script = spawn_script.replace("{command}", &crate::escape::applescript_escape(&cd_command));
    execute_spawn_script(&script, terminal_name)
}

/// Close a window using an AppleScript template with `{window_id}` placeholder.
#[cfg(target_os = "macos")]
pub fn close_via_applescript(close_script: &str, terminal_name: &str, window_id: &str) {
    let script = close_script.replace("{window_id}", window_id);
    close_applescript_window(&script, terminal_name, window_id);
}

/// Focus a window using an AppleScript template with `{window_id}` placeholder.
#[cfg(target_os = "macos")]
pub fn focus_via_applescript(
    focus_script: &str,
    terminal_name: &str,
    window_id: &str,
) -> Result<(), TerminalError> {
    let script = focus_script.replace("{window_id}", window_id);
    focus_applescript_window(&script, terminal_name, window_id)
}

/// Hide/minimize a window using an AppleScript template with `{window_id}` placeholder.
#[cfg(target_os = "macos")]
pub fn hide_via_applescript(
    hide_script: &str,
    terminal_name: &str,
    window_id: &str,
) -> Result<(), TerminalError> {
    let script = hide_script.replace("{window_id}", window_id);
    hide_applescript_window(&script, terminal_name, window_id)
}

#[cfg(not(target_os = "macos"))]
pub fn spawn_via_applescript(
    _spawn_script: &str,
    _terminal_name: &str,
    _config: &crate::terminal::types::SpawnConfig,
) -> Result<Option<String>, TerminalError> {
    Ok(None)
}

#[cfg(not(target_os = "macos"))]
pub fn close_via_applescript(_close_script: &str, _terminal_name: &str, _window_id: &str) {}

#[cfg(not(target_os = "macos"))]
pub fn focus_via_applescript(
    _focus_script: &str,
    _terminal_name: &str,
    _window_id: &str,
) -> Result<(), TerminalError> {
    Err(TerminalError::FocusFailed {
        message: "Focus not supported on this platform".to_string(),
    })
}

#[cfg(not(target_os = "macos"))]
pub fn hide_via_applescript(
    _hide_script: &str,
    _terminal_name: &str,
    _window_id: &str,
) -> Result<(), TerminalError> {
    Err(TerminalError::HideFailed {
        message: "Hide not supported on this platform".to_string(),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_execute_spawn_script_non_macos() {
        #[cfg(not(target_os = "macos"))]
        {
            use super::execute_spawn_script;
            let result = execute_spawn_script("test script", "test_terminal");
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), None);
        }
    }

    #[test]
    fn test_close_applescript_window_non_macos_does_not_panic() {
        #[cfg(not(target_os = "macos"))]
        {
            use super::close_applescript_window;
            // Should not panic on non-macOS
            close_applescript_window("test script", "test_terminal", "123");
        }
    }
}
