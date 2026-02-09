use std::ffi::c_void;
use std::ptr;

use accessibility_sys::{
    AXError, AXUIElementCopyAttributeValue, AXUIElementCreateApplication, AXUIElementRef,
    AXUIElementSetAttributeValue, AXUIElementSetMessagingTimeout, kAXErrorSuccess,
    kAXMinimizedAttribute, kAXTitleAttribute, kAXWindowsAttribute,
};
use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::string::CFString;
use tracing::{debug, warn};

use super::types::NativeWindowInfo;
use crate::terminal::errors::TerminalError;

/// Timeout for AX messaging (seconds)
const AX_MESSAGING_TIMEOUT: f32 = 1.0;

/// Find a window by app name and partial title match using Core Graphics API (via xcap).
///
/// Enumerates all visible windows, filters to those belonging to `app_name`,
/// then finds one whose title contains `title_contains` (case-insensitive).
pub fn find_window(
    app_name: &str,
    title_contains: &str,
) -> Result<Option<NativeWindowInfo>, TerminalError> {
    debug!(
        event = "core.terminal.native.find_window_started",
        app_name = app_name,
        title_contains = title_contains
    );

    let windows = xcap::Window::all().map_err(|e| TerminalError::NativeWindowError {
        message: format!("Failed to enumerate windows via Core Graphics: {}", e),
    })?;

    let app_lower = app_name.to_lowercase();
    let title_lower = title_contains.to_lowercase();

    for w in windows {
        let w_app = match w.app_name() {
            Ok(name) => name,
            Err(_) => continue,
        };

        if !w_app.to_lowercase().contains(&app_lower) {
            continue;
        }

        let w_title = w.title().unwrap_or_default();
        if !w_title.to_lowercase().contains(&title_lower) {
            continue;
        }

        let id = match w.id() {
            Ok(id) => id,
            Err(_) => continue,
        };

        let is_minimized = w.is_minimized().unwrap_or(false);
        let pid = w.pid().ok().map(|p| p as i32);

        debug!(
            event = "core.terminal.native.find_window_found",
            window_id = id,
            title = %w_title,
            app_name = %w_app,
            pid = ?pid,
            is_minimized = is_minimized
        );

        return Ok(Some(NativeWindowInfo {
            id,
            title: w_title,
            app_name: w_app,
            pid,
            is_minimized,
        }));
    }

    debug!(
        event = "core.terminal.native.find_window_not_found",
        app_name = app_name,
        title_contains = title_contains
    );

    Ok(None)
}

/// Find a window by app name and PID using Core Graphics API (via xcap).
///
/// Enumerates all visible windows, filters to those belonging to `app_name`
/// with matching PID.
pub fn find_window_by_pid(
    app_name: &str,
    pid: u32,
) -> Result<Option<NativeWindowInfo>, TerminalError> {
    debug!(
        event = "core.terminal.native.find_window_by_pid_started",
        app_name = app_name,
        pid = pid
    );

    let windows = xcap::Window::all().map_err(|e| TerminalError::NativeWindowError {
        message: format!("Failed to enumerate windows via Core Graphics: {}", e),
    })?;

    let app_lower = app_name.to_lowercase();

    for w in windows {
        let w_app = match w.app_name() {
            Ok(name) => name,
            Err(_) => continue,
        };

        if !w_app.to_lowercase().contains(&app_lower) {
            continue;
        }

        let w_pid = match w.pid() {
            Ok(p) => p,
            Err(_) => continue,
        };

        if w_pid != pid {
            continue;
        }

        let id = match w.id() {
            Ok(id) => id,
            Err(_) => continue,
        };

        let title = w.title().unwrap_or_default();
        let is_minimized = w.is_minimized().unwrap_or(false);

        debug!(
            event = "core.terminal.native.find_window_by_pid_found",
            window_id = id,
            title = %title,
            pid = w_pid
        );

        return Ok(Some(NativeWindowInfo {
            id,
            title,
            app_name: w_app,
            pid: Some(w_pid as i32),
            is_minimized,
        }));
    }

    debug!(
        event = "core.terminal.native.find_window_by_pid_not_found",
        app_name = app_name,
        pid = pid
    );

    Ok(None)
}

/// Focus (raise) a specific window using the macOS Accessibility API.
///
/// Uses AXUIElementCreateApplication(pid) to get the app's AX element,
/// then iterates its windows to find the one matching the window title,
/// and performs AXRaise + app activation to bring it to front.
///
/// If the Accessibility API fails (Ghostty may not expose AX windows due to
/// GPU rendering), falls back to `tell application "Ghostty" to activate`.
pub fn focus_window(window: &NativeWindowInfo) -> Result<(), TerminalError> {
    let pid = window.pid.ok_or_else(|| TerminalError::NativeWindowError {
        message: "Cannot focus window: no PID available".to_string(),
    })?;

    debug!(
        event = "core.terminal.native.focus_started",
        window_id = window.id,
        title = %window.title,
        pid = pid
    );

    // Try Accessibility API first
    match ax_raise_window(pid, &window.title) {
        Ok(()) => {
            debug!(
                event = "core.terminal.native.focus_ax_succeeded",
                window_id = window.id,
                pid = pid
            );
        }
        Err(e) => {
            // AX failed — fall back to AppleScript activation (brings app to front,
            // just can't target specific window)
            warn!(
                event = "core.terminal.native.focus_ax_failed_fallback",
                window_id = window.id,
                pid = pid,
                error = %e,
                message = "Accessibility API failed, falling back to app activation"
            );
        }
    }

    // Always activate the app to bring it to the foreground
    activate_app(&window.app_name)?;

    Ok(())
}

/// Minimize a specific window using the macOS Accessibility API.
///
/// Uses AXUIElementCreateApplication(pid) to get the app's AX element,
/// then sets kAXMinimizedAttribute to true on the matching window.
///
/// If the Accessibility API fails, falls back to hiding the app via System Events.
pub fn minimize_window(window: &NativeWindowInfo) -> Result<(), TerminalError> {
    let pid = window.pid.ok_or_else(|| TerminalError::NativeWindowError {
        message: "Cannot minimize window: no PID available".to_string(),
    })?;

    debug!(
        event = "core.terminal.native.minimize_started",
        window_id = window.id,
        title = %window.title,
        pid = pid
    );

    // Try Accessibility API first
    match ax_minimize_window(pid, &window.title) {
        Ok(()) => {
            debug!(
                event = "core.terminal.native.minimize_ax_succeeded",
                window_id = window.id,
                pid = pid
            );
            return Ok(());
        }
        Err(e) => {
            warn!(
                event = "core.terminal.native.minimize_ax_failed_fallback",
                window_id = window.id,
                pid = pid,
                error = %e,
                message = "Accessibility API failed, falling back to System Events hide"
            );
        }
    }

    // Fallback: hide via System Events (hides all windows of the app)
    hide_app_via_system_events(&window.app_name)
}

/// Raise a window via the Accessibility API by matching its title.
fn ax_raise_window(pid: i32, title: &str) -> Result<(), String> {
    // SAFETY: AXUIElementCreateApplication creates a +1 retained AXUIElementRef.
    let app_element = unsafe { AXUIElementCreateApplication(pid) };
    if app_element.is_null() {
        return Err(format!("Failed to create AX element for PID {}", pid));
    }

    // SAFETY: app_element is a valid AXUIElementRef we just created.
    unsafe {
        AXUIElementSetMessagingTimeout(app_element, AX_MESSAGING_TIMEOUT);
    }

    let result = ax_find_and_act_on_window(app_element, title, WindowAction::Raise);

    // SAFETY: Release the app element (Create Rule — we own it).
    unsafe {
        core_foundation::base::CFRelease(app_element as *mut c_void);
    }

    result
}

/// Minimize a window via the Accessibility API by matching its title.
fn ax_minimize_window(pid: i32, title: &str) -> Result<(), String> {
    // SAFETY: AXUIElementCreateApplication creates a +1 retained AXUIElementRef.
    let app_element = unsafe { AXUIElementCreateApplication(pid) };
    if app_element.is_null() {
        return Err(format!("Failed to create AX element for PID {}", pid));
    }

    // SAFETY: app_element is a valid AXUIElementRef we just created.
    unsafe {
        AXUIElementSetMessagingTimeout(app_element, AX_MESSAGING_TIMEOUT);
    }

    let result = ax_find_and_act_on_window(app_element, title, WindowAction::Minimize);

    // SAFETY: Release the app element (Create Rule — we own it).
    unsafe {
        core_foundation::base::CFRelease(app_element as *mut c_void);
    }

    result
}

enum WindowAction {
    Raise,
    Minimize,
}

/// Find a window by title in the app's AX windows and perform an action on it.
fn ax_find_and_act_on_window(
    app_element: AXUIElementRef,
    title: &str,
    action: WindowAction,
) -> Result<(), String> {
    let cf_windows_attr = CFString::new(kAXWindowsAttribute);
    let mut windows_value: core_foundation::base::CFTypeRef = ptr::null();

    // SAFETY: Standard AXUIElementCopyAttributeValue call (Copy Rule: +1 retained ref).
    let result = unsafe {
        AXUIElementCopyAttributeValue(
            app_element,
            cf_windows_attr.as_concrete_TypeRef(),
            &mut windows_value,
        )
    };

    if result != kAXErrorSuccess || windows_value.is_null() {
        return Err(format!(
            "Failed to get windows attribute (AXError: {})",
            result
        ));
    }

    // SAFETY: windows_value is a +1 retained CFArrayRef from CopyAttributeValue.
    // wrap_under_create_rule takes ownership — it will CFRelease when dropped.
    let cf_array: CFArray<CFType> = unsafe {
        CFArray::wrap_under_create_rule(windows_value as core_foundation::array::CFArrayRef)
    };

    let title_lower = title.to_lowercase();

    for i in 0..cf_array.len() {
        // SAFETY: Accessing array elements — these are unretained borrows into the CFArray.
        let Some(item) = cf_array.get(i) else {
            continue;
        };
        let window_element = item.as_CFTypeRef() as AXUIElementRef;

        if let Some(window_title) = ax_get_string_attribute(window_element, kAXTitleAttribute)
            && window_title.to_lowercase().contains(&title_lower)
        {
            return match action {
                WindowAction::Raise => ax_perform_raise(window_element),
                WindowAction::Minimize => ax_set_minimized(window_element, true),
            };
        }
    }

    Err(format!("No AX window found matching title '{}'", title))
}

/// Perform AXRaise action on a window element.
fn ax_perform_raise(window_element: AXUIElementRef) -> Result<(), String> {
    // Use kAXRaisedAttribute = true to raise the window
    let cf_attr = CFString::new("AXRaised");
    let cf_true = CFBoolean::true_value();

    // SAFETY: Setting attribute value on a valid window element.
    let result = unsafe {
        AXUIElementSetAttributeValue(
            window_element,
            cf_attr.as_concrete_TypeRef(),
            cf_true.as_CFTypeRef(),
        )
    };

    if result != kAXErrorSuccess {
        // Try AXMain as fallback (some apps respond to this instead)
        let cf_main = CFString::new("AXMain");
        let result2 = unsafe {
            AXUIElementSetAttributeValue(
                window_element,
                cf_main.as_concrete_TypeRef(),
                cf_true.as_CFTypeRef(),
            )
        };

        if result2 != kAXErrorSuccess {
            return Err(format!(
                "Failed to raise window (AXRaised error: {}, AXMain error: {})",
                result, result2
            ));
        }
    }

    Ok(())
}

/// Set the minimized attribute on a window element.
fn ax_set_minimized(window_element: AXUIElementRef, minimized: bool) -> Result<(), String> {
    let cf_attr = CFString::new(kAXMinimizedAttribute);
    let cf_value = if minimized {
        CFBoolean::true_value()
    } else {
        CFBoolean::false_value()
    };

    // SAFETY: Setting attribute value on a valid window element.
    let result = unsafe {
        AXUIElementSetAttributeValue(
            window_element,
            cf_attr.as_concrete_TypeRef(),
            cf_value.as_CFTypeRef(),
        )
    };

    if result != kAXErrorSuccess {
        return Err(format!(
            "Failed to set minimized to {} (AXError: {})",
            minimized, result
        ));
    }

    Ok(())
}

/// Get a string attribute from an AX element.
fn ax_get_string_attribute(element: AXUIElementRef, attribute: &str) -> Option<String> {
    let cf_attr = CFString::new(attribute);
    let mut value: core_foundation::base::CFTypeRef = ptr::null();

    // SAFETY: Standard AXUIElementCopyAttributeValue (Copy Rule: +1 retained on success).
    let result = unsafe {
        AXUIElementCopyAttributeValue(element, cf_attr.as_concrete_TypeRef(), &mut value)
    };

    if result != kAXErrorSuccess as AXError || value.is_null() {
        return None;
    }

    // SAFETY: value is a +1 retained CFTypeRef. wrap_under_create_rule takes ownership.
    let cf_type: CFType = unsafe { TCFType::wrap_under_create_rule(value) };

    if cf_type.instance_of::<CFString>() {
        let ptr = cf_type.as_CFTypeRef() as *const _;
        let s = unsafe { CFString::wrap_under_get_rule(ptr) }.to_string();
        Some(s)
    } else {
        None
    }
}

/// Activate an application by name via AppleScript.
fn activate_app(app_name: &str) -> Result<(), TerminalError> {
    let script = format!(r#"tell application "{}" to activate"#, app_name);

    match std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
    {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            warn!(
                event = "core.terminal.native.activate_app_failed",
                app_name = app_name,
                stderr = %stderr
            );
            Err(TerminalError::FocusFailed {
                message: format!("Failed to activate {}: {}", app_name, stderr),
            })
        }
        Err(e) => {
            warn!(
                event = "core.terminal.native.activate_app_error",
                app_name = app_name,
                error = %e
            );
            Err(TerminalError::FocusFailed {
                message: format!("Failed to run osascript for {}: {}", app_name, e),
            })
        }
    }
}

/// Hide an application via System Events (hides all windows).
fn hide_app_via_system_events(app_name: &str) -> Result<(), TerminalError> {
    let script = format!(
        r#"tell application "System Events" to set visible of process "{}" to false"#,
        app_name
    );

    match std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
    {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(TerminalError::HideFailed {
                message: format!("Failed to hide {} via System Events: {}", app_name, stderr),
            })
        }
        Err(e) => Err(TerminalError::HideFailed {
            message: format!("Failed to run osascript for {}: {}", app_name, e),
        }),
    }
}
