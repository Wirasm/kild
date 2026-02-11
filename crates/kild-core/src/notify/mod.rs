//! Platform-native desktop notification dispatch.
//!
//! Best-effort notifications — failures are logged but never propagate.
//! Used by `kild agent-status --notify` to alert when an agent enters
//! `Waiting` or `Error` status.

use crate::sessions::types::AgentStatus;
use tracing::{info, warn};

#[cfg(not(target_os = "macos"))]
use tracing::debug;

/// Returns `true` if a notification should be sent for the given status.
///
/// Only `Waiting` and `Error` require user attention.
pub fn should_notify(notify: bool, status: AgentStatus) -> bool {
    notify && matches!(status, AgentStatus::Waiting | AgentStatus::Error)
}

/// Format the notification message for an agent status change.
pub fn format_notification_message(agent: &str, branch: &str, status: AgentStatus) -> String {
    format!("Agent {} in {} needs input ({})", agent, branch, status)
}

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
    use crate::escape::applescript_escape;

    let escaped_title = applescript_escape(title);
    let escaped_message = applescript_escape(message);
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
                title = title,
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
    debug!(
        event = "core.notify.send_skipped",
        reason = "unsupported platform",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_notify_fires_for_waiting() {
        assert!(should_notify(true, AgentStatus::Waiting));
    }

    #[test]
    fn test_should_notify_fires_for_error() {
        assert!(should_notify(true, AgentStatus::Error));
    }

    #[test]
    fn test_should_notify_skips_working() {
        assert!(!should_notify(true, AgentStatus::Working));
    }

    #[test]
    fn test_should_notify_skips_idle() {
        assert!(!should_notify(true, AgentStatus::Idle));
    }

    #[test]
    fn test_should_notify_skips_done() {
        assert!(!should_notify(true, AgentStatus::Done));
    }

    #[test]
    fn test_should_notify_suppressed_when_flag_false() {
        assert!(!should_notify(false, AgentStatus::Waiting));
        assert!(!should_notify(false, AgentStatus::Error));
    }

    #[test]
    fn test_format_notification_message_content() {
        let msg = format_notification_message("claude", "my-branch", AgentStatus::Waiting);
        assert_eq!(msg, "Agent claude in my-branch needs input (waiting)");
    }

    #[test]
    fn test_format_notification_message_error_status() {
        let msg = format_notification_message("claude", "feat-x", AgentStatus::Error);
        assert_eq!(msg, "Agent claude in feat-x needs input (error)");
    }
}
