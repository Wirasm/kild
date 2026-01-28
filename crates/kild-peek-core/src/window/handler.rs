use tracing::info;

use super::errors::WindowError;
use super::types::{MonitorInfo, WindowInfo};

/// List all visible windows
pub fn list_windows() -> Result<Vec<WindowInfo>, WindowError> {
    info!(event = "core.window.list_started");

    let windows = xcap::Window::all().map_err(|e| WindowError::EnumerationFailed {
        message: e.to_string(),
    })?;

    let result: Vec<WindowInfo> = windows
        .into_iter()
        .filter_map(|w| {
            let id = w.id().ok()?;
            let x = w.x().ok()?;
            let y = w.y().ok()?;
            let width = w.width().ok()?;
            let height = w.height().ok()?;

            // Skip tiny windows (likely invisible/system windows)
            if width < 10 || height < 10 {
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

            let is_minimized = w.is_minimized().ok().unwrap_or(false);

            Some(WindowInfo {
                id,
                title: display_title,
                app_name,
                x,
                y,
                width,
                height,
                is_minimized,
            })
        })
        .collect();

    info!(event = "core.window.list_completed", count = result.len());
    Ok(result)
}

/// List all monitors
pub fn list_monitors() -> Result<Vec<MonitorInfo>, WindowError> {
    info!(event = "core.monitor.list_started");

    let monitors = xcap::Monitor::all().map_err(|e| WindowError::MonitorEnumerationFailed {
        message: e.to_string(),
    })?;

    let result: Vec<MonitorInfo> = monitors
        .into_iter()
        .enumerate()
        .filter_map(|(idx, m)| {
            let name = m.name().unwrap_or_else(|_| format!("Monitor {}", idx));
            let x = m.x().ok()?;
            let y = m.y().ok()?;
            let width = m.width().ok()?;
            let height = m.height().ok()?;
            let is_primary = m.is_primary().unwrap_or(false);

            Some(MonitorInfo {
                id: idx as u32,
                name,
                x,
                y,
                width,
                height,
                is_primary,
            })
        })
        .collect();

    info!(event = "core.monitor.list_completed", count = result.len());
    Ok(result)
}

/// Find a window by title (partial match, case-insensitive)
/// Searches both window title and app name
pub fn find_window_by_title(title: &str) -> Result<WindowInfo, WindowError> {
    info!(event = "core.window.find_started", title = title);

    let title_lower = title.to_lowercase();

    // Search through all xcap windows directly for maximum coverage
    let xcap_windows = xcap::Window::all().map_err(|e| WindowError::EnumerationFailed {
        message: e.to_string(),
    })?;

    for w in xcap_windows {
        let window_title = w.title().ok().unwrap_or_default();
        let app_name = w.app_name().ok().unwrap_or_default();

        // Match against both title and app_name
        let matches = window_title.to_lowercase().contains(&title_lower)
            || app_name.to_lowercase().contains(&title_lower);

        if matches {
            let id = w.id().ok().ok_or_else(|| WindowError::WindowNotFound {
                title: title.to_string(),
            })?;
            let x = w.x().ok().unwrap_or(0);
            let y = w.y().ok().unwrap_or(0);
            let width = w.width().ok().unwrap_or(0);
            let height = w.height().ok().unwrap_or(0);
            let is_minimized = w.is_minimized().ok().unwrap_or(false);

            let display_title = if window_title.is_empty() {
                if app_name.is_empty() {
                    format!("[Window {}]", id)
                } else {
                    app_name.clone()
                }
            } else {
                window_title
            };

            info!(
                event = "core.window.find_completed",
                title = title,
                found_id = id
            );

            return Ok(WindowInfo {
                id,
                title: display_title,
                app_name,
                x,
                y,
                width,
                height,
                is_minimized,
            });
        }
    }

    Err(WindowError::WindowNotFound {
        title: title.to_string(),
    })
}

/// Find a window by its ID
pub fn find_window_by_id(id: u32) -> Result<WindowInfo, WindowError> {
    info!(event = "core.window.find_by_id_started", id = id);

    let windows = list_windows()?;

    let window = windows
        .into_iter()
        .find(|w| w.id == id)
        .ok_or(WindowError::WindowNotFoundById { id })?;

    info!(
        event = "core.window.find_by_id_completed",
        id = id,
        title = window.title
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
        name = monitor.name
    );
    Ok(monitor)
}

/// Get the primary monitor
pub fn get_primary_monitor() -> Result<MonitorInfo, WindowError> {
    info!(event = "core.monitor.get_primary_started");

    let monitors = list_monitors()?;

    let monitor = monitors
        .into_iter()
        .find(|m| m.is_primary)
        .or_else(|| {
            // Fall back to first monitor if no primary is set
            list_monitors().ok().and_then(|m| m.into_iter().next())
        })
        .ok_or(WindowError::MonitorNotFound { index: 0 })?;

    info!(
        event = "core.monitor.get_primary_completed",
        name = monitor.name
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
}
