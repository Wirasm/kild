use super::*;

#[test]
fn test_to_screen_coordinates() {
    let window = WindowInfo::new(
        1,
        "Test".to_string(),
        "TestApp".to_string(),
        100,
        200,
        800,
        600,
        false,
        None,
    );
    let (sx, sy) = to_screen_coordinates(50, 30, &window);
    assert!((sx - 150.0).abs() < f64::EPSILON);
    assert!((sy - 230.0).abs() < f64::EPSILON);
}

#[test]
fn test_to_screen_coordinates_origin() {
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
    let (sx, sy) = to_screen_coordinates(0, 0, &window);
    assert!((sx - 0.0).abs() < f64::EPSILON);
    assert!((sy - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_validate_coordinates_valid() {
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
    assert!(validate_coordinates(0, 0, &window).is_ok());
    assert!(validate_coordinates(799, 599, &window).is_ok());
    assert!(validate_coordinates(400, 300, &window).is_ok());
}

#[test]
fn test_validate_coordinates_out_of_bounds() {
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
    assert!(validate_coordinates(800, 0, &window).is_err());
    assert!(validate_coordinates(0, 600, &window).is_err());
    assert!(validate_coordinates(999, 999, &window).is_err());
}

#[test]
fn test_validate_coordinates_negative() {
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
    assert!(validate_coordinates(-1, 0, &window).is_err());
    assert!(validate_coordinates(0, -1, &window).is_err());
}

#[test]
fn test_map_window_error_not_found() {
    let err = map_window_error(WindowError::WindowNotFound {
        title: "Test".to_string(),
    });
    match err {
        InteractionError::WindowNotFound { title } => assert_eq!(title, "Test"),
        _ => panic!("Expected WindowNotFound"),
    }
}

#[test]
fn test_map_window_error_not_found_by_app() {
    let err = map_window_error(WindowError::WindowNotFoundByApp {
        app: "TestApp".to_string(),
    });
    match err {
        InteractionError::WindowNotFoundByApp { app } => assert_eq!(app, "TestApp"),
        _ => panic!("Expected WindowNotFoundByApp"),
    }
}

#[test]
fn test_map_window_error_other() {
    let err = map_window_error(WindowError::EnumerationFailed {
        message: "test error".to_string(),
    });
    match err {
        InteractionError::WindowLookupFailed { reason } => {
            assert!(reason.contains("test error"));
        }
        _ => panic!("Expected WindowLookupFailed"),
    }
}

#[test]
fn test_map_window_error_wait_timeout_by_title() {
    let err = map_window_error(WindowError::WaitTimeoutByTitle {
        title: "Test".to_string(),
        timeout_ms: 5000,
    });
    match err {
        InteractionError::WaitTimeoutByTitle { title, timeout_ms } => {
            assert_eq!(title, "Test");
            assert_eq!(timeout_ms, 5000);
        }
        _ => panic!("Expected WaitTimeoutByTitle"),
    }
}

#[test]
fn test_map_window_error_wait_timeout_by_app() {
    let err = map_window_error(WindowError::WaitTimeoutByApp {
        app: "Ghostty".to_string(),
        timeout_ms: 3000,
    });
    match err {
        InteractionError::WaitTimeoutByApp { app, timeout_ms } => {
            assert_eq!(app, "Ghostty");
            assert_eq!(timeout_ms, 3000);
        }
        _ => panic!("Expected WaitTimeoutByApp"),
    }
}

#[test]
fn test_map_window_error_wait_timeout_by_app_and_title() {
    let err = map_window_error(WindowError::WaitTimeoutByAppAndTitle {
        app: "Ghostty".to_string(),
        title: "Terminal".to_string(),
        timeout_ms: 10000,
    });
    match err {
        InteractionError::WaitTimeoutByAppAndTitle {
            app,
            title,
            timeout_ms,
        } => {
            assert_eq!(app, "Ghostty");
            assert_eq!(title, "Terminal");
            assert_eq!(timeout_ms, 10000);
        }
        _ => panic!("Expected WaitTimeoutByAppAndTitle"),
    }
}

// Integration tests requiring accessibility permissions
#[test]
#[ignore]
fn test_click_text_integration() {
    // This test requires accessibility permission and a running app
    // Run manually: cargo test --all -- --ignored test_click_text_integration
    // WARNING: This will actually click on a UI element!

    let request = ClickTextRequest::new(
        InteractionTarget::App {
            app: "Finder".to_string(),
        },
        "File",
    );
    let result = click_text(&request);
    match result {
        Ok(result) => {
            assert!(result.success);
        }
        Err(InteractionError::AccessibilityPermissionDenied) => {
            // Expected if running without accessibility permission
        }
        Err(InteractionError::ElementNotFound { .. }) => {
            // "File" menu might not exist if Finder isn't focused
        }
        Err(e) => {
            panic!("Unexpected error: {:?}", e);
        }
    }
}
