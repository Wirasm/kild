//! Platform-specific detection utilities.

use std::path::Path;

/// Check if a macOS application exists in /Applications.
///
/// Uses simple filesystem check instead of spawning processes.
#[cfg(target_os = "macos")]
pub fn app_exists_macos(app_name: &str) -> bool {
    Path::new(&format!("/Applications/{}.app", app_name)).exists()
}

/// Check if a macOS application exists.
///
/// Returns false on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub fn app_exists_macos(_app_name: &str) -> bool {
    false
}

/// Check if an application exists on Linux using the `which` command.
///
/// Returns true if the binary is found in PATH.
#[cfg(target_os = "linux")]
pub fn app_exists_linux(app_name: &str) -> bool {
    std::process::Command::new("which")
        .arg(app_name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if an application exists on Linux.
///
/// Returns false on non-Linux platforms.
#[cfg(not(target_os = "linux"))]
pub fn app_exists_linux(_app_name: &str) -> bool {
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

    #[test]
    fn test_app_exists_linux_nonexistent() {
        // A clearly nonexistent app should return false
        assert!(!app_exists_linux("nonexistent-app-12345"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_app_exists_linux_does_not_panic() {
        // This test just verifies the function doesn't panic
        // The actual result depends on what's installed
        let _alacritty = app_exists_linux("alacritty");
        let _hyprctl = app_exists_linux("hyprctl");
        // sh should exist on all Linux systems
        assert!(app_exists_linux("sh"));
    }
}
