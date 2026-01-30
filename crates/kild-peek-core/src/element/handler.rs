use tracing::{info, warn};

use super::accessibility;
use super::errors::ElementError;
use super::types::{ElementInfo, ElementsRequest, ElementsResult, FindRequest};
use crate::interact::InteractionTarget;
use crate::window::{
    WindowError, WindowInfo, find_window_by_app, find_window_by_app_and_title, find_window_by_title,
};

// SAFETY: FFI declaration for AXIsProcessTrusted from macOS ApplicationServices framework.
// Returns false when the process lacks accessibility permissions (does not crash).
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

/// Check if the current process has accessibility permissions
fn check_accessibility_permission() -> Result<(), ElementError> {
    let trusted = unsafe { AXIsProcessTrusted() };
    if !trusted {
        return Err(ElementError::AccessibilityPermissionDenied);
    }
    Ok(())
}

/// Find a window by interaction target
fn find_window_by_target(target: &InteractionTarget) -> Result<WindowInfo, ElementError> {
    let result = match target {
        InteractionTarget::Window { title } => find_window_by_title(title),
        InteractionTarget::App { app } => find_window_by_app(app),
        InteractionTarget::AppAndWindow { app, title } => find_window_by_app_and_title(app, title),
    };
    result.map_err(map_window_error)
}

/// Map WindowError to ElementError
fn map_window_error(error: WindowError) -> ElementError {
    use WindowError::*;

    match error {
        WindowNotFound { title } => ElementError::WindowNotFound { title },
        WindowNotFoundByApp { app } => ElementError::WindowNotFoundByApp { app },
        other => {
            warn!(
                event = "peek.core.element.window_error_unmapped",
                error = %other
            );
            ElementError::WindowLookupFailed {
                reason: other.to_string(),
            }
        }
    }
}

/// List all UI elements in a window
pub fn list_elements(request: &ElementsRequest) -> Result<ElementsResult, ElementError> {
    info!(
        event = "peek.core.element.list_started",
        target = ?request.target()
    );

    check_accessibility_permission()?;

    let window = find_window_by_target(request.target())?;

    if window.is_minimized() {
        return Err(ElementError::WindowMinimized {
            title: window.title().to_string(),
        });
    }

    let pid = window.pid().ok_or(ElementError::NoPidAvailable)?;

    let raw_elements = accessibility::query_elements(pid)
        .map_err(|reason| ElementError::AccessibilityQueryFailed { reason })?;

    // Convert RawElement → ElementInfo (screen-absolute → window-relative coordinates)
    let elements: Vec<ElementInfo> = raw_elements
        .iter()
        .map(|raw| convert_raw_to_element_info(raw, &window))
        .collect();

    info!(
        event = "peek.core.element.list_completed",
        count = elements.len(),
        window = window.title()
    );

    Ok(ElementsResult::new(elements, window.title().to_string()))
}

/// Find a specific element by text content
pub fn find_element(request: &FindRequest) -> Result<ElementInfo, ElementError> {
    info!(
        event = "peek.core.element.find_started",
        text = request.text(),
        target = ?request.target()
    );

    if request.text().is_empty() {
        return Err(ElementError::ElementNotFound {
            text: String::new(),
        });
    }

    // List all elements then filter
    let elements_request = ElementsRequest::new(request.target().clone());
    let result = list_elements(&elements_request)?;

    let matches: Vec<&ElementInfo> = result
        .elements()
        .iter()
        .filter(|e| e.matches_text(request.text()))
        .collect();

    match matches.len() {
        0 => {
            info!(
                event = "peek.core.element.find_not_found",
                text = request.text()
            );
            Err(ElementError::ElementNotFound {
                text: request.text().to_string(),
            })
        }
        1 => {
            info!(
                event = "peek.core.element.find_completed",
                text = request.text(),
                role = matches[0].role()
            );
            Ok(matches[0].clone())
        }
        count => {
            warn!(
                event = "peek.core.element.find_ambiguous",
                text = request.text(),
                count = count
            );
            Err(ElementError::ElementAmbiguous {
                text: request.text().to_string(),
                count,
            })
        }
    }
}

/// Convert a RawElement to ElementInfo, adjusting coordinates from screen-absolute
/// to window-relative.
///
/// Subtracts window position (window.x, window.y) from element's screen coordinates
/// to produce coordinates relative to the window's top-left corner.
pub(crate) fn convert_raw_to_element_info(
    raw: &accessibility::RawElement,
    window: &WindowInfo,
) -> ElementInfo {
    let (x, y) = match raw.position() {
        Some((abs_x, abs_y)) => {
            let rel_x = abs_x as i32 - window.x();
            let rel_y = abs_y as i32 - window.y();
            (rel_x, rel_y)
        }
        None => (0, 0),
    };

    let (width, height) = match raw.size() {
        Some((w, h)) => (w as u32, h as u32),
        None => (0, 0),
    };

    ElementInfo::new(
        raw.role().to_string(),
        raw.title().map(String::from),
        raw.value().map(String::from),
        raw.description().map(String::from),
        x,
        y,
        width,
        height,
        raw.enabled(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::accessibility::RawElement;

    #[test]
    fn test_convert_raw_to_element_info_with_position() {
        let window = WindowInfo::new(
            1,
            "Test".to_string(),
            "TestApp".to_string(),
            100,
            200,
            800,
            600,
            false,
            Some(1234),
        );

        let raw = RawElement::new(
            "AXButton".to_string(),
            Some("OK".to_string()),
            None,
            None,
            Some((250.0, 350.0)),
            Some((80.0, 30.0)),
            true,
        );

        let elem = convert_raw_to_element_info(&raw, &window);
        assert_eq!(elem.role(), "AXButton");
        assert_eq!(elem.title(), Some("OK"));
        // 250 - 100 = 150 (screen x - window x)
        assert_eq!(elem.x(), 150);
        // 350 - 200 = 150 (screen y - window y)
        assert_eq!(elem.y(), 150);
        assert_eq!(elem.width(), 80);
        assert_eq!(elem.height(), 30);
        assert!(elem.enabled());
    }

    #[test]
    fn test_convert_raw_to_element_info_no_position() {
        let window = WindowInfo::new(
            1,
            "Test".to_string(),
            "TestApp".to_string(),
            0,
            0,
            800,
            600,
            false,
            None,
        );

        let raw = RawElement::new("AXGroup".to_string(), None, None, None, None, None, true);

        let elem = convert_raw_to_element_info(&raw, &window);
        assert_eq!(elem.x(), 0);
        assert_eq!(elem.y(), 0);
        assert_eq!(elem.width(), 0);
        assert_eq!(elem.height(), 0);
    }

    #[test]
    fn test_map_window_error_not_found() {
        let err = map_window_error(WindowError::WindowNotFound {
            title: "Test".to_string(),
        });
        match err {
            ElementError::WindowNotFound { title } => assert_eq!(title, "Test"),
            _ => panic!("Expected WindowNotFound"),
        }
    }

    #[test]
    fn test_map_window_error_not_found_by_app() {
        let err = map_window_error(WindowError::WindowNotFoundByApp {
            app: "TestApp".to_string(),
        });
        match err {
            ElementError::WindowNotFoundByApp { app } => assert_eq!(app, "TestApp"),
            _ => panic!("Expected WindowNotFoundByApp"),
        }
    }

    #[test]
    fn test_map_window_error_other() {
        let err = map_window_error(WindowError::EnumerationFailed {
            message: "test error".to_string(),
        });
        match err {
            ElementError::WindowLookupFailed { reason } => {
                assert!(reason.contains("test error"));
            }
            _ => panic!("Expected WindowLookupFailed"),
        }
    }

    #[test]
    fn test_convert_raw_to_element_info_negative_window_coords() {
        // Window positioned off-screen (negative coordinates)
        let window = WindowInfo::new(
            1,
            "OffScreen".to_string(),
            "TestApp".to_string(),
            -100,
            -50,
            800,
            600,
            false,
            Some(1234),
        );

        let raw = RawElement::new(
            "AXButton".to_string(),
            Some("OK".to_string()),
            None,
            None,
            Some((50.0, 100.0)),
            Some((80.0, 30.0)),
            true,
        );

        let elem = convert_raw_to_element_info(&raw, &window);
        // 50 - (-100) = 150 (screen x - window x)
        assert_eq!(elem.x(), 150);
        // 100 - (-50) = 150 (screen y - window y)
        assert_eq!(elem.y(), 150);
    }

    #[test]
    fn test_convert_raw_to_element_info_partial_zero_size() {
        // Horizontal divider: width > 0, height = 0
        let window = WindowInfo::new(
            1,
            "Test".to_string(),
            "TestApp".to_string(),
            0,
            0,
            800,
            600,
            false,
            Some(1234),
        );

        let raw = RawElement::new(
            "AXSplitter".to_string(),
            None,
            None,
            None,
            Some((100.0, 200.0)),
            Some((500.0, 0.0)),
            true,
        );

        let elem = convert_raw_to_element_info(&raw, &window);
        assert_eq!(elem.width(), 500);
        assert_eq!(elem.height(), 0);
        // Element is still valid, just has zero height
    }

    // Integration tests requiring accessibility permissions
    #[test]
    #[ignore]
    fn test_list_elements_integration() {
        // This test requires accessibility permission and a running app
        // Run manually: cargo test --all -- --ignored test_list_elements_integration
        use crate::interact::InteractionTarget;

        let request = ElementsRequest::new(InteractionTarget::App {
            app: "Finder".to_string(),
        });
        let result = list_elements(&request);
        // Just verify it returns something or a meaningful error
        match result {
            Ok(elements) => {
                assert!(elements.count() > 0, "Expected at least one element");
            }
            Err(ElementError::AccessibilityPermissionDenied) => {
                // Expected if running without accessibility permission
            }
            Err(e) => {
                panic!("Unexpected error: {:?}", e);
            }
        }
    }

    #[test]
    #[ignore]
    fn test_find_element_integration() {
        // This test requires accessibility permission and Finder running
        // Run manually: cargo test --all -- --ignored test_find_element_integration
        use crate::interact::InteractionTarget;

        let request = FindRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            "File",
        );
        let result = find_element(&request);
        match result {
            Ok(elem) => {
                assert!(!elem.role().is_empty());
            }
            Err(ElementError::AccessibilityPermissionDenied) => {
                // Expected if running without accessibility permission
            }
            Err(ElementError::ElementNotFound { .. }) => {
                // "File" menu might not exist if Finder isn't focused
            }
            Err(e) => {
                panic!("Unexpected error: {:?}", e);
            }
        }
    }
}
