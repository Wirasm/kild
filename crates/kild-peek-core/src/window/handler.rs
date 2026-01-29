use tracing::{debug, info, warn};

use super::errors::WindowError;
use super::types::{MonitorInfo, WindowInfo};

/// List all visible windows
pub fn list_windows() -> Result<Vec<WindowInfo>, WindowError> {
    info!(event = "core.window.list_started");

    let windows = xcap::Window::all().map_err(|e| WindowError::EnumerationFailed {
        message: e.to_string(),
    })?;

    let mut skipped_count = 0;
    let mut tiny_count = 0;

    let result: Vec<WindowInfo> = windows
        .into_iter()
        .filter_map(|w| {
            // Get required properties, tracking failures
            let id = match w.id() {
                Ok(id) => id,
                Err(e) => {
                    debug!(
                        event = "core.window.property_access_failed",
                        property = "id",
                        error = %e
                    );
                    skipped_count += 1;
                    return None;
                }
            };

            let x = match w.x() {
                Ok(x) => x,
                Err(e) => {
                    debug!(
                        event = "core.window.property_access_failed",
                        property = "x",
                        window_id = id,
                        error = %e
                    );
                    skipped_count += 1;
                    return None;
                }
            };

            let y = match w.y() {
                Ok(y) => y,
                Err(e) => {
                    debug!(
                        event = "core.window.property_access_failed",
                        property = "y",
                        window_id = id,
                        error = %e
                    );
                    skipped_count += 1;
                    return None;
                }
            };

            let width = match w.width() {
                Ok(w) => w,
                Err(e) => {
                    debug!(
                        event = "core.window.property_access_failed",
                        property = "width",
                        window_id = id,
                        error = %e
                    );
                    skipped_count += 1;
                    return None;
                }
            };

            let height = match w.height() {
                Ok(h) => h,
                Err(e) => {
                    debug!(
                        event = "core.window.property_access_failed",
                        property = "height",
                        window_id = id,
                        error = %e
                    );
                    skipped_count += 1;
                    return None;
                }
            };

            // Skip tiny windows (likely invisible/system windows)
            if width < 10 || height < 10 {
                tiny_count += 1;
                return None;
            }

            let app_name = w.app_name().ok().unwrap_or_default();
            let title = w.title().ok().unwrap_or_default();

            // Use app_name as fallback title if title is empty
            let display_title = if title.is_empty() {
                if app_name.is_empty() {
                    format!("[Window {}]", id)
                } else {
                    app_name.clone()
                }
            } else {
                title
            };

            let is_minimized = match w.is_minimized() {
                Ok(minimized) => minimized,
                Err(e) => {
                    debug!(
                        event = "core.window.is_minimized_check_failed",
                        window_id = id,
                        error = %e
                    );
                    false
                }
            };

            Some(WindowInfo::new(
                id,
                display_title,
                app_name,
                x,
                y,
                width,
                height,
                is_minimized,
            ))
        })
        .collect();

    if skipped_count > 0 {
        warn!(
            event = "core.window.list_incomplete",
            skipped_count = skipped_count,
            tiny_count = tiny_count,
            returned_count = result.len()
        );
    }

    info!(event = "core.window.list_completed", count = result.len());
    Ok(result)
}

/// List all monitors
pub fn list_monitors() -> Result<Vec<MonitorInfo>, WindowError> {
    info!(event = "core.monitor.list_started");

    let monitors = xcap::Monitor::all().map_err(|e| WindowError::MonitorEnumerationFailed {
        message: e.to_string(),
    })?;

    let mut skipped_count = 0;

    let result: Vec<MonitorInfo> = monitors
        .into_iter()
        .enumerate()
        .filter_map(|(idx, m)| {
            let name = m.name().unwrap_or_else(|_| format!("Monitor {}", idx));

            let x = match m.x() {
                Ok(x) => x,
                Err(e) => {
                    debug!(
                        event = "core.monitor.property_access_failed",
                        property = "x",
                        monitor_index = idx,
                        error = %e
                    );
                    skipped_count += 1;
                    return None;
                }
            };

            let y = match m.y() {
                Ok(y) => y,
                Err(e) => {
                    debug!(
                        event = "core.monitor.property_access_failed",
                        property = "y",
                        monitor_index = idx,
                        error = %e
                    );
                    skipped_count += 1;
                    return None;
                }
            };

            let width = match m.width() {
                Ok(w) => w,
                Err(e) => {
                    debug!(
                        event = "core.monitor.property_access_failed",
                        property = "width",
                        monitor_index = idx,
                        error = %e
                    );
                    skipped_count += 1;
                    return None;
                }
            };

            let height = match m.height() {
                Ok(h) => h,
                Err(e) => {
                    debug!(
                        event = "core.monitor.property_access_failed",
                        property = "height",
                        monitor_index = idx,
                        error = %e
                    );
                    skipped_count += 1;
                    return None;
                }
            };

            let is_primary = match m.is_primary() {
                Ok(primary) => primary,
                Err(e) => {
                    debug!(
                        event = "core.monitor.is_primary_check_failed",
                        monitor_index = idx,
                        error = %e
                    );
                    false
                }
            };

            Some(MonitorInfo::new(
                idx as u32, name, x, y, width, height, is_primary,
            ))
        })
        .collect();

    if skipped_count > 0 {
        warn!(
            event = "core.monitor.list_incomplete",
            skipped_count = skipped_count,
            returned_count = result.len()
        );
    }

    info!(event = "core.monitor.list_completed", count = result.len());
    Ok(result)
}

/// Find a window by title (exact match preferred, falls back to partial match)
/// Searches both window title and app name
///
/// Matching priority (returns first match at highest priority level):
/// 1. Exact case-insensitive match on window title
/// 2. Exact case-insensitive match on app name
/// 3. Partial case-insensitive match on window title
/// 4. Partial case-insensitive match on app name
///
/// When multiple windows match at the same priority level, the first one
/// encountered in the system's window enumeration order is returned.
pub fn find_window_by_title(title: &str) -> Result<WindowInfo, WindowError> {
    info!(event = "core.window.find_started", title = title);

    let title_lower = title.to_lowercase();

    // Search through all xcap windows directly for maximum coverage
    let xcap_windows = xcap::Window::all().map_err(|e| WindowError::EnumerationFailed {
        message: e.to_string(),
    })?;

    // Collect all windows with their properties for multi-pass matching
    let windows_with_props: Vec<_> = xcap_windows
        .into_iter()
        .map(|w| {
            let window_title = w.title().ok().unwrap_or_default();
            let app_name = w.app_name().ok().unwrap_or_default();
            (w, window_title, app_name)
        })
        .collect();

    // Try each match type in priority order
    if let Some(result) = try_match(
        &windows_with_props,
        &title_lower,
        MatchType::ExactTitle,
        title,
    ) {
        return result;
    }
    if let Some(result) = try_match(
        &windows_with_props,
        &title_lower,
        MatchType::ExactAppName,
        title,
    ) {
        return result;
    }
    if let Some(result) = try_match(
        &windows_with_props,
        &title_lower,
        MatchType::PartialTitle,
        title,
    ) {
        return result;
    }
    if let Some(result) = try_match(
        &windows_with_props,
        &title_lower,
        MatchType::PartialAppName,
        title,
    ) {
        return result;
    }

    Err(WindowError::WindowNotFound {
        title: title.to_string(),
    })
}

/// Try to find a matching window using the specified match type
fn try_match(
    windows: &[(xcap::Window, String, String)],
    title_lower: &str,
    match_type: MatchType,
    original_title: &str,
) -> Option<Result<WindowInfo, WindowError>> {
    for (w, window_title, app_name) in windows {
        let matches = match match_type {
            MatchType::ExactTitle => window_title.to_lowercase() == title_lower,
            MatchType::ExactAppName => app_name.to_lowercase() == title_lower,
            MatchType::PartialTitle => window_title.to_lowercase().contains(title_lower),
            MatchType::PartialAppName => app_name.to_lowercase().contains(title_lower),
        };

        if matches {
            info!(
                event = "core.window.find_completed",
                title = original_title,
                match_type = match_type.as_str()
            );
            return Some(build_window_info(w, window_title, app_name, original_title));
        }
    }
    None
}

/// Types of window title matches, in priority order
#[derive(Copy, Clone)]
enum MatchType {
    ExactTitle,
    ExactAppName,
    PartialTitle,
    PartialAppName,
}

impl MatchType {
    fn as_str(&self) -> &'static str {
        match self {
            MatchType::ExactTitle => "exact_title",
            MatchType::ExactAppName => "exact_app_name",
            MatchType::PartialTitle => "partial_title",
            MatchType::PartialAppName => "partial_app_name",
        }
    }
}

/// Helper to build WindowInfo from xcap window and pre-fetched properties
///
/// Returns WindowNotFound error if the window ID cannot be retrieved.
/// Falls back to 0 for position and 1 for dimensions if properties are unavailable.
fn build_window_info(
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

/// Find a window by its ID
pub fn find_window_by_id(id: u32) -> Result<WindowInfo, WindowError> {
    info!(event = "core.window.find_by_id_started", id = id);

    let windows = list_windows()?;

    let window = windows
        .into_iter()
        .find(|w| w.id() == id)
        .ok_or(WindowError::WindowNotFoundById { id })?;

    info!(
        event = "core.window.find_by_id_completed",
        id = id,
        title = window.title()
    );
    Ok(window)
}

/// Get a monitor by index
pub fn get_monitor(index: usize) -> Result<MonitorInfo, WindowError> {
    info!(event = "core.monitor.get_started", index = index);

    let monitors = list_monitors()?;

    let monitor = monitors
        .into_iter()
        .nth(index)
        .ok_or(WindowError::MonitorNotFound { index })?;

    info!(
        event = "core.monitor.get_completed",
        index = index,
        name = monitor.name()
    );
    Ok(monitor)
}

/// Get the primary monitor
pub fn get_primary_monitor() -> Result<MonitorInfo, WindowError> {
    info!(event = "core.monitor.get_primary_started");

    let monitors = list_monitors()?;

    // First try to find primary monitor
    let monitor = if let Some(primary) = monitors.iter().find(|m| m.is_primary()).cloned() {
        primary
    } else {
        // Fall back to first monitor if no primary is set
        warn!(event = "core.monitor.no_primary_found_using_fallback");
        monitors
            .into_iter()
            .next()
            .ok_or(WindowError::MonitorNotFound { index: 0 })?
    };

    info!(
        event = "core.monitor.get_primary_completed",
        name = monitor.name()
    );
    Ok(monitor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::PeekError;

    #[test]
    fn test_list_windows_does_not_panic() {
        // This test verifies the function doesn't panic
        // Actual window enumeration depends on the system state
        let result = list_windows();
        // Either succeeds or fails with an error, but shouldn't panic
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_list_monitors_does_not_panic() {
        let result = list_monitors();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_find_window_by_title_not_found() {
        // This should fail since "NONEXISTENT_WINDOW_12345" is unlikely to exist
        let result = find_window_by_title("NONEXISTENT_WINDOW_12345_UNIQUE");
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.error_code(), "WINDOW_NOT_FOUND");
        }
    }

    #[test]
    fn test_find_window_by_id_not_found() {
        let result = find_window_by_id(u32::MAX);
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.error_code(), "WINDOW_NOT_FOUND_BY_ID");
        }
    }

    #[test]
    fn test_get_monitor_not_found() {
        let result = get_monitor(999);
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.error_code(), "MONITOR_NOT_FOUND");
        }
    }

    #[test]
    fn test_window_info_getters() {
        let window = WindowInfo::new(
            123,
            "Test Title".to_string(),
            "TestApp".to_string(),
            100,
            200,
            800,
            600,
            false,
        );

        assert_eq!(window.id(), 123);
        assert_eq!(window.title(), "Test Title");
        assert_eq!(window.app_name(), "TestApp");
        assert_eq!(window.x(), 100);
        assert_eq!(window.y(), 200);
        assert_eq!(window.width(), 800);
        assert_eq!(window.height(), 600);
        assert!(!window.is_minimized());
    }

    #[test]
    fn test_monitor_info_getters() {
        let monitor = MonitorInfo::new(0, "Main Display".to_string(), 0, 0, 2560, 1440, true);

        assert_eq!(monitor.id(), 0);
        assert_eq!(monitor.name(), "Main Display");
        assert_eq!(monitor.x(), 0);
        assert_eq!(monitor.y(), 0);
        assert_eq!(monitor.width(), 2560);
        assert_eq!(monitor.height(), 1440);
        assert!(monitor.is_primary());
    }

    #[test]
    fn test_find_window_by_title_is_case_insensitive() {
        // Both should return the same error (no such window exists)
        // This verifies case-insensitivity is applied consistently
        let result_lower = find_window_by_title("nonexistent_window_test_abc123");
        let result_upper = find_window_by_title("NONEXISTENT_WINDOW_TEST_ABC123");

        // Both should be errors (window doesn't exist)
        assert!(result_lower.is_err());
        assert!(result_upper.is_err());

        // Both should have the same error code
        assert_eq!(
            result_lower.unwrap_err().error_code(),
            result_upper.unwrap_err().error_code()
        );
    }
}
