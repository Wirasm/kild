use tracing::{debug, info, warn};

use crate::window::errors::WindowError;
use crate::window::types::{MonitorInfo, WindowInfo};

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

            let pid = w.pid().ok().map(|p| p as i32);

            Some(WindowInfo::new(
                id,
                display_title,
                app_name,
                x,
                y,
                width,
                height,
                is_minimized,
                pid,
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
            // Use catch_unwind to protect against xcap/objc2-app-kit panics
            // when NSScreen.localizedName returns NULL on headless macOS (e.g. CI)
            let name = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| m.name())) {
                Ok(Ok(n)) => n,
                Ok(Err(e)) => {
                    debug!(
                        event = "core.monitor.property_access_failed",
                        property = "name",
                        monitor_index = idx,
                        error = %e
                    );
                    format!("Monitor {}", idx)
                }
                Err(_) => {
                    debug!(
                        event = "core.monitor.name_panic_caught",
                        monitor_index = idx,
                    );
                    format!("Monitor {}", idx)
                }
            };

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
