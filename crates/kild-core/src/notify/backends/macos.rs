//! macOS notification backend using osascript (Notification Center).

use crate::escape::applescript_escape;
use crate::notify::errors::NotifyError;
use crate::notify::traits::NotificationBackend;

/// macOS notification backend via `osascript` (Notification Center).
pub struct MacOsNotificationBackend;

impl NotificationBackend for MacOsNotificationBackend {
    fn name(&self) -> &'static str {
        "macos"
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos")
    }

    fn send(&self, title: &str, message: &str) -> Result<(), NotifyError> {
        let escaped_title = applescript_escape(title);
        let escaped_message = applescript_escape(message);
        let script = format!(
            r#"display notification "{}" with title "{}""#,
            escaped_message, escaped_title
        );

        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| NotifyError::SendFailed {
                message: format!("osascript exec failed: {}", e),
            })?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(NotifyError::SendFailed {
                message: format!("osascript exit {}: {}", output.status, stderr.trim()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_backend_name() {
        let backend = MacOsNotificationBackend;
        assert_eq!(backend.name(), "macos");
    }

    #[test]
    fn macos_backend_availability_matches_platform() {
        let backend = MacOsNotificationBackend;
        assert_eq!(backend.is_available(), cfg!(target_os = "macos"));
    }
}
