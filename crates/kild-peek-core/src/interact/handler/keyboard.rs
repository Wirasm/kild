use std::thread;

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use tracing::{debug, info};

use crate::interact::errors::InteractionError;
use crate::interact::operations;
use crate::interact::types::{InteractionResult, KeyComboRequest, TypeRequest};

use super::helpers::{
    CHAR_EVENT_DELAY, KEY_EVENT_DELAY, check_accessibility_permission, resolve_and_focus_window,
};

/// Type text into the focused element of a window
///
/// Focuses the target window, then sends each character as an individual
/// CGEvent with a small delay between them. GPUI and other Metal-based apps
/// only read the first character from a CGEvent's unicode string, so we must
/// send one event per character.
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

    // Send each character as an individual keyboard event.
    // GPUI and other Metal-based apps only read the first character from a
    // CGEvent's unicode string, so we must send one event per character.
    debug!(
        event = "peek.core.interact.type_posting",
        text_len = request.text().len()
    );
    for ch in request.text().chars() {
        let event = CGEvent::new_keyboard_event(source.clone(), 0, true)
            .map_err(|()| InteractionError::KeyboardEventFailed { keycode: 0 })?;
        event.set_string(&ch.to_string());
        event.post(CGEventTapLocation::HID);
        thread::sleep(CHAR_EVENT_DELAY);
    }

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
