//! Hyprland IPC utilities for terminal window management.
//!
//! This module provides utilities for interacting with Hyprland compositor
//! via `hyprctl` commands. Used by the Alacritty backend on Linux.

use crate::terminal::errors::TerminalError;
use tracing::debug;
#[cfg(target_os = "linux")]
use tracing::warn;

/// Check if Hyprland is available and running.
///
/// Returns true if `hyprctl version` succeeds.
#[cfg(target_os = "linux")]
pub fn is_hyprland_available() -> bool {
    match std::process::Command::new("hyprctl")
        .arg("version")
        .output()
    {
        Ok(output) => output.status.success(),
        Err(e) => {
            warn!(
                event = "core.terminal.hyprland_detection_failed",
                error = %e,
            );
            false
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub fn is_hyprland_available() -> bool {
    false
}

/// Focus a window by its title using Hyprland IPC.
///
/// Uses `hyprctl dispatch focuswindow title:X` to focus the window.
#[cfg(target_os = "linux")]
pub fn focus_window_by_title(title: &str) -> Result<(), TerminalError> {
    debug!(
        event = "core.terminal.hyprland_focus_started",
        title = %title
    );

    let output = std::process::Command::new("hyprctl")
        .arg("dispatch")
        .arg("focuswindow")
        .arg(format!("title:{}", title))
        .output()
        .map_err(|e| TerminalError::HyprlandIpcFailed {
            message: format!("Failed to execute hyprctl: {}", e),
        })?;

    if output.status.success() {
        debug!(
            event = "core.terminal.hyprland_focus_completed",
            title = %title
        );
        return Ok(());
    }

    let stderr = super::helpers::stderr_lossy(&output);
    warn!(
        event = "core.terminal.hyprland_focus_failed",
        title = %title,
        stderr = %stderr
    );
    Err(TerminalError::FocusFailed {
        message: format!("Hyprland focus failed for '{}': {}", title, stderr),
    })
}

#[cfg(not(target_os = "linux"))]
pub fn focus_window_by_title(_title: &str) -> Result<(), TerminalError> {
    Err(TerminalError::FocusFailed {
        message: "Hyprland focus not supported on this platform".to_string(),
    })
}

/// Hide a window by its title using Hyprland IPC.
///
/// Uses `hyprctl dispatch movetoworkspacesilent special` to move the window
/// to the special (hidden) workspace.
#[cfg(target_os = "linux")]
pub fn hide_window_by_title(title: &str) -> Result<(), TerminalError> {
    debug!(
        event = "core.terminal.hyprland_hide_started",
        title = %title
    );

    let output = std::process::Command::new("hyprctl")
        .arg("dispatch")
        .arg("movetoworkspacesilent")
        .arg(format!("special,title:{}", title))
        .output()
        .map_err(|e| TerminalError::HyprlandIpcFailed {
            message: format!("Failed to execute hyprctl: {}", e),
        })?;

    if output.status.success() {
        debug!(
            event = "core.terminal.hyprland_hide_completed",
            title = %title
        );
        return Ok(());
    }

    let stderr = super::helpers::stderr_lossy(&output);
    warn!(
        event = "core.terminal.hyprland_hide_failed",
        title = %title,
        stderr = %stderr
    );
    Err(TerminalError::HideFailed {
        message: format!("Hyprland hide failed for '{}': {}", title, stderr),
    })
}

#[cfg(not(target_os = "linux"))]
pub fn hide_window_by_title(_title: &str) -> Result<(), TerminalError> {
    Err(TerminalError::HideFailed {
        message: "Hyprland hide not supported on this platform".to_string(),
    })
}

/// Close a window by its title using Hyprland IPC.
///
/// Uses `hyprctl dispatch closewindow title:X` to close the window.
/// This is a fire-and-forget operation - errors are logged but not returned.
#[cfg(target_os = "linux")]
pub fn close_window_by_title(title: &str) {
    debug!(
        event = "core.terminal.hyprland_close_started",
        title = %title
    );

    let output = match std::process::Command::new("hyprctl")
        .arg("dispatch")
        .arg("closewindow")
        .arg(format!("title:{}", title))
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            warn!(
                event = "core.terminal.hyprland_close_exec_failed",
                title = %title,
                error = %e,
                message = "Failed to execute hyprctl - window may remain open"
            );
            return;
        }
    };

    if output.status.success() {
        debug!(
            event = "core.terminal.hyprland_close_completed",
            title = %title
        );
        return;
    }

    let stderr = super::helpers::stderr_lossy(&output);
    warn!(
        event = "core.terminal.hyprland_close_failed",
        title = %title,
        stderr = %stderr,
        message = "hyprctl closewindow failed - window may remain open"
    );
}

#[cfg(not(target_os = "linux"))]
pub fn close_window_by_title(_title: &str) {
    debug!(
        event = "core.terminal.hyprland_close_not_supported",
        platform = std::env::consts::OS
    );
}

/// Check if a window with the given title exists using Hyprland IPC.
///
/// Uses `hyprctl clients -j` to query all windows and searches for a matching title.
///
/// **Note:** Uses substring matching (`.contains()`) rather than exact match. This
/// allows finding windows where the title may have additional context appended by
/// the application. For KILD's use case with unique session-based titles (e.g.,
/// "kild-feature-auth"), this is sufficient and avoids false negatives from title
/// variations.
///
/// # Returns
/// - `Ok(Some(true))` - Window with matching title substring exists
/// - `Ok(Some(false))` - No window contains the title substring
/// - `Ok(None)` - Cannot determine (hyprctl failed to parse)
/// - `Err(...)` - hyprctl execution failed
#[cfg(target_os = "linux")]
pub fn window_exists_by_title(title: &str) -> Result<Option<bool>, TerminalError> {
    debug!(
        event = "core.terminal.hyprland_window_check_started",
        title = %title
    );

    let output = std::process::Command::new("hyprctl")
        .arg("clients")
        .arg("-j")
        .output()
        .map_err(|e| TerminalError::HyprlandIpcFailed {
            message: format!("Failed to execute hyprctl clients: {}", e),
        })?;

    if !output.status.success() {
        let stderr = super::helpers::stderr_lossy(&output);
        warn!(
            event = "core.terminal.hyprland_window_check_failed",
            title = %title,
            stderr = %stderr
        );
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output to find windows with matching title
    // The JSON is an array of client objects, each with a "title" field
    let clients = match serde_json::from_str::<Vec<HyprlandClient>>(&stdout) {
        Ok(clients) => clients,
        Err(e) => {
            warn!(
                event = "core.terminal.hyprland_window_check_parse_failed",
                title = %title,
                error = %e,
                message = "Failed to parse hyprctl clients JSON"
            );
            return Ok(None);
        }
    };

    let match_count = clients.iter().filter(|c| c.title.contains(title)).count();
    if match_count > 1 {
        warn!(
            event = "core.terminal.hyprland_window_multiple_matches",
            title = %title,
            match_count = match_count,
        );
    }

    let found = match_count > 0;
    debug!(
        event = "core.terminal.hyprland_window_check_completed",
        title = %title,
        found = found,
        client_count = clients.len()
    );
    Ok(Some(found))
}

#[cfg(not(target_os = "linux"))]
pub fn window_exists_by_title(_title: &str) -> Result<Option<bool>, TerminalError> {
    Ok(None)
}

/// Hyprland client (window) information from `hyprctl clients -j`.
///
/// Note: Fields are read via serde deserialization, not direct access.
#[cfg(target_os = "linux")]
#[derive(Debug, serde::Deserialize)]
struct HyprlandClient {
    /// Window title
    title: String,
    // Other fields exist but we only need title for window matching
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hyprland_available_does_not_panic() {
        // Just verify the function doesn't panic
        let _available = is_hyprland_available();
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn test_hyprland_not_available_on_non_linux() {
        assert!(!is_hyprland_available());
    }

    #[test]
    fn test_hide_window_does_not_panic() {
        // hide_window_by_title returns Result - should not panic on any platform
        let _result = hide_window_by_title("nonexistent-window-12345");
    }

    #[test]
    fn test_close_window_does_not_panic() {
        // close_window_by_title is fire-and-forget, should never panic
        close_window_by_title("nonexistent-window-12345");
    }

    #[test]
    fn test_window_exists_nonexistent() {
        let result = window_exists_by_title("nonexistent-window-12345");
        match result {
            // Non-Linux returns Ok(None), Linux with Hyprland returns Ok(Some(false))
            Ok(value) => assert!(value.is_none() || value == Some(false)),
            // Linux without Hyprland: hyprctl not available
            Err(_) => {}
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_hyprland_client_parsing() {
        let json = r#"[
            {"title": "kild-test-session", "class": "Alacritty"},
            {"title": "Firefox", "class": "firefox"}
        ]"#;

        let clients: Vec<HyprlandClient> = serde_json::from_str(json).unwrap();
        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].title, "kild-test-session");
        assert_eq!(clients[1].title, "Firefox");

        // Test title matching logic
        assert!(clients.iter().any(|c| c.title.contains("kild-test")));
        assert!(!clients.iter().any(|c| c.title.contains("nonexistent")));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_hyprland_client_parsing_malformed_json() {
        // Truncated JSON (missing closing bracket)
        let truncated = r#"[{"title": "test"}"#;
        let result: Result<Vec<HyprlandClient>, _> = serde_json::from_str(truncated);
        assert!(result.is_err());

        // Corrupt JSON (invalid syntax)
        let corrupt = r#"[{"title": }]"#;
        let result: Result<Vec<HyprlandClient>, _> = serde_json::from_str(corrupt);
        assert!(result.is_err());

        // Missing required field
        let missing_title = r#"[{"class": "Alacritty"}]"#;
        let result: Result<Vec<HyprlandClient>, _> = serde_json::from_str(missing_title);
        assert!(result.is_err());

        // Empty array is valid
        let empty = "[]";
        let result: Result<Vec<HyprlandClient>, _> = serde_json::from_str(empty);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
