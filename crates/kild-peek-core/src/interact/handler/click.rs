use std::thread;

use core_graphics::geometry::CGPoint;
use tracing::{debug, info};

use crate::interact::errors::InteractionError;
use crate::interact::types::{ClickRequest, ClickTextRequest, InteractionResult};

use super::helpers::{
    FOCUS_SETTLE_DELAY, check_accessibility_permission, click_action_name,
    create_and_post_mouse_click, find_window_by_target, focus_window, resolve_and_focus_window,
    to_screen_coordinates, validate_coordinates,
};

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
