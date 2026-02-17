use tracing::{info, warn};

use super::list::list_monitors;
use crate::window::errors::WindowError;
use crate::window::types::MonitorInfo;

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
