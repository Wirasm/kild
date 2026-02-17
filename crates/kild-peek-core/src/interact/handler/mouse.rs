use std::thread;

use core_graphics::event::{
    CGEvent, CGEventTapLocation, CGEventType, CGMouseButton, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use tracing::{debug, info};

use crate::interact::errors::InteractionError;
use crate::interact::types::{
    DragRequest, HoverRequest, HoverTextRequest, InteractionResult, ScrollRequest,
};

use super::helpers::{
    DRAG_EVENT_DELAY, FOCUS_SETTLE_DELAY, MOUSE_EVENT_DELAY, check_accessibility_permission,
    find_window_by_target, focus_window, resolve_and_focus_window, to_screen_coordinates,
    validate_coordinates,
};

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
