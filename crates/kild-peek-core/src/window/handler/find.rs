use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

use super::builders::build_window_info;
use super::list::list_windows;
use crate::window::errors::WindowError;
use crate::window::types::WindowInfo;

/// Generic polling function that retries until success or timeout
///
/// Polls every 100ms until the find function succeeds or timeout is reached.
/// Returns immediately if found on first attempt.
/// Propagates non-retryable errors immediately.
pub(super) fn poll_until_found<F, M, T>(
    timeout_ms: u64,
    find_fn: F,
    error_matcher: M,
    timeout_error: T,
) -> Result<WindowInfo, WindowError>
where
    F: Fn() -> Result<WindowInfo, WindowError>,
    M: Fn(WindowError) -> WindowError,
    T: Fn() -> WindowError,
{
    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    let poll_interval = Duration::from_millis(100);

    loop {
        match find_fn() {
            Ok(window) => return Ok(window),
            Err(e) => {
                let normalized = error_matcher(e);

                // Check if this is a retryable error
                let is_retryable = matches!(
                    normalized,
                    WindowError::WindowNotFound { .. } | WindowError::WindowNotFoundByApp { .. }
                );

                if !is_retryable {
                    return Err(normalized);
                }

                if start.elapsed() >= timeout {
                    return Err(timeout_error());
                }

                std::thread::sleep(poll_interval);
            }
        }
    }
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

/// Find a window by title, polling until found or timeout
///
/// Polls every 100ms until the window appears or the timeout is reached.
/// Returns immediately if the window is found on first attempt.
pub fn find_window_by_title_with_wait(
    title: &str,
    timeout_ms: u64,
) -> Result<WindowInfo, WindowError> {
    info!(
        event = "core.window.poll_started",
        title = title,
        timeout_ms = timeout_ms
    );

    let result = poll_until_found(
        timeout_ms,
        || find_window_by_title(title),
        |_| WindowError::WindowNotFound {
            title: title.to_string(),
        },
        || WindowError::WaitTimeoutByTitle {
            title: title.to_string(),
            timeout_ms,
        },
    );

    match &result {
        Ok(_) => {
            info!(event = "core.window.poll_completed", title = title);
        }
        Err(WindowError::WaitTimeoutByTitle { .. }) => {
            warn!(
                event = "core.window.poll_timeout",
                title = title,
                timeout_ms = timeout_ms
            );
        }
        _ => {}
    }

    result
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

/// Find a window by app name (exact match preferred, falls back to partial match)
///
/// Matching priority (returns first match at highest priority level):
/// 1. Exact case-insensitive match on app name
/// 2. Partial case-insensitive match on app name
pub fn find_window_by_app(app: &str) -> Result<WindowInfo, WindowError> {
    info!(event = "core.window.find_by_app_started", app = app);

    let app_lower = app.to_lowercase();

    let xcap_windows = xcap::Window::all().map_err(|e| WindowError::EnumerationFailed {
        message: e.to_string(),
    })?;

    let windows_with_props: Vec<_> = xcap_windows
        .into_iter()
        .filter_map(|w| {
            let id = w.id().ok()?;
            let window_title = w.title().unwrap_or_else(|e| {
                debug!(
                    event = "core.window.property_access_failed",
                    property = "title",
                    window_id = id,
                    error = %e
                );
                String::new()
            });
            let app_name = w.app_name().unwrap_or_else(|e| {
                debug!(
                    event = "core.window.property_access_failed",
                    property = "app_name",
                    window_id = id,
                    error = %e
                );
                String::new()
            });
            Some((w, window_title, app_name))
        })
        .collect();

    // Try exact app match first
    if let Some(result) = try_match_app(&windows_with_props, &app_lower, true, app) {
        return result;
    }
    // Fall back to partial app match
    if let Some(result) = try_match_app(&windows_with_props, &app_lower, false, app) {
        return result;
    }

    Err(WindowError::WindowNotFoundByApp {
        app: app.to_string(),
    })
}

/// Find a window by app name, polling until found or timeout
///
/// Polls every 100ms until the window appears or the timeout is reached.
/// Returns immediately if the window is found on first attempt.
pub fn find_window_by_app_with_wait(app: &str, timeout_ms: u64) -> Result<WindowInfo, WindowError> {
    info!(
        event = "core.window.poll_by_app_started",
        app = app,
        timeout_ms = timeout_ms
    );

    let result = poll_until_found(
        timeout_ms,
        || find_window_by_app(app),
        |_| WindowError::WindowNotFoundByApp {
            app: app.to_string(),
        },
        || WindowError::WaitTimeoutByApp {
            app: app.to_string(),
            timeout_ms,
        },
    );

    match &result {
        Ok(_) => {
            info!(event = "core.window.poll_by_app_completed", app = app);
        }
        Err(WindowError::WaitTimeoutByApp { .. }) => {
            warn!(
                event = "core.window.poll_by_app_timeout",
                app = app,
                timeout_ms = timeout_ms
            );
        }
        _ => {}
    }

    result
}

/// Find a window by app name and title (for precise matching)
///
/// First filters windows to those matching the app, then applies title matching
/// within that filtered set. Returns error if app has no windows or no window matches title.
pub fn find_window_by_app_and_title(app: &str, title: &str) -> Result<WindowInfo, WindowError> {
    info!(
        event = "core.window.find_by_app_and_title_started",
        app = app,
        title = title
    );

    let app_lower = app.to_lowercase();
    let title_lower = title.to_lowercase();

    let xcap_windows = xcap::Window::all().map_err(|e| WindowError::EnumerationFailed {
        message: e.to_string(),
    })?;

    // Collect all windows and filter to app matches
    let app_windows: Vec<_> = xcap_windows
        .into_iter()
        .filter_map(|w| {
            let id = w.id().ok()?;
            let window_title = w.title().unwrap_or_else(|e| {
                debug!(
                    event = "core.window.property_access_failed",
                    property = "title",
                    window_id = id,
                    error = %e
                );
                String::new()
            });
            let app_name = w.app_name().unwrap_or_else(|e| {
                debug!(
                    event = "core.window.property_access_failed",
                    property = "app_name",
                    window_id = id,
                    error = %e
                );
                String::new()
            });
            // Include if app matches (exact or partial)
            let app_name_lower = app_name.to_lowercase();
            if app_name_lower == app_lower || app_name_lower.contains(&app_lower) {
                Some((w, window_title, app_name))
            } else {
                None
            }
        })
        .collect();

    if app_windows.is_empty() {
        return Err(WindowError::WindowNotFoundByApp {
            app: app.to_string(),
        });
    }

    // Now apply title matching within app's windows
    // Priority: exact title > partial title
    if let Some(result) = try_match(&app_windows, &title_lower, MatchType::ExactTitle, title) {
        info!(
            event = "core.window.find_by_app_and_title_completed",
            app = app,
            title = title,
            match_type = "exact_title"
        );
        return result;
    }
    if let Some(result) = try_match(&app_windows, &title_lower, MatchType::PartialTitle, title) {
        info!(
            event = "core.window.find_by_app_and_title_completed",
            app = app,
            title = title,
            match_type = "partial_title"
        );
        return result;
    }

    Err(WindowError::WindowNotFound {
        title: title.to_string(),
    })
}

/// Find a window by app and title, polling until found or timeout
///
/// Polls every 100ms until the window appears or the timeout is reached.
/// Returns immediately if the window is found on first attempt.
pub fn find_window_by_app_and_title_with_wait(
    app: &str,
    title: &str,
    timeout_ms: u64,
) -> Result<WindowInfo, WindowError> {
    info!(
        event = "core.window.poll_by_app_and_title_started",
        app = app,
        title = title,
        timeout_ms = timeout_ms
    );

    let result = poll_until_found(
        timeout_ms,
        || find_window_by_app_and_title(app, title),
        |e| match e {
            WindowError::WindowNotFound { .. } => WindowError::WindowNotFound {
                title: title.to_string(),
            },
            WindowError::WindowNotFoundByApp { .. } => WindowError::WindowNotFoundByApp {
                app: app.to_string(),
            },
            other => other,
        },
        || WindowError::WaitTimeoutByAppAndTitle {
            app: app.to_string(),
            title: title.to_string(),
            timeout_ms,
        },
    );

    match &result {
        Ok(_) => {
            info!(
                event = "core.window.poll_by_app_and_title_completed",
                app = app,
                title = title
            );
        }
        Err(WindowError::WaitTimeoutByAppAndTitle { .. }) => {
            warn!(
                event = "core.window.poll_by_app_and_title_timeout",
                app = app,
                title = title,
                timeout_ms = timeout_ms
            );
        }
        _ => {}
    }

    result
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

/// Helper for app matching
fn try_match_app(
    windows: &[(xcap::Window, String, String)],
    app_lower: &str,
    exact: bool,
    original_app: &str,
) -> Option<Result<WindowInfo, WindowError>> {
    for (w, window_title, app_name) in windows {
        let app_name_lower = app_name.to_lowercase();
        let matches = match exact {
            true => app_name_lower == app_lower,
            false => app_name_lower.contains(app_lower),
        };

        if matches {
            let match_type = match exact {
                true => "exact_app",
                false => "partial_app",
            };
            info!(
                event = "core.window.find_by_app_completed",
                app = original_app,
                match_type = match_type
            );
            return Some(build_window_info(w, window_title, app_name, original_app));
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
