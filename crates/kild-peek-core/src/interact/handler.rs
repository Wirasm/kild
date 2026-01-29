use std::thread;
use std::time::Duration;

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use tracing::{debug, info};

use super::errors::InteractionError;
use super::keymap;
use super::types::{
    ClickRequest, InteractionResult, InteractionTarget, KeyComboRequest, TypeRequest,
};
use crate::window::{
    WindowError, WindowInfo, find_window_by_app, find_window_by_app_and_title, find_window_by_title,
};

// FFI declaration for accessibility check
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

/// Check if the current process has accessibility permissions
fn check_accessibility_permission() -> Result<(), InteractionError> {
    let trusted = unsafe { AXIsProcessTrusted() };
    if !trusted {
        return Err(InteractionError::AccessibilityPermissionDenied);
    }
    Ok(())
}

/// Resolve an InteractionTarget to a WindowInfo and focus the window
fn resolve_and_focus_window(target: &InteractionTarget) -> Result<WindowInfo, InteractionError> {
    let window = match target {
        InteractionTarget::Window { title } => {
            find_window_by_title(title).map_err(map_window_error)?
        }
        InteractionTarget::App { app } => find_window_by_app(app).map_err(map_window_error)?,
        InteractionTarget::AppAndWindow { app, title } => {
            find_window_by_app_and_title(app, title).map_err(map_window_error)?
        }
    };

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

/// Focus a window by app name using AppleScript
fn focus_window(app_name: &str) -> Result<(), InteractionError> {
    debug!(event = "peek.core.interact.focus_started", app = app_name);

    let script = format!(
        "tell application \"System Events\" to set frontmost of process \"{}\" to true",
        app_name
    );

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output();

    match output {
        Ok(result) => {
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                debug!(
                    event = "peek.core.interact.focus_failed",
                    app = app_name,
                    stderr = %stderr
                );
                // Don't fail the operation — focus is best-effort
            }
        }
        Err(e) => {
            debug!(
                event = "peek.core.interact.focus_failed",
                app = app_name,
                error = %e
            );
        }
    }

    Ok(())
}

/// Map WindowError to InteractionError
fn map_window_error(error: WindowError) -> InteractionError {
    match error {
        WindowError::WindowNotFound { title } => InteractionError::WindowNotFound { title },
        WindowError::WindowNotFoundByApp { app } => InteractionError::WindowNotFoundByApp { app },
        other => InteractionError::WindowNotFound {
            title: other.to_string(),
        },
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

/// Click at coordinates within a window
pub fn click(request: &ClickRequest) -> Result<InteractionResult, InteractionError> {
    info!(
        event = "peek.core.interact.click_started",
        x = request.x,
        y = request.y,
        target = ?request.target
    );

    check_accessibility_permission()?;

    let window = resolve_and_focus_window(&request.target)?;
    validate_coordinates(request.x, request.y, &window)?;

    let (screen_x, screen_y) = to_screen_coordinates(request.x, request.y, &window);
    let point = CGPoint::new(screen_x, screen_y);

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| InteractionError::EventSourceFailed)?;

    let mouse_down = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseDown,
        point,
        CGMouseButton::Left,
    )
    .map_err(|()| InteractionError::MouseEventFailed {
        x: screen_x,
        y: screen_y,
    })?;

    let mouse_up =
        CGEvent::new_mouse_event(source, CGEventType::LeftMouseUp, point, CGMouseButton::Left)
            .map_err(|()| InteractionError::MouseEventFailed {
                x: screen_x,
                y: screen_y,
            })?;

    mouse_down.post(CGEventTapLocation::HID);
    thread::sleep(MOUSE_EVENT_DELAY);
    mouse_up.post(CGEventTapLocation::HID);

    info!(
        event = "peek.core.interact.click_completed",
        screen_x = screen_x,
        screen_y = screen_y,
        window_title = window.title()
    );

    Ok(InteractionResult::success(
        "click",
        serde_json::json!({
            "x": request.x,
            "y": request.y,
            "screen_x": screen_x,
            "screen_y": screen_y,
            "window": window.title(),
        }),
    ))
}

/// Type text into the focused element of a window
pub fn type_text(request: &TypeRequest) -> Result<InteractionResult, InteractionError> {
    info!(
        event = "peek.core.interact.type_started",
        text_len = request.text.len(),
        target = ?request.target
    );

    check_accessibility_permission()?;

    let window = resolve_and_focus_window(&request.target)?;

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| InteractionError::EventSourceFailed)?;

    // Create a keyboard event and set the unicode string
    let event = CGEvent::new_keyboard_event(source, 0, true)
        .map_err(|()| InteractionError::KeyboardEventFailed { keycode: 0 })?;

    event.set_string(&request.text);
    event.post(CGEventTapLocation::HID);

    info!(
        event = "peek.core.interact.type_completed",
        text_len = request.text.len(),
        window_title = window.title()
    );

    Ok(InteractionResult::success(
        "type",
        serde_json::json!({
            "text_length": request.text.len(),
            "window": window.title(),
        }),
    ))
}

/// Send a key combination to a window
pub fn send_key_combo(request: &KeyComboRequest) -> Result<InteractionResult, InteractionError> {
    info!(
        event = "peek.core.interact.key_started",
        combo = &request.combo,
        target = ?request.target
    );

    check_accessibility_permission()?;

    let window = resolve_and_focus_window(&request.target)?;
    let mapping = keymap::parse_key_combo(&request.combo)?;

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| InteractionError::EventSourceFailed)?;

    // Key down
    let key_down =
        CGEvent::new_keyboard_event(source.clone(), mapping.keycode, true).map_err(|()| {
            InteractionError::KeyboardEventFailed {
                keycode: mapping.keycode,
            }
        })?;

    if mapping.flags != CGEventFlags::CGEventFlagNull {
        key_down.set_flags(mapping.flags);
    }

    // Key up
    let key_up = CGEvent::new_keyboard_event(source, mapping.keycode, false).map_err(|()| {
        InteractionError::KeyboardEventFailed {
            keycode: mapping.keycode,
        }
    })?;

    if mapping.flags != CGEventFlags::CGEventFlagNull {
        key_up.set_flags(mapping.flags);
    }

    key_down.post(CGEventTapLocation::HID);
    thread::sleep(KEY_EVENT_DELAY);
    key_up.post(CGEventTapLocation::HID);

    info!(
        event = "peek.core.interact.key_completed",
        combo = &request.combo,
        keycode = mapping.keycode,
        window_title = window.title()
    );

    Ok(InteractionResult::success(
        "key",
        serde_json::json!({
            "combo": &request.combo,
            "keycode": mapping.keycode,
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
            InteractionError::WindowNotFound { title } => {
                assert!(title.contains("test error"));
            }
            _ => panic!("Expected WindowNotFound fallback"),
        }
    }
}
