//! Platform-specific detection utilities.

/// Check if a macOS application exists.
///
/// Checks both running processes and the /Applications directory.
#[cfg(target_os = "macos")]
pub fn app_exists_macos(app_name: &str) -> bool {
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!(
            r#"tell application "System Events" to exists application process "{}""#,
            app_name
        ))
        .output()
        .map(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
        })
        .unwrap_or(false)
        ||
        // Also check if app exists in Applications
        std::process::Command::new("test")
            .arg("-d")
            .arg(format!("/Applications/{}.app", app_name))
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
}

/// Check if a macOS application exists.
///
/// Returns false on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub fn app_exists_macos(_app_name: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_exists_macos_nonexistent() {
        // A clearly nonexistent app should return false
        assert!(!app_exists_macos("NonExistentAppThatDoesNotExist12345"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_app_exists_macos_does_not_panic() {
        // This test just verifies the function doesn't panic
        // The actual result depends on what's installed
        let _ghostty = app_exists_macos("Ghostty");
        let _iterm = app_exists_macos("iTerm");
        let _terminal = app_exists_macos("Terminal");
    }
}
