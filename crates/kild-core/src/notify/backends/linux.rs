//! Linux notification backend using notify-send (libnotify).

use crate::notify::errors::NotifyError;
use crate::notify::traits::NotificationBackend;

/// Linux notification backend via `notify-send` (libnotify).
pub struct LinuxNotificationBackend;

impl NotificationBackend for LinuxNotificationBackend {
    fn name(&self) -> &'static str {
        "linux"
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "linux") && which::which("notify-send").is_ok()
    }

    fn send(&self, title: &str, message: &str) -> Result<(), NotifyError> {
        let output = std::process::Command::new("notify-send")
            .arg(title)
            .arg(message)
            .output()
            .map_err(|e| NotifyError::SendFailed {
                message: format!("notify-send exec failed: {}", e),
            })?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(NotifyError::SendFailed {
                message: format!("notify-send exit {}: {}", output.status, stderr.trim()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_backend_name() {
        let backend = LinuxNotificationBackend;
        assert_eq!(backend.name(), "linux");
    }

    #[test]
    fn linux_backend_availability_matches_platform() {
        let backend = LinuxNotificationBackend;
        if !cfg!(target_os = "linux") {
            assert!(!backend.is_available());
        }
    }
}
