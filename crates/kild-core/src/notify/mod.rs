//! Platform-native desktop notification dispatch.
//!
//! Best-effort notifications — failures are logged but never propagate.
//! Used by `kild agent-status --notify` to alert when an agent enters
//! `Waiting` or `Error` status.
//!
//! Notifications are dispatched via the [`NotificationBackend`] trait,
//! with platform-specific backends registered in [`registry`].

pub mod backends;
pub mod errors;
pub mod registry;
pub mod traits;

pub use errors::NotifyError;
pub use traits::NotificationBackend;

use kild_protocol::AgentStatus;
use tracing::{info, warn};

/// Returns `true` if a notification should be sent for the given status.
///
/// Only `Waiting` and `Error` require user attention.
pub fn should_notify(notify: bool, status: AgentStatus) -> bool {
    notify && matches!(status, AgentStatus::Waiting | AgentStatus::Error)
}

/// Format the notification message for an agent status change.
///
/// The message body always reads "needs input" regardless of status.
/// This covers both `Waiting` (literal input required) and `Error`
/// (user must inspect and unblock the agent).
pub fn format_notification_message(agent: &str, branch: &str, status: AgentStatus) -> String {
    format!("Agent {} in {} needs input ({})", agent, branch, status)
}

/// Send a platform-native desktop notification (best-effort).
///
/// Dispatches to the first available [`NotificationBackend`] via the registry.
/// Failures are logged at warn level but never returned as errors.
pub fn send_notification(title: &str, message: &str) {
    info!(
        event = "core.notify.send_started",
        title = title,
        message = message,
    );

    match registry::send_via_backend(title, message) {
        Ok(true) => {
            info!(event = "core.notify.send_completed", title = title);
        }
        Ok(false) => {
            // No backend available — already logged at debug in registry
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
