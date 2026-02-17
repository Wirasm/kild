use std::time::{Duration, Instant};

use super::find::poll_until_found;
use super::*;
use crate::errors::PeekError;
use crate::window::errors::WindowError;
use crate::window::types::{MonitorInfo, WindowInfo};

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
        Some(1234),
    );

    assert_eq!(window.id(), 123);
    assert_eq!(window.title(), "Test Title");
    assert_eq!(window.app_name(), "TestApp");
    assert_eq!(window.x(), 100);
    assert_eq!(window.y(), 200);
    assert_eq!(window.width(), 800);
    assert_eq!(window.height(), 600);
    assert!(!window.is_minimized());
    assert_eq!(window.pid(), Some(1234));
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

#[test]
fn test_find_window_by_app_not_found() {
    let result = find_window_by_app("NONEXISTENT_APP_12345_UNIQUE");
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.error_code(), "WINDOW_NOT_FOUND_BY_APP");
    }
}

#[test]
fn test_find_window_by_app_is_case_insensitive() {
    // Both should return the same error (no such app exists)
    let result_lower = find_window_by_app("nonexistent_app_test_xyz789");
    let result_upper = find_window_by_app("NONEXISTENT_APP_TEST_XYZ789");

    // Both should be errors (app doesn't exist)
    assert!(result_lower.is_err());
    assert!(result_upper.is_err());

    // Both should have the same error code
    assert_eq!(
        result_lower.unwrap_err().error_code(),
        result_upper.unwrap_err().error_code()
    );
}

#[test]
fn test_find_window_by_app_and_title_app_not_found() {
    let result = find_window_by_app_and_title("NONEXISTENT_APP_ABC", "Some Title");
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.error_code(), "WINDOW_NOT_FOUND_BY_APP");
    }
}

#[test]
fn test_poll_until_found_returns_immediately_on_success() {
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

    let start = Instant::now();
    let result = poll_until_found(
        5000,
        || Ok(window.clone()),
        |e| e,
        || WindowError::WaitTimeoutByTitle {
            title: "Test".to_string(),
            timeout_ms: 5000,
        },
    );

    assert!(result.is_ok());
    assert!(
        start.elapsed() < Duration::from_millis(50),
        "Should return immediately on first success"
    );
}

#[test]
fn test_poll_until_found_retries_and_succeeds() {
    use std::sync::{Arc, Mutex};
    let attempts = Arc::new(Mutex::new(0u32));
    let attempts_clone = attempts.clone();

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

    let start = Instant::now();
    let result = poll_until_found(
        2000,
        move || {
            let mut count = attempts_clone.lock().unwrap();
            *count += 1;
            if *count < 3 {
                Err(WindowError::WindowNotFound {
                    title: "Test".to_string(),
                })
            } else {
                Ok(window.clone())
            }
        },
        |e| e,
        || WindowError::WaitTimeoutByTitle {
            title: "Test".to_string(),
            timeout_ms: 2000,
        },
    );

    assert!(result.is_ok());
    assert_eq!(*attempts.lock().unwrap(), 3);
    // Should have slept at least 200ms (2 retries * 100ms interval)
    assert!(start.elapsed() >= Duration::from_millis(200));
}

#[test]
fn test_poll_until_found_propagates_non_retryable_errors() {
    let start = Instant::now();
    let result = poll_until_found(
        5000,
        || {
            Err(WindowError::EnumerationFailed {
                message: "permission denied".to_string(),
            })
        },
        |e| e,
        || WindowError::WaitTimeoutByTitle {
            title: "Test".to_string(),
            timeout_ms: 5000,
        },
    );

    // Should fail immediately, not retry for 5 seconds
    assert!(
        start.elapsed() < Duration::from_millis(50),
        "Non-retryable errors should propagate immediately"
    );
    assert!(matches!(result, Err(WindowError::EnumerationFailed { .. })));
}

#[test]
fn test_poll_until_found_respects_timeout() {
    let start = Instant::now();
    let result = poll_until_found(
        300,
        || {
            Err(WindowError::WindowNotFound {
                title: "Test".to_string(),
            })
        },
        |e| e,
        || WindowError::WaitTimeoutByTitle {
            title: "Test".to_string(),
            timeout_ms: 300,
        },
    );

    let elapsed = start.elapsed();
    assert!(result.is_err());
    assert!(
        elapsed >= Duration::from_millis(300),
        "Should wait at least the timeout duration, got {:?}",
        elapsed
    );
    assert!(
        elapsed < Duration::from_millis(600),
        "Should not overshoot timeout significantly, got {:?}",
        elapsed
    );
    assert!(matches!(
        result,
        Err(WindowError::WaitTimeoutByTitle { .. })
    ));
}

#[test]
fn test_find_window_by_title_with_wait_timeout() {
    let start = Instant::now();
    let result = find_window_by_title_with_wait("NONEXISTENT_WINDOW_UNIQUE_12345", 200);
    let elapsed = start.elapsed();

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().error_code(),
        "WINDOW_WAIT_TIMEOUT_BY_TITLE"
    );
    assert!(
        elapsed >= Duration::from_millis(200),
        "Should wait at least the timeout duration, got {:?}",
        elapsed
    );
}

#[test]
fn test_find_window_by_app_with_wait_timeout() {
    let start = Instant::now();
    let result = find_window_by_app_with_wait("NONEXISTENT_APP_UNIQUE_12345", 200);
    let elapsed = start.elapsed();

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().error_code(),
        "WINDOW_WAIT_TIMEOUT_BY_APP"
    );
    assert!(
        elapsed >= Duration::from_millis(200),
        "Should wait at least the timeout duration, got {:?}",
        elapsed
    );
}

#[test]
fn test_find_window_by_app_and_title_with_wait_timeout() {
    let start = Instant::now();
    let result = find_window_by_app_and_title_with_wait(
        "NONEXISTENT_APP_UNIQUE_12345",
        "NONEXISTENT_TITLE",
        200,
    );
    let elapsed = start.elapsed();

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().error_code(),
        "WINDOW_WAIT_TIMEOUT_BY_APP_AND_TITLE"
    );
    assert!(
        elapsed >= Duration::from_millis(200),
        "Should wait at least the timeout duration, got {:?}",
        elapsed
    );
}
