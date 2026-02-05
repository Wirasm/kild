//! Alacritty terminal backend implementation for Linux with Hyprland.
//!
//! This backend uses Alacritty as the terminal emulator and Hyprland IPC
//! for window management. Both Alacritty and Hyprland are required.

use tracing::debug;

use crate::terminal::{
    common::{detection::app_exists_linux, hyprland},
    errors::TerminalError,
    traits::TerminalBackend,
    types::SpawnConfig,
};

#[cfg(target_os = "linux")]
use crate::terminal::common::escape::build_cd_command;

/// Backend implementation for Alacritty terminal on Linux with Hyprland.
pub struct AlacrittyBackend;

impl TerminalBackend for AlacrittyBackend {
    fn name(&self) -> &'static str {
        "alacritty"
    }

    fn display_name(&self) -> &'static str {
        "Alacritty"
    }

    fn is_available(&self) -> bool {
        // Both Alacritty and Hyprland are required
        let alacritty = app_exists_linux("alacritty");
        let hyprland = hyprland::is_hyprland_available();

        debug!(
            event = "core.terminal.alacritty_availability_checked",
            alacritty_available = alacritty,
            hyprland_available = hyprland
        );

        alacritty && hyprland
    }

    #[cfg(target_os = "linux")]
    fn execute_spawn(
        &self,
        config: &SpawnConfig,
        window_title: Option<&str>,
    ) -> Result<Option<String>, TerminalError> {
        let cd_command = build_cd_command(config.working_directory(), config.command());
        let title = window_title.unwrap_or("kild-session");

        debug!(
            event = "core.terminal.spawn_alacritty_started",
            terminal_type = %config.terminal_type(),
            working_directory = %config.working_directory().display(),
            window_title = %title
        );

        // Spawn Alacritty with:
        // --title X : Set window title for Hyprland to identify
        // -e sh -c "cd /path && command" : Execute command in shell
        //
        // IMPORTANT: Redirect stdin/stdout/stderr to null to fully detach the terminal.
        // Without this, the child process inherits kild's file descriptors, causing
        // issues when kild's own stdin/stdout/stderr are used (e.g., piped input).
        // This mirrors macOS behavior where `open -n` launches apps fully detached.
        let child = std::process::Command::new("alacritty")
            .arg("--title")
            .arg(title)
            .arg("-e")
            .arg("sh")
            .arg("-c")
            .arg(&cd_command)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| TerminalError::SpawnFailed {
                message: format!(
                    "Failed to spawn Alacritty (title='{}', cwd='{}', cmd='{}'): {}",
                    title,
                    config.working_directory().display(),
                    config.command(),
                    e
                ),
            })?;

        debug!(
            event = "core.terminal.spawn_alacritty_completed",
            terminal_type = %config.terminal_type(),
            window_title = %title,
            pid = child.id(),
            message = "Alacritty process spawned, window should be visible"
        );

        // Return the resolved window title for use as identifier in close_window/focus_window
        Ok(Some(title.to_string()))
    }

    #[cfg(not(target_os = "linux"))]
    fn execute_spawn(
        &self,
        _config: &SpawnConfig,
        _window_title: Option<&str>,
    ) -> Result<Option<String>, TerminalError> {
        debug!(
            event = "core.terminal.spawn_alacritty_not_supported",
            platform = std::env::consts::OS
        );
        Ok(None)
    }

    #[cfg(target_os = "linux")]
    fn close_window(&self, window_id: Option<&str>) {
        let Some(id) = window_id else {
            debug!(
                event = "core.terminal.close_skipped_no_id",
                terminal = "alacritty",
                message = "No window ID available, skipping close to avoid closing wrong window"
            );
            return;
        };

        debug!(
            event = "core.terminal.close_alacritty_started",
            window_title = %id
        );

        // Use Hyprland IPC to close the window by title
        hyprland::close_window_by_title(id);
    }

    #[cfg(not(target_os = "linux"))]
    fn close_window(&self, _window_id: Option<&str>) {
        debug!(
            event = "core.terminal.close_not_supported",
            platform = std::env::consts::OS
        );
    }

    #[cfg(target_os = "linux")]
    fn focus_window(&self, window_id: &str) -> Result<(), TerminalError> {
        debug!(
            event = "core.terminal.focus_alacritty_started",
            window_id = %window_id
        );

        // Use Hyprland IPC to focus the window by title
        hyprland::focus_window_by_title(window_id)
    }

    #[cfg(not(target_os = "linux"))]
    fn focus_window(&self, _window_id: &str) -> Result<(), TerminalError> {
        Err(TerminalError::FocusFailed {
            message: "Alacritty focus not supported on this platform".to_string(),
        })
    }

    #[cfg(target_os = "linux")]
    fn hide_window(&self, window_id: &str) -> Result<(), TerminalError> {
        debug!(
            event = "core.terminal.hide_alacritty_started",
            window_id = %window_id
        );

        hyprland::hide_window_by_title(window_id)
    }

    #[cfg(not(target_os = "linux"))]
    fn hide_window(&self, _window_id: &str) -> Result<(), TerminalError> {
        Err(TerminalError::HideFailed {
            message: "Alacritty hide not supported on this platform".to_string(),
        })
    }

    #[cfg(target_os = "linux")]
    fn is_window_open(&self, window_id: &str) -> Result<Option<bool>, TerminalError> {
        debug!(
            event = "core.terminal.alacritty_window_check_started",
            window_title = %window_id
        );

        // Use Hyprland IPC to check if window exists by title
        hyprland::window_exists_by_title(window_id)
    }

    #[cfg(not(target_os = "linux"))]
    fn is_window_open(&self, _window_id: &str) -> Result<Option<bool>, TerminalError> {
        // Non-Linux: window detection not supported
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_alacritty_backend_name() {
        let backend = AlacrittyBackend;
        assert_eq!(backend.name(), "alacritty");
    }

    #[test]
    fn test_alacritty_backend_display_name() {
        let backend = AlacrittyBackend;
        assert_eq!(backend.display_name(), "Alacritty");
    }

    #[test]
    fn test_alacritty_close_window_skips_when_no_id() {
        let backend = AlacrittyBackend;
        // close_window returns () - just verify it doesn't panic
        backend.close_window(None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_alacritty_spawn_command_structure() {
        // Verify the structure of what would be passed to alacritty
        let config = SpawnConfig::new(
            crate::terminal::types::TerminalType::Alacritty,
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
        );

        let _title = "kild-test-session";
        let cd_command = build_cd_command(config.working_directory(), config.command());

        // Command should contain the path and command
        assert!(cd_command.contains("/tmp/test"));
        assert!(cd_command.contains("claude"));
    }

    #[test]
    fn test_is_window_open_returns_option_type() {
        let backend = AlacrittyBackend;
        let result = backend.is_window_open("nonexistent-window-title");
        match result {
            // Non-Linux returns Ok(None), Linux with Hyprland returns Ok(Some(false))
            Ok(value) => assert!(value.is_none() || value == Some(false)),
            // Linux without Hyprland: hyprctl not available
            Err(_) => {}
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn test_alacritty_not_available_on_non_linux() {
        let backend = AlacrittyBackend;
        assert!(!backend.is_available());
    }
}
