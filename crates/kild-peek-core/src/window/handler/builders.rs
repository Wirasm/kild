use tracing::debug;

use crate::window::errors::WindowError;
use crate::window::types::WindowInfo;

/// Helper to build WindowInfo from xcap window and pre-fetched properties
///
/// Returns WindowNotFound error if the window ID cannot be retrieved.
/// Falls back to 0 for position and 1 for dimensions if properties are unavailable.
pub(super) fn build_window_info(
    w: &xcap::Window,
    window_title: &str,
    app_name: &str,
    search_title: &str,
) -> Result<WindowInfo, WindowError> {
    let id = w.id().ok().ok_or_else(|| WindowError::WindowNotFound {
        title: search_title.to_string(),
    })?;

    let x = get_window_property_i32(w, "x", id, |w| w.x(), 0);
    let y = get_window_property_i32(w, "y", id, |w| w.y(), 0);
    let width = get_window_property_u32(w, "width", id, |w| w.width(), 0);
    let height = get_window_property_u32(w, "height", id, |w| w.height(), 0);

    let is_minimized = w.is_minimized().unwrap_or_else(|e| {
        debug!(
            event = "core.window.is_minimized_check_failed",
            window_id = id,
            error = %e
        );
        false
    });

    let pid = w.pid().ok().map(|p| p as i32);

    let display_title = build_display_title(window_title, app_name, id);

    Ok(WindowInfo::new(
        id,
        display_title,
        app_name.to_string(),
        x,
        y,
        width.max(1),
        height.max(1),
        is_minimized,
        pid,
    ))
}

/// Get an i32 window property with fallback and debug logging
fn get_window_property_i32<F>(w: &xcap::Window, name: &str, id: u32, getter: F, default: i32) -> i32
where
    F: FnOnce(&xcap::Window) -> Result<i32, xcap::XCapError>,
{
    getter(w).unwrap_or_else(|e| {
        debug!(
            event = "core.window.property_access_failed",
            property = name,
            window_id = id,
            error = %e
        );
        default
    })
}

/// Get a u32 window property with fallback and debug logging
fn get_window_property_u32<F>(w: &xcap::Window, name: &str, id: u32, getter: F, default: u32) -> u32
where
    F: FnOnce(&xcap::Window) -> Result<u32, xcap::XCapError>,
{
    getter(w).unwrap_or_else(|e| {
        debug!(
            event = "core.window.property_access_failed",
            property = name,
            window_id = id,
            error = %e
        );
        default
    })
}

/// Build a display title from window title and app name
fn build_display_title(window_title: &str, app_name: &str, window_id: u32) -> String {
    if !window_title.is_empty() {
        return window_title.to_string();
    }

    if !app_name.is_empty() {
        return app_name.to_string();
    }

    format!("[Window {}]", window_id)
}
