//! Platform-native desktop notification dispatch.
//!
//! Best-effort notifications — failures are logged but never propagate.
//! Used by `kild agent-status --notify` to alert when an agent needs input.

use tracing::{info, warn};

/// Send a platform-native desktop notification (best-effort).
///
/// - macOS: `osascript` (Notification Center)
/// - Linux: `notify-send` (requires libnotify)
/// - Other: no-op
///
/// Failures are logged at warn level but never returned as errors.
pub fn send_notification(title: &str, message: &str) {
    info!(
        event = "core.notify.send_started",
        title = title,
        message = message,
    );

    send_platform_notification(title, message);
}

#[cfg(target_os = "macos")]
fn send_platform_notification(title: &str, message: &str) {
    let escaped_title = title.replace('"', r#"\""#);
    let escaped_message = message.replace('"', r#"\""#);
    let script = format!(
        r#"display notification "{}" with title "{}""#,
        escaped_message, escaped_title
    );

    match std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
    {
        Ok(output) if output.status.success() => {
            info!(event = "core.notify.send_completed", title = title);
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                event = "core.notify.send_failed",
                title = title,
                stderr = %stderr,
            );
        }
        Err(e) => {
            warn!(
                event = "core.notify.send_failed",
                title = title,
                error = %e,
            );
        }
    }
}

#[cfg(target_os = "linux")]
fn send_platform_notification(title: &str, message: &str) {
    use tracing::debug;

    match which::which("notify-send") {
        Ok(_) => {}
        Err(which::Error::CannotFindBinaryPath) => {
            debug!(
                event = "core.notify.send_skipped",
                reason = "notify-send not found",
            );
            return;
        }
        Err(e) => {
            warn!(
                event = "core.notify.send_failed",
                error = %e,
            );
            return;
        }
    }

    match std::process::Command::new("notify-send")
        .arg(title)
        .arg(message)
        .output()
    {
        Ok(output) if output.status.success() => {
            info!(event = "core.notify.send_completed", title = title);
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                event = "core.notify.send_failed",
                title = title,
                stderr = %stderr,
            );
        }
        Err(e) => {
            warn!(
                event = "core.notify.send_failed",
                title = title,
                error = %e,
            );
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn send_platform_notification(_title: &str, _message: &str) {
    tracing::debug!(
        event = "core.notify.send_skipped",
        reason = "unsupported platform",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_notification_does_not_panic() {
        // Should never panic regardless of platform or tool availability
        send_notification("Test Title", "Test message body");
    }

    #[test]
    fn test_notification_message_escaping() {
        // Titles/messages with double quotes should not panic
        send_notification(r#"Title with "quotes""#, r#"Message with "quotes""#);
    }
}
