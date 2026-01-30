use std::thread;
use std::time::Duration;

use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton, EventField,
    ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use tracing::{debug, info, warn};

use super::errors::InteractionError;
use super::operations;
use super::types::{
    ClickModifier, ClickRequest, ClickTextRequest, DragRequest, HoverRequest, HoverTextRequest,
    InteractionResult, InteractionTarget, KeyComboRequest, ScrollRequest, TypeRequest,
};
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
const MOUSE_EVENT_DELAY: Duration = Duration::from_millis(10);

/// Delay after focusing a window before sending events
const FOCUS_SETTLE_DELAY: Duration = Duration::from_millis(50);

/// Delay between key down and key up events
const KEY_EVENT_DELAY: Duration = Duration::from_millis(10);

/// Delay between drag events (down, move, up)
const DRAG_EVENT_DELAY: Duration = Duration::from_millis(25);

/// Check if the current process has accessibility permissions
fn check_accessibility_permission() -> Result<(), InteractionError> {
    let trusted = unsafe { AXIsProcessTrusted() };
    if !trusted {
        return Err(InteractionError::AccessibilityPermissionDenied);
    }
    Ok(())
}

/// Resolve an InteractionTarget to a WindowInfo and focus the window
fn resolve_and_focus_window(
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
fn find_window_by_target(
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
fn focus_window(app_name: &str) -> Result<(), InteractionError> {
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
fn map_window_error(error: WindowError) -> InteractionError {
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
fn validate_coordinates(x: i32, y: i32, window: &WindowInfo) -> Result<(), InteractionError> {
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
fn to_screen_coordinates(x: i32, y: i32, window: &WindowInfo) -> (f64, f64) {
    let screen_x = (window.x() + x) as f64;
    let screen_y = (window.y() + y) as f64;
    (screen_x, screen_y)
}

/// Create and post mouse click events based on modifier
fn create_and_post_mouse_click(
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
fn click_action_name(modifier: ClickModifier) -> &'static str {
    match modifier {
        ClickModifier::None => "click",
        ClickModifier::Right => "right_click",
        ClickModifier::Double => "double_click",
    }
}

/// Click at coordinates within a window
///
/// Focuses the target window via AppleScript, validates coordinates are within
/// window bounds, then sends mouse down/up CGEvents at the screen-absolute position.
/// Supports left-click, right-click, and double-click via the request's modifier.
///
/// # Errors
///
/// Returns error if accessibility permission is denied, window is not found or
/// minimized, coordinates are out of bounds, or event creation fails.
pub fn click(request: &ClickRequest) -> Result<InteractionResult, InteractionError> {
    info!(
        event = "peek.core.interact.click_started",
        x = request.x(),
        y = request.y(),
        modifier = ?request.modifier(),
        target = ?request.target()
    );

    check_accessibility_permission()?;

    let window = resolve_and_focus_window(request.target(), request.timeout_ms())?;
    validate_coordinates(request.x(), request.y(), &window)?;

    let (screen_x, screen_y) = to_screen_coordinates(request.x(), request.y(), &window);
    let point = CGPoint::new(screen_x, screen_y);

    debug!(
        event = "peek.core.interact.click_posting",
        screen_x = screen_x,
        screen_y = screen_y,
        modifier = ?request.modifier()
    );
    create_and_post_mouse_click(point, screen_x, screen_y, request.modifier())?;

    let action = click_action_name(request.modifier());
    info!(
        event = "peek.core.interact.click_completed",
        action = action,
        screen_x = screen_x,
        screen_y = screen_y,
        window_title = window.title()
    );

    Ok(InteractionResult::success(
        action,
        serde_json::json!({
            "x": request.x(),
            "y": request.y(),
            "screen_x": screen_x,
            "screen_y": screen_y,
            "modifier": format!("{:?}", request.modifier()),
            "window": window.title(),
        }),
    ))
}

/// Type text into the focused element of a window
///
/// Focuses the target window, then sends the text as a unicode string via a
/// single CGEvent (keycode 0 with unicode string set). This handles special
/// characters and international input correctly.
///
/// # Errors
///
/// Returns error if accessibility permission is denied, window is not found or
/// minimized, or event creation fails.
pub fn type_text(request: &TypeRequest) -> Result<InteractionResult, InteractionError> {
    info!(
        event = "peek.core.interact.type_started",
        text_len = request.text().len(),
        target = ?request.target()
    );

    check_accessibility_permission()?;

    let window = resolve_and_focus_window(request.target(), request.timeout_ms())?;

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| InteractionError::EventSourceFailed)?;

    // Create a keyboard event with keycode 0 and set the unicode string.
    // This sends text as a unicode string rather than individual key events,
    // which correctly handles special characters and international input.
    let event = CGEvent::new_keyboard_event(source, 0, true)
        .map_err(|()| InteractionError::KeyboardEventFailed { keycode: 0 })?;

    event.set_string(request.text());
    debug!(
        event = "peek.core.interact.type_posting",
        text_len = request.text().len()
    );
    event.post(CGEventTapLocation::HID);

    info!(
        event = "peek.core.interact.type_completed",
        text_len = request.text().len(),
        window_title = window.title()
    );

    Ok(InteractionResult::success(
        "type",
        serde_json::json!({
            "text_length": request.text().len(),
            "window": window.title(),
        }),
    ))
}

/// Send a key combination to a window
///
/// Focuses the target window, parses the combo string into keycode + modifier
/// flags, then sends key down/up CGEvents.
///
/// # Errors
///
/// Returns error if accessibility permission is denied, window is not found or
/// minimized, the combo string is invalid, or event creation fails.
pub fn send_key_combo(request: &KeyComboRequest) -> Result<InteractionResult, InteractionError> {
    info!(
        event = "peek.core.interact.key_started",
        combo = request.combo(),
        target = ?request.target()
    );

    check_accessibility_permission()?;

    let window = resolve_and_focus_window(request.target(), request.timeout_ms())?;
    let mapping = operations::parse_key_combo(request.combo())?;

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| InteractionError::EventSourceFailed)?;

    let key_down =
        CGEvent::new_keyboard_event(source.clone(), mapping.keycode(), true).map_err(|()| {
            InteractionError::KeyboardEventFailed {
                keycode: mapping.keycode(),
            }
        })?;

    let key_up = CGEvent::new_keyboard_event(source, mapping.keycode(), false).map_err(|()| {
        InteractionError::KeyboardEventFailed {
            keycode: mapping.keycode(),
        }
    })?;

    if mapping.flags() != CGEventFlags::CGEventFlagNull {
        debug!(
            event = "peek.core.interact.key_flags_applied",
            keycode = mapping.keycode(),
            flags = ?mapping.flags()
        );
        key_down.set_flags(mapping.flags());
        key_up.set_flags(mapping.flags());
    }

    debug!(
        event = "peek.core.interact.key_posting",
        keycode = mapping.keycode()
    );
    key_down.post(CGEventTapLocation::HID);
    thread::sleep(KEY_EVENT_DELAY);
    key_up.post(CGEventTapLocation::HID);

    info!(
        event = "peek.core.interact.key_completed",
        combo = request.combo(),
        keycode = mapping.keycode(),
        window_title = window.title()
    );

    Ok(InteractionResult::success(
        "key",
        serde_json::json!({
            "combo": request.combo(),
            "keycode": mapping.keycode(),
            "window": window.title(),
        }),
    ))
}

/// Click an element identified by text content
///
/// Finds the target element by text, computes its center position in window-relative
/// coordinates, then clicks at that position. Errors if no element or multiple elements
/// match the text.
///
/// # Errors
///
/// Returns error if accessibility permission is denied, window is not found or
/// minimized, element text is not found, multiple elements match (ambiguous),
/// or the element has no position data.
pub fn click_text(request: &ClickTextRequest) -> Result<InteractionResult, InteractionError> {
    info!(
        event = "peek.core.interact.click_text_started",
        text = request.text(),
        modifier = ?request.modifier(),
        target = ?request.target()
    );

    check_accessibility_permission()?;

    // Find the window (without focusing yet)
    let window = find_window_by_target(request.target(), request.timeout_ms())?;

    if window.is_minimized() {
        return Err(InteractionError::WindowMinimized {
            title: window.title().to_string(),
        });
    }

    let pid = window.pid().ok_or(InteractionError::NoPidAvailable)?;

    // Query accessibility tree for elements
    let raw_elements = crate::element::accessibility::query_elements(pid)
        .map_err(|reason| InteractionError::ElementQueryFailed { reason })?;

    // Convert RawElement → ElementInfo (screen-absolute → window-relative coordinates)
    let elements: Vec<crate::element::ElementInfo> = raw_elements
        .iter()
        .map(|raw| crate::element::handler::convert_raw_to_element_info(raw, &window))
        .collect();

    // Find matching elements
    let matches: Vec<&crate::element::ElementInfo> = elements
        .iter()
        .filter(|e| e.matches_text(request.text()))
        .collect();

    if matches.is_empty() {
        return Err(InteractionError::ElementNotFound {
            text: request.text().to_string(),
        });
    }

    if matches.len() > 1 {
        return Err(InteractionError::ElementAmbiguous {
            text: request.text().to_string(),
            count: matches.len(),
        });
    }

    let element = matches[0];

    // Element must have non-zero width or height (reject invisible/zero-size elements)
    if element.width() == 0 && element.height() == 0 {
        return Err(InteractionError::ElementNoPosition);
    }

    // Compute center of element (window-relative)
    let center_x = element.x() + (element.width() as i32) / 2;
    let center_y = element.y() + (element.height() as i32) / 2;

    // Now focus the window
    focus_window(window.app_name())?;
    thread::sleep(FOCUS_SETTLE_DELAY);

    // Validate coordinates are within bounds
    validate_coordinates(center_x, center_y, &window)?;

    let (screen_x, screen_y) = to_screen_coordinates(center_x, center_y, &window);
    let point = CGPoint::new(screen_x, screen_y);

    debug!(
        event = "peek.core.interact.click_text_posting",
        screen_x = screen_x,
        screen_y = screen_y,
        text = request.text(),
        modifier = ?request.modifier()
    );
    create_and_post_mouse_click(point, screen_x, screen_y, request.modifier())?;

    let action = click_action_name(request.modifier());
    info!(
        event = "peek.core.interact.click_text_completed",
        action = action,
        text = request.text(),
        center_x = center_x,
        center_y = center_y,
        window_title = window.title()
    );

    Ok(InteractionResult::success(
        action,
        serde_json::json!({
            "text": request.text(),
            "element_role": element.role(),
            "element_x": element.x(),
            "element_y": element.y(),
            "center_x": center_x,
            "center_y": center_y,
            "screen_x": screen_x,
            "screen_y": screen_y,
            "modifier": format!("{:?}", request.modifier()),
            "window": window.title(),
        }),
    ))
}

/// Drag from one point to another within a window
///
/// Performs a drag-and-drop sequence: mouse down at source, drag to destination,
/// mouse up at destination. Uses `LeftMouseDragged` event type between down and up.
/// Events are spaced by 25ms delays to allow the system to process the drag.
///
/// # Errors
///
/// Returns error if accessibility permission is denied, window is not found or
/// minimized, either coordinate pair is out of bounds, or event creation fails.
pub fn drag(request: &DragRequest) -> Result<InteractionResult, InteractionError> {
    info!(
        event = "peek.core.interact.drag_started",
        from_x = request.from_x(),
        from_y = request.from_y(),
        to_x = request.to_x(),
        to_y = request.to_y(),
        target = ?request.target()
    );

    check_accessibility_permission()?;

    let window = resolve_and_focus_window(request.target(), request.timeout_ms())?;
    validate_coordinates(request.from_x(), request.from_y(), &window)?;
    validate_coordinates(request.to_x(), request.to_y(), &window)?;

    let (from_screen_x, from_screen_y) =
        to_screen_coordinates(request.from_x(), request.from_y(), &window);
    let (to_screen_x, to_screen_y) = to_screen_coordinates(request.to_x(), request.to_y(), &window);

    let from_point = CGPoint::new(from_screen_x, from_screen_y);
    let to_point = CGPoint::new(to_screen_x, to_screen_y);

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| InteractionError::EventSourceFailed)?;

    let mouse_down = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseDown,
        from_point,
        CGMouseButton::Left,
    )
    .map_err(|()| InteractionError::DragEventFailed {
        from_x: from_screen_x,
        from_y: from_screen_y,
        to_x: to_screen_x,
        to_y: to_screen_y,
    })?;

    let mouse_dragged = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseDragged,
        to_point,
        CGMouseButton::Left,
    )
    .map_err(|()| InteractionError::DragEventFailed {
        from_x: from_screen_x,
        from_y: from_screen_y,
        to_x: to_screen_x,
        to_y: to_screen_y,
    })?;

    let mouse_up = CGEvent::new_mouse_event(
        source,
        CGEventType::LeftMouseUp,
        to_point,
        CGMouseButton::Left,
    )
    .map_err(|()| InteractionError::DragEventFailed {
        from_x: from_screen_x,
        from_y: from_screen_y,
        to_x: to_screen_x,
        to_y: to_screen_y,
    })?;

    debug!(
        event = "peek.core.interact.drag_posting",
        from_screen_x = from_screen_x,
        from_screen_y = from_screen_y,
        to_screen_x = to_screen_x,
        to_screen_y = to_screen_y
    );
    mouse_down.post(CGEventTapLocation::HID);
    thread::sleep(DRAG_EVENT_DELAY);
    mouse_dragged.post(CGEventTapLocation::HID);
    thread::sleep(DRAG_EVENT_DELAY);
    mouse_up.post(CGEventTapLocation::HID);

    info!(
        event = "peek.core.interact.drag_completed",
        from_screen_x = from_screen_x,
        from_screen_y = from_screen_y,
        to_screen_x = to_screen_x,
        to_screen_y = to_screen_y,
        window_title = window.title()
    );

    Ok(InteractionResult::success(
        "drag",
        serde_json::json!({
            "from_x": request.from_x(),
            "from_y": request.from_y(),
            "to_x": request.to_x(),
            "to_y": request.to_y(),
            "from_screen_x": from_screen_x,
            "from_screen_y": from_screen_y,
            "to_screen_x": to_screen_x,
            "to_screen_y": to_screen_y,
            "window": window.title(),
        }),
    ))
}

/// Scroll within a window
///
/// If coordinates are provided via `at_x`/`at_y`, moves the mouse to that
/// position first (to control scroll location), then sends the scroll event.
/// Uses line-based scrolling.
///
/// # Errors
///
/// Returns error if accessibility permission is denied, window is not found or
/// minimized, position coordinates are out of bounds, or scroll event creation fails.
pub fn scroll(request: &ScrollRequest) -> Result<InteractionResult, InteractionError> {
    info!(
        event = "peek.core.interact.scroll_started",
        delta_x = request.delta_x(),
        delta_y = request.delta_y(),
        at_x = ?request.at_x(),
        at_y = ?request.at_y(),
        target = ?request.target()
    );

    check_accessibility_permission()?;

    if request.delta_x() == 0 && request.delta_y() == 0 {
        return Err(InteractionError::ScrollEventFailed);
    }

    let window = resolve_and_focus_window(request.target(), request.timeout_ms())?;

    // If at_x/at_y specified, validate and move mouse there first
    if let (Some(at_x), Some(at_y)) = (request.at_x(), request.at_y()) {
        validate_coordinates(at_x, at_y, &window)?;
        let (screen_x, screen_y) = to_screen_coordinates(at_x, at_y, &window);
        let point = CGPoint::new(screen_x, screen_y);

        let move_source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|()| InteractionError::EventSourceFailed)?;
        let move_event = CGEvent::new_mouse_event(
            move_source,
            CGEventType::MouseMoved,
            point,
            CGMouseButton::Left,
        )
        .map_err(|()| InteractionError::MouseEventFailed {
            x: screen_x,
            y: screen_y,
        })?;
        move_event.post(CGEventTapLocation::HID);
        thread::sleep(MOUSE_EVENT_DELAY);
    }

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| InteractionError::EventSourceFailed)?;

    // wheel_count: 1 for vertical-only scroll, 2 when horizontal scrolling is also needed
    let wheel_count = if request.delta_x() == 0 { 1 } else { 2 };

    let scroll_event = CGEvent::new_scroll_event(
        source,
        ScrollEventUnit::LINE,
        wheel_count,
        request.delta_y(),
        request.delta_x(),
        0,
    )
    .map_err(|()| InteractionError::ScrollEventFailed)?;

    debug!(
        event = "peek.core.interact.scroll_posting",
        delta_x = request.delta_x(),
        delta_y = request.delta_y(),
        wheel_count = wheel_count
    );
    scroll_event.post(CGEventTapLocation::HID);

    info!(
        event = "peek.core.interact.scroll_completed",
        delta_x = request.delta_x(),
        delta_y = request.delta_y(),
        window_title = window.title()
    );

    Ok(InteractionResult::success(
        "scroll",
        serde_json::json!({
            "delta_x": request.delta_x(),
            "delta_y": request.delta_y(),
            "at_x": request.at_x(),
            "at_y": request.at_y(),
            "window": window.title(),
        }),
    ))
}

/// Hover at coordinates within a window (move mouse without clicking)
///
/// Focuses the target window, then moves the mouse to the specified position
/// using a `MouseMoved` event. Does not click.
///
/// # Errors
///
/// Returns error if accessibility permission is denied, window is not found or
/// minimized, coordinates are out of bounds, or event creation fails.
pub fn hover(request: &HoverRequest) -> Result<InteractionResult, InteractionError> {
    info!(
        event = "peek.core.interact.hover_started",
        x = request.x(),
        y = request.y(),
        target = ?request.target()
    );

    check_accessibility_permission()?;

    let window = resolve_and_focus_window(request.target(), request.timeout_ms())?;
    validate_coordinates(request.x(), request.y(), &window)?;

    let (screen_x, screen_y) = to_screen_coordinates(request.x(), request.y(), &window);
    let point = CGPoint::new(screen_x, screen_y);

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| InteractionError::EventSourceFailed)?;

    let move_event =
        CGEvent::new_mouse_event(source, CGEventType::MouseMoved, point, CGMouseButton::Left)
            .map_err(|()| InteractionError::MouseEventFailed {
                x: screen_x,
                y: screen_y,
            })?;

    debug!(
        event = "peek.core.interact.hover_posting",
        screen_x = screen_x,
        screen_y = screen_y
    );
    move_event.post(CGEventTapLocation::HID);

    info!(
        event = "peek.core.interact.hover_completed",
        screen_x = screen_x,
        screen_y = screen_y,
        window_title = window.title()
    );

    Ok(InteractionResult::success(
        "hover",
        serde_json::json!({
            "x": request.x(),
            "y": request.y(),
            "screen_x": screen_x,
            "screen_y": screen_y,
            "window": window.title(),
        }),
    ))
}

/// Hover over an element identified by text content (move mouse without clicking)
///
/// Finds the target element by text, computes its center position, then moves
/// the mouse to that position. Does not click.
///
/// # Errors
///
/// Returns error if accessibility permission is denied, window is not found or
/// minimized, element text is not found, multiple elements match, or the element
/// has no position data.
pub fn hover_text(request: &HoverTextRequest) -> Result<InteractionResult, InteractionError> {
    info!(
        event = "peek.core.interact.hover_text_started",
        text = request.text(),
        target = ?request.target()
    );

    check_accessibility_permission()?;

    let window = find_window_by_target(request.target(), request.timeout_ms())?;

    if window.is_minimized() {
        return Err(InteractionError::WindowMinimized {
            title: window.title().to_string(),
        });
    }

    let pid = window.pid().ok_or(InteractionError::NoPidAvailable)?;

    let raw_elements = crate::element::accessibility::query_elements(pid)
        .map_err(|reason| InteractionError::ElementQueryFailed { reason })?;

    let elements: Vec<crate::element::ElementInfo> = raw_elements
        .iter()
        .map(|raw| crate::element::handler::convert_raw_to_element_info(raw, &window))
        .collect();

    let matches: Vec<&crate::element::ElementInfo> = elements
        .iter()
        .filter(|e| e.matches_text(request.text()))
        .collect();

    if matches.is_empty() {
        return Err(InteractionError::ElementNotFound {
            text: request.text().to_string(),
        });
    }

    if matches.len() > 1 {
        return Err(InteractionError::ElementAmbiguous {
            text: request.text().to_string(),
            count: matches.len(),
        });
    }

    let element = matches[0];

    if element.width() == 0 && element.height() == 0 {
        return Err(InteractionError::ElementNoPosition);
    }

    let center_x = element.x() + (element.width() as i32) / 2;
    let center_y = element.y() + (element.height() as i32) / 2;

    focus_window(window.app_name())?;
    thread::sleep(FOCUS_SETTLE_DELAY);

    validate_coordinates(center_x, center_y, &window)?;

    let (screen_x, screen_y) = to_screen_coordinates(center_x, center_y, &window);
    let point = CGPoint::new(screen_x, screen_y);

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| InteractionError::EventSourceFailed)?;

    let move_event =
        CGEvent::new_mouse_event(source, CGEventType::MouseMoved, point, CGMouseButton::Left)
            .map_err(|()| InteractionError::MouseEventFailed {
                x: screen_x,
                y: screen_y,
            })?;

    debug!(
        event = "peek.core.interact.hover_text_posting",
        screen_x = screen_x,
        screen_y = screen_y,
        text = request.text()
    );
    move_event.post(CGEventTapLocation::HID);

    info!(
        event = "peek.core.interact.hover_text_completed",
        text = request.text(),
        center_x = center_x,
        center_y = center_y,
        window_title = window.title()
    );

    Ok(InteractionResult::success(
        "hover",
        serde_json::json!({
            "text": request.text(),
            "element_role": element.role(),
            "element_x": element.x(),
            "element_y": element.y(),
            "center_x": center_x,
            "center_y": center_y,
            "screen_x": screen_x,
            "screen_y": screen_y,
            "window": window.title(),
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_screen_coordinates() {
        let window = WindowInfo::new(
            1,
            "Test".to_string(),
            "TestApp".to_string(),
            100,
            200,
            800,
            600,
            false,
            None,
        );
        let (sx, sy) = to_screen_coordinates(50, 30, &window);
        assert!((sx - 150.0).abs() < f64::EPSILON);
        assert!((sy - 230.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_to_screen_coordinates_origin() {
        let window = WindowInfo::new(
            1,
            "Test".to_string(),
            "TestApp".to_string(),
            0,
            0,
            800,
            600,
            false,
            None,
        );
        let (sx, sy) = to_screen_coordinates(0, 0, &window);
        assert!((sx - 0.0).abs() < f64::EPSILON);
        assert!((sy - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_validate_coordinates_valid() {
        let window = WindowInfo::new(
            1,
            "Test".to_string(),
            "TestApp".to_string(),
            0,
            0,
            800,
            600,
            false,
            None,
        );
        assert!(validate_coordinates(0, 0, &window).is_ok());
        assert!(validate_coordinates(799, 599, &window).is_ok());
        assert!(validate_coordinates(400, 300, &window).is_ok());
    }

    #[test]
    fn test_validate_coordinates_out_of_bounds() {
        let window = WindowInfo::new(
            1,
            "Test".to_string(),
            "TestApp".to_string(),
            0,
            0,
            800,
            600,
            false,
            None,
        );
        assert!(validate_coordinates(800, 0, &window).is_err());
        assert!(validate_coordinates(0, 600, &window).is_err());
        assert!(validate_coordinates(999, 999, &window).is_err());
    }

    #[test]
    fn test_validate_coordinates_negative() {
        let window = WindowInfo::new(
            1,
            "Test".to_string(),
            "TestApp".to_string(),
            0,
            0,
            800,
            600,
            false,
            None,
        );
        assert!(validate_coordinates(-1, 0, &window).is_err());
        assert!(validate_coordinates(0, -1, &window).is_err());
    }

    #[test]
    fn test_map_window_error_not_found() {
        let err = map_window_error(WindowError::WindowNotFound {
            title: "Test".to_string(),
        });
        match err {
            InteractionError::WindowNotFound { title } => assert_eq!(title, "Test"),
            _ => panic!("Expected WindowNotFound"),
        }
    }

    #[test]
    fn test_map_window_error_not_found_by_app() {
        let err = map_window_error(WindowError::WindowNotFoundByApp {
            app: "TestApp".to_string(),
        });
        match err {
            InteractionError::WindowNotFoundByApp { app } => assert_eq!(app, "TestApp"),
            _ => panic!("Expected WindowNotFoundByApp"),
        }
    }

    #[test]
    fn test_map_window_error_other() {
        let err = map_window_error(WindowError::EnumerationFailed {
            message: "test error".to_string(),
        });
        match err {
            InteractionError::WindowLookupFailed { reason } => {
                assert!(reason.contains("test error"));
            }
            _ => panic!("Expected WindowLookupFailed"),
        }
    }

    #[test]
    fn test_map_window_error_wait_timeout_by_title() {
        let err = map_window_error(WindowError::WaitTimeoutByTitle {
            title: "Test".to_string(),
            timeout_ms: 5000,
        });
        match err {
            InteractionError::WaitTimeoutByTitle { title, timeout_ms } => {
                assert_eq!(title, "Test");
                assert_eq!(timeout_ms, 5000);
            }
            _ => panic!("Expected WaitTimeoutByTitle"),
        }
    }

    #[test]
    fn test_map_window_error_wait_timeout_by_app() {
        let err = map_window_error(WindowError::WaitTimeoutByApp {
            app: "Ghostty".to_string(),
            timeout_ms: 3000,
        });
        match err {
            InteractionError::WaitTimeoutByApp { app, timeout_ms } => {
                assert_eq!(app, "Ghostty");
                assert_eq!(timeout_ms, 3000);
            }
            _ => panic!("Expected WaitTimeoutByApp"),
        }
    }

    #[test]
    fn test_map_window_error_wait_timeout_by_app_and_title() {
        let err = map_window_error(WindowError::WaitTimeoutByAppAndTitle {
            app: "Ghostty".to_string(),
            title: "Terminal".to_string(),
            timeout_ms: 10000,
        });
        match err {
            InteractionError::WaitTimeoutByAppAndTitle {
                app,
                title,
                timeout_ms,
            } => {
                assert_eq!(app, "Ghostty");
                assert_eq!(title, "Terminal");
                assert_eq!(timeout_ms, 10000);
            }
            _ => panic!("Expected WaitTimeoutByAppAndTitle"),
        }
    }

    // Integration tests requiring accessibility permissions
    #[test]
    #[ignore]
    fn test_click_text_integration() {
        // This test requires accessibility permission and a running app
        // Run manually: cargo test --all -- --ignored test_click_text_integration
        // WARNING: This will actually click on a UI element!

        let request = ClickTextRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            "File",
        );
        let result = click_text(&request);
        match result {
            Ok(result) => {
                assert!(result.success);
            }
            Err(InteractionError::AccessibilityPermissionDenied) => {
                // Expected if running without accessibility permission
            }
            Err(InteractionError::ElementNotFound { .. }) => {
                // "File" menu might not exist if Finder isn't focused
            }
            Err(e) => {
                panic!("Unexpected error: {:?}", e);
            }
        }
    }
}
