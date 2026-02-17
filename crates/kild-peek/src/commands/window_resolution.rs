use kild_peek_core::errors::PeekError;
use kild_peek_core::events;
use kild_peek_core::window::{
    find_window_by_app, find_window_by_app_and_title, find_window_by_app_and_title_with_wait,
    find_window_by_app_with_wait, find_window_by_title_with_wait,
};
use tracing::error;

/// Resolve window title from app name and/or window title
pub(crate) fn resolve_window_title(
    app_name: Option<&String>,
    window_title: Option<&String>,
) -> Result<String, Box<dyn std::error::Error>> {
    resolve_window_title_impl(app_name, window_title, None)
}

/// Resolve window title with wait support
pub(crate) fn resolve_window_title_with_wait(
    app_name: Option<&String>,
    window_title: Option<&String>,
    timeout_ms: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    resolve_window_title_impl(app_name, window_title, Some(timeout_ms))
}

/// Implementation for window title resolution with optional wait
fn resolve_window_title_impl(
    app_name: Option<&String>,
    window_title: Option<&String>,
    timeout_ms: Option<u64>,
) -> Result<String, Box<dyn std::error::Error>> {
    if app_name.is_none() && window_title.is_none() {
        return Ok(String::new());
    }

    // If only window title provided and no wait, return title directly without lookup
    if app_name.is_none()
        && timeout_ms.is_none()
        && let Some(title) = window_title
    {
        return Ok(title.clone());
    }

    // Find window based on provided parameters
    let window = find_window_with_params(app_name, window_title, timeout_ms).map_err(|e| {
        log_window_resolution_error(&e, app_name, window_title);
        format_window_resolution_error(&e, app_name, window_title)
    })?;

    Ok(window.title().to_string())
}

/// Find window with the given parameters
fn find_window_with_params(
    app_name: Option<&String>,
    window_title: Option<&String>,
    timeout_ms: Option<u64>,
) -> Result<kild_peek_core::window::WindowInfo, kild_peek_core::window::WindowError> {
    match (app_name, window_title, timeout_ms) {
        (Some(app), Some(title), Some(timeout)) => {
            find_window_by_app_and_title_with_wait(app, title, timeout)
        }
        (Some(app), Some(title), None) => find_window_by_app_and_title(app, title),
        (Some(app), None, Some(timeout)) => find_window_by_app_with_wait(app, timeout),
        (Some(app), None, None) => find_window_by_app(app),
        (None, Some(title), Some(timeout)) => find_window_by_title_with_wait(title, timeout),
        (None, Some(_), None) => unreachable!("handled in early return"),
        (None, None, _) => unreachable!("handled in early return"),
    }
}

/// Resolve a window for capture, with optional wait timeout
pub(crate) fn resolve_window_for_capture(
    app_name: Option<&String>,
    window_title: Option<&String>,
    timeout_ms: Option<u64>,
) -> Result<kild_peek_core::window::WindowInfo, Box<dyn std::error::Error>> {
    let timeout = timeout_ms.expect("resolve_window_for_capture requires timeout");

    let window = find_window_with_wait(app_name, window_title, timeout).map_err(|e| {
        log_capture_window_resolution_error(&e, app_name, window_title);
        format_window_resolution_error(&e, app_name, window_title)
    })?;

    Ok(window)
}

/// Find window with wait based on app and/or title
fn find_window_with_wait(
    app_name: Option<&String>,
    window_title: Option<&String>,
    timeout: u64,
) -> Result<kild_peek_core::window::WindowInfo, kild_peek_core::window::WindowError> {
    match (app_name, window_title) {
        (Some(app), Some(title)) => find_window_by_app_and_title_with_wait(app, title, timeout),
        (Some(app), None) => find_window_by_app_with_wait(app, timeout),
        (None, Some(title)) => find_window_by_title_with_wait(title, timeout),
        (None, None) => unreachable!("at least one of app or title must be provided"),
    }
}

/// Log window resolution error
fn log_window_resolution_error(
    error: &kild_peek_core::window::WindowError,
    app_name: Option<&String>,
    window_title: Option<&String>,
) {
    error!(
        event = "peek.cli.assert_window_resolution_failed",
        app = ?app_name,
        title = ?window_title,
        error = %error,
        error_code = error.error_code()
    );
    events::log_app_error(error);
}

/// Log window resolution error for capture operations
fn log_capture_window_resolution_error(
    error: &kild_peek_core::window::WindowError,
    app_name: Option<&String>,
    window_title: Option<&String>,
) {
    error!(
        event = "peek.cli.assert_similar_window_resolution_failed",
        app = ?app_name,
        title = ?window_title,
        error = %error,
        error_code = error.error_code()
    );
    events::log_app_error(error);
}

/// Format window resolution error message
fn format_window_resolution_error(
    error: &kild_peek_core::window::WindowError,
    app_name: Option<&String>,
    window_title: Option<&String>,
) -> String {
    match (app_name, window_title) {
        (Some(app), Some(title)) => {
            format!(
                "Window not found for app '{}' with title '{}': {}",
                app, title, error
            )
        }
        (Some(app), None) => format!("Window not found for app '{}': {}", app, error),
        (None, Some(title)) => format!("Window not found with title '{}': {}", title, error),
        (None, None) => format!("Window resolution error: {}", error),
    }
}
