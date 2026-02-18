use tracing::{info, warn};

use super::errors::AssertError;
use super::types::{Assertion, AssertionResult};
use crate::diff::{DiffRequest, compare_images};
use crate::element::{ElementsRequest, list_elements};
use crate::errors::PeekError;
use crate::interact::InteractionTarget;
use crate::window::{find_window_by_title, list_windows};

/// Run an assertion and return the result
pub fn run_assertion(assertion: &Assertion) -> Result<AssertionResult, AssertError> {
    info!(event = "core.assert.run_started", assertion = ?assertion);

    let result = match assertion {
        Assertion::WindowExists { title } => assert_window_exists(title),
        Assertion::WindowVisible { title } => assert_window_visible(title),
        Assertion::ElementExists { window_title, text } => {
            assert_element_exists(window_title, text)
        }
        Assertion::ImageSimilar {
            image_path,
            baseline_path,
            threshold,
        } => assert_image_similar(image_path, baseline_path, *threshold),
    }?;

    if result.passed {
        info!(
            event = "core.assert.run_passed",
            message = %result.message
        );
    } else {
        warn!(
            event = "core.assert.run_failed",
            message = %result.message
        );
    }

    Ok(result)
}

fn assert_window_exists(title: &str) -> Result<AssertionResult, AssertError> {
    match find_window_by_title(title) {
        Ok(window) => Ok(AssertionResult::pass(format!(
            "Window '{}' exists (id: {}, {}x{})",
            window.title(),
            window.id(),
            window.width(),
            window.height()
        ))
        .with_details(serde_json::json!({
            "window_id": window.id(),
            "window_title": window.title(),
            "width": window.width(),
            "height": window.height(),
        }))),
        Err(_) => {
            // List available windows for debugging
            let available = match list_windows() {
                Ok(windows) => {
                    let titles: Vec<_> = windows
                        .iter()
                        .map(|w| w.title().to_string())
                        .take(10)
                        .collect();
                    serde_json::json!(titles)
                }
                Err(e) => {
                    warn!(
                        event = "core.assert.list_windows_failed_during_error_reporting",
                        error = %e
                    );
                    serde_json::json!("enumeration_failed")
                }
            };

            Ok(
                AssertionResult::fail(format!("Window '{}' not found", title)).with_details(
                    serde_json::json!({
                        "searched_title": title,
                        "available_windows": available,
                    }),
                ),
            )
        }
    }
}

fn assert_window_visible(title: &str) -> Result<AssertionResult, AssertError> {
    match find_window_by_title(title) {
        Ok(window) => {
            if window.is_minimized() {
                Ok(
                    AssertionResult::fail(format!("Window '{}' exists but is minimized", title))
                        .with_details(serde_json::json!({
                            "window_id": window.id(),
                            "window_title": window.title(),
                            "is_minimized": true,
                        })),
                )
            } else {
                Ok(AssertionResult::pass(format!(
                    "Window '{}' is visible (id: {}, {}x{})",
                    window.title(),
                    window.id(),
                    window.width(),
                    window.height()
                ))
                .with_details(serde_json::json!({
                    "window_id": window.id(),
                    "window_title": window.title(),
                    "width": window.width(),
                    "height": window.height(),
                    "is_minimized": false,
                })))
            }
        }
        Err(_) => Ok(
            AssertionResult::fail(format!("Window '{}' not found", title)).with_details(
                serde_json::json!({
                    "searched_title": title,
                }),
            ),
        ),
    }
}

fn assert_element_exists(window_title: &str, text: &str) -> Result<AssertionResult, AssertError> {
    let request = ElementsRequest::new(InteractionTarget::Window {
        title: window_title.to_string(),
    });

    let result = match list_elements(&request) {
        Ok(r) => r,
        Err(e) => {
            warn!(
                event = "peek.core.assert.list_elements_failed",
                window = window_title,
                error_code = e.error_code(),
                error = %e
            );
            return Ok(AssertionResult::fail(format!(
                "Could not list elements in window '{}': {}",
                window_title, e
            ))
            .with_details(serde_json::json!({
                "window": window_title,
                "error": e.to_string(),
            })));
        }
    };

    let search_text = text;
    let element_count = result.count();

    let found = if search_text.is_empty() {
        !result.elements().is_empty()
    } else {
        result
            .elements()
            .iter()
            .any(|e| e.matches_text(search_text))
    };

    if found {
        Ok(AssertionResult::pass(format!(
            "Element with text '{}' found in window '{}'",
            search_text, window_title
        ))
        .with_details(serde_json::json!({
            "window": window_title,
            "text": search_text,
            "element_count": element_count,
        })))
    } else {
        Ok(AssertionResult::fail(format!(
            "No element with text '{}' found in window '{}' ({} elements checked)",
            search_text, window_title, element_count,
        ))
        .with_details(serde_json::json!({
            "window": window_title,
            "text": search_text,
            "element_count": element_count,
        })))
    }
}

fn assert_image_similar(
    image_path: &std::path::Path,
    baseline_path: &std::path::Path,
    threshold: f64,
) -> Result<AssertionResult, AssertError> {
    let request = DiffRequest::new(image_path, baseline_path).with_threshold(threshold);

    match compare_images(&request) {
        Ok(diff_result) => {
            if diff_result.is_similar() {
                Ok(AssertionResult::pass(format!(
                    "Images are similar ({}% similarity, threshold: {}%)",
                    (diff_result.similarity() * 100.0).round(),
                    (threshold * 100.0).round()
                ))
                .with_details(serde_json::json!({
                    "similarity": diff_result.similarity(),
                    "threshold": threshold,
                    "is_similar": true,
                })))
            } else {
                Ok(AssertionResult::fail(format!(
                    "Images are not similar enough ({}% similarity, threshold: {}%)",
                    (diff_result.similarity() * 100.0).round(),
                    (threshold * 100.0).round()
                ))
                .with_details(serde_json::json!({
                    "similarity": diff_result.similarity(),
                    "threshold": threshold,
                    "is_similar": false,
                })))
            }
        }
        Err(e) => Err(AssertError::ImageComparisonFailed(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assert_window_exists_not_found() {
        let assertion = Assertion::window_exists("NONEXISTENT_WINDOW_12345_UNIQUE");
        let result = run_assertion(&assertion).unwrap();
        assert!(!result.passed);
        assert!(result.message.contains("not found"));
    }

    #[test]
    fn test_assert_window_visible_not_found() {
        let assertion = Assertion::window_visible("NONEXISTENT_WINDOW_12345_UNIQUE");
        let result = run_assertion(&assertion).unwrap();
        assert!(!result.passed);
        assert!(result.message.contains("not found"));
    }

    #[test]
    fn test_assert_image_similar_missing_files() {
        let assertion =
            Assertion::image_similar("/nonexistent/image.png", "/nonexistent/baseline.png", 0.95);
        let result = run_assertion(&assertion);
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_element_exists_not_found() {
        let assertion = Assertion::element_exists(
            "NONEXISTENT_WINDOW_12345_UNIQUE",
            "DEFINITELY_NOT_THERE_XYZ",
        );
        let result = run_assertion(&assertion).unwrap();
        // Either window-not-found fail or element-not-found fail — both are failures
        assert!(!result.passed);
    }

    #[test]
    fn test_assert_element_exists_empty_text_nonexistent_window() {
        let assertion = Assertion::element_exists("NONEXISTENT_WINDOW_12345_UNIQUE", "");
        let result = run_assertion(&assertion).unwrap();
        assert!(!result.passed);
    }
}
