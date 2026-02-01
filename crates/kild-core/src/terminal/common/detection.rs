//! Platform-specific detection utilities.

#[cfg(target_os = "macos")]
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
