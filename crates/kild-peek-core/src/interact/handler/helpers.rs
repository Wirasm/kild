use std::thread;
use std::time::Duration;

use core_graphics::event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton, EventField};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use tracing::{debug, warn};

use crate::interact::errors::InteractionError;
use crate::interact::types::{ClickModifier, InteractionTarget};
use crate::window::{
    WindowError, WindowInfo, find_window_by_app, find_window_by_app_and_title,
    find_window_by_app_and_title_with_wait, find_window_by_app_with_wait, find_window_by_title,
    find_window_by_title_with_wait,
};

// SAFETY: FFI declaration for AXIsProcessTrusted from macOS ApplicationServices framework.
// Returns false when the process lacks accessibility permissions (does not crash).
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

/// Delay between mouse down and mouse up events
pub(super) const MOUSE_EVENT_DELAY: Duration = Duration::from_millis(10);

/// Delay after focusing a window before sending events
pub(super) const FOCUS_SETTLE_DELAY: Duration = Duration::from_millis(50);

/// Delay between key down and key up events
pub(super) const KEY_EVENT_DELAY: Duration = Duration::from_millis(10);

/// Delay between drag events (down, move, up)
pub(super) const DRAG_EVENT_DELAY: Duration = Duration::from_millis(25);

/// Delay between individual character events when typing text
pub(super) const CHAR_EVENT_DELAY: Duration = Duration::from_millis(5);

/// Check if the current process has accessibility permissions
pub(super) fn check_accessibility_permission() -> Result<(), InteractionError> {
    let trusted = unsafe { AXIsProcessTrusted() };
    if !trusted {
        return Err(InteractionError::AccessibilityPermissionDenied);
    }
    Ok(())
}

/// Resolve an InteractionTarget to a WindowInfo and focus the window
pub(super) fn resolve_and_focus_window(
    target: &InteractionTarget,
    timeout_ms: Option<u64>,
) -> Result<WindowInfo, InteractionError> {
    let window = find_window_by_target(target, timeout_ms)?;

    if window.is_minimized() {
        return Err(InteractionError::WindowMinimized {
            title: window.title().to_string(),
        });
    }

    // Focus the window via AppleScript
    focus_window(window.app_name())?;

    // Brief pause for focus to settle
    thread::sleep(FOCUS_SETTLE_DELAY);

    Ok(window)
}

/// Find a window by interaction target, optionally waiting for it to appear
pub(super) fn find_window_by_target(
    target: &InteractionTarget,
    timeout_ms: Option<u64>,
) -> Result<WindowInfo, InteractionError> {
    let result = match (target, timeout_ms) {
        (InteractionTarget::Window { title }, Some(timeout)) => {
            find_window_by_title_with_wait(title, timeout)
        }
        (InteractionTarget::Window { title }, None) => find_window_by_title(title),
        (InteractionTarget::App { app }, Some(timeout)) => {
            find_window_by_app_with_wait(app, timeout)
        }
        (InteractionTarget::App { app }, None) => find_window_by_app(app),
        (InteractionTarget::AppAndWindow { app, title }, Some(timeout)) => {
            find_window_by_app_and_title_with_wait(app, title, timeout)
        }
        (InteractionTarget::AppAndWindow { app, title }, None) => {
            find_window_by_app_and_title(app, title)
        }
    };
    result.map_err(map_window_error)
}

/// Focus a window by app name using AppleScript
///
/// # Errors
///
/// Returns `InteractionError::WindowFocusFailed` if the osascript command fails
/// to execute or returns a non-zero exit status.
pub(super) fn focus_window(app_name: &str) -> Result<(), InteractionError> {
    debug!(event = "peek.core.interact.focus_started", app = app_name);

    let script = format!(
        "tell application \"System Events\" to set frontmost of process \"{}\" to true",
        app_name
    );

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| {
            warn!(
                event = "peek.core.interact.focus_command_failed",
                app = app_name,
                error = %e
            );
            InteractionError::WindowFocusFailed {
                app: app_name.to_string(),
                reason: format!("Failed to execute osascript: {}", e),
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(
            event = "peek.core.interact.focus_failed",
            app = app_name,
            stderr = %stderr
        );
        return Err(InteractionError::WindowFocusFailed {
            app: app_name.to_string(),
            reason: stderr.trim().to_string(),
        });
    }

    debug!(event = "peek.core.interact.focus_completed", app = app_name);
    Ok(())
}

/// Map WindowError to InteractionError
pub(super) fn map_window_error(error: WindowError) -> InteractionError {
    use WindowError::*;

    match error {
        WindowNotFound { title } => InteractionError::WindowNotFound { title },
        WindowNotFoundByApp { app } => InteractionError::WindowNotFoundByApp { app },
        WaitTimeoutByTitle { title, timeout_ms } => {
            InteractionError::WaitTimeoutByTitle { title, timeout_ms }
        }
        WaitTimeoutByApp { app, timeout_ms } => {
            InteractionError::WaitTimeoutByApp { app, timeout_ms }
        }
        WaitTimeoutByAppAndTitle {
            app,
            title,
            timeout_ms,
        } => InteractionError::WaitTimeoutByAppAndTitle {
            app,
            title,
            timeout_ms,
        },
        other => {
            warn!(
                event = "peek.core.interact.window_error_unmapped",
                error = %other
            );
            InteractionError::WindowLookupFailed {
                reason: other.to_string(),
            }
        }
    }
}

/// Validate that coordinates are within window bounds
pub(super) fn validate_coordinates(
    x: i32,
    y: i32,
    window: &WindowInfo,
) -> Result<(), InteractionError> {
    if x < 0 || y < 0 || x as u32 >= window.width() || y as u32 >= window.height() {
        return Err(InteractionError::CoordinateOutOfBounds {
            x,
            y,
            width: window.width(),
            height: window.height(),
        });
    }
    Ok(())
}

/// Convert window-relative coordinates to screen-absolute coordinates
pub(super) fn to_screen_coordinates(x: i32, y: i32, window: &WindowInfo) -> (f64, f64) {
    let screen_x = (window.x() + x) as f64;
    let screen_y = (window.y() + y) as f64;
    (screen_x, screen_y)
}

/// Create and post mouse click events based on modifier
pub(super) fn create_and_post_mouse_click(
    point: CGPoint,
    screen_x: f64,
    screen_y: f64,
    modifier: ClickModifier,
) -> Result<(), InteractionError> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| InteractionError::EventSourceFailed)?;

    let (down_type, up_type, button) = if modifier == ClickModifier::Right {
        (
            CGEventType::RightMouseDown,
            CGEventType::RightMouseUp,
            CGMouseButton::Right,
        )
    } else {
        (
            CGEventType::LeftMouseDown,
            CGEventType::LeftMouseUp,
            CGMouseButton::Left,
        )
    };

    let mouse_down =
        CGEvent::new_mouse_event(source.clone(), down_type, point, button).map_err(|()| {
            InteractionError::MouseEventFailed {
                x: screen_x,
                y: screen_y,
            }
        })?;

    let mouse_up = CGEvent::new_mouse_event(source, up_type, point, button).map_err(|()| {
        InteractionError::MouseEventFailed {
            x: screen_x,
            y: screen_y,
        }
    })?;

    if modifier == ClickModifier::Double {
        mouse_down.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, 2);
        mouse_up.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, 2);
    }

    mouse_down.post(CGEventTapLocation::HID);
    thread::sleep(MOUSE_EVENT_DELAY);
    mouse_up.post(CGEventTapLocation::HID);

    Ok(())
}

/// Action name for a click modifier
pub(super) fn click_action_name(modifier: ClickModifier) -> &'static str {
    match modifier {
        ClickModifier::None => "click",
        ClickModifier::Right => "right_click",
        ClickModifier::Double => "double_click",
    }
}
