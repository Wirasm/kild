//! Shared helper functions and macros for terminal backends.

use tracing::debug;

/// Extract stderr from command output as a trimmed UTF-8 string.
pub fn stderr_lossy(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

/// Check for a window ID before close operations, logging when absent.
///
/// Returns `Some(id)` if a window ID is provided, or `None` after logging
/// a debug event indicating the close was skipped.
pub fn require_window_id<'a>(window_id: Option<&'a str>, terminal_name: &str) -> Option<&'a str> {
    if let Some(id) = window_id {
        return Some(id);
    }

    debug!(
        event = "core.terminal.close_skipped_no_id",
        terminal = terminal_name,
        message = "No window ID available, skipping close to avoid closing wrong window"
    );
    None
}

/// Generate platform-not-supported stub methods for a terminal backend.
///
/// This macro generates `execute_spawn`, `close_window`, `focus_window`, and
/// `hide_window` stubs that are conditionally compiled when the backend's
/// target platform is not active. Must be invoked inside an
/// `impl TerminalBackend for ...` block.
///
/// # Usage
///
/// ```ignore
/// platform_unsupported!(not(target_os = "macos"), "ghostty");
/// ```
macro_rules! platform_unsupported {
    ($cfg_pred:meta, $backend:expr) => {
        #[cfg($cfg_pred)]
        fn execute_spawn(
            &self,
            _config: &crate::terminal::types::SpawnConfig,
            _window_title: Option<&str>,
        ) -> Result<Option<String>, crate::terminal::errors::TerminalError> {
            tracing::debug!(
                event = concat!("core.terminal.spawn_", $backend, "_not_supported"),
                platform = std::env::consts::OS
            );
            Ok(None)
        }

        #[cfg($cfg_pred)]
        fn close_window(&self, _window_id: Option<&str>) {
            tracing::debug!(
                event = "core.terminal.close_not_supported",
                platform = std::env::consts::OS
            );
        }

        #[cfg($cfg_pred)]
        fn focus_window(
            &self,
            _window_id: &str,
        ) -> Result<(), crate::terminal::errors::TerminalError> {
            Err(crate::terminal::errors::TerminalError::FocusFailed {
                message: format!("{} focus not supported on this platform", $backend),
            })
        }

        #[cfg($cfg_pred)]
        fn hide_window(
            &self,
            _window_id: &str,
        ) -> Result<(), crate::terminal::errors::TerminalError> {
            Err(crate::terminal::errors::TerminalError::HideFailed {
                message: format!("{} hide not supported on this platform", $backend),
            })
        }
    };
}

pub(crate) use platform_unsupported;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stderr_lossy_extracts_trimmed() {
        let output = std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: Vec::new(),
            stderr: b"  some error message  \n".to_vec(),
        };
        assert_eq!(stderr_lossy(&output), "some error message");
    }

    #[test]
    fn test_stderr_lossy_empty() {
        let output = std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        assert_eq!(stderr_lossy(&output), "");
    }

    #[test]
    fn test_stderr_lossy_whitespace_only() {
        let output = std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: Vec::new(),
            stderr: b"   \n\t  ".to_vec(),
        };
        assert_eq!(stderr_lossy(&output), "");
    }

    #[test]
    fn test_require_window_id_some() {
        assert_eq!(require_window_id(Some("123"), "test"), Some("123"));
    }

    #[test]
    fn test_require_window_id_none() {
        assert_eq!(require_window_id(None, "test"), None);
    }
}
