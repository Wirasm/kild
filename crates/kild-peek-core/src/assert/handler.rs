use tracing::{info, warn};

use super::errors::AssertError;
use super::types::{Assertion, AssertionResult};
use crate::diff::{DiffRequest, compare_images};
use crate::window::{find_window_by_title, list_windows};

/// Run an assertion and return the result
pub fn run_assertion(assertion: &Assertion) -> Result<AssertionResult, AssertError> {
    info!(event = "core.assert.run_started", assertion = ?assertion);

    let result = match assertion {
        Assertion::WindowExists { title } => assert_window_exists(title),
        Assertion::WindowVisible { title } => assert_window_visible(title),
        Assertion::ElementExists {
            window_title: _,
            query: _,
        } => {
            // Element inspection requires accessibility APIs which we're deferring
            // For now, return a meaningful error
            Ok(AssertionResult::fail(
                "Element assertions require accessibility APIs (not yet implemented)",
            ))
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
            window.title, window.id, window.width, window.height
        ))
        .with_details(serde_json::json!({
            "window_id": window.id,
            "window_title": window.title,
            "width": window.width,
            "height": window.height,
        }))),
        Err(_) => {
            // List available windows for debugging
            let available = list_windows()
                .map(|windows| windows.iter().map(|w| w.title.clone()).collect::<Vec<_>>())
                .unwrap_or_default();

            Ok(
                AssertionResult::fail(format!("Window '{}' not found", title)).with_details(
                    serde_json::json!({
                        "searched_title": title,
                        "available_windows": available.into_iter().take(10).collect::<Vec<_>>(),
                    }),
                ),
            )
        }
    }
}

fn assert_window_visible(title: &str) -> Result<AssertionResult, AssertError> {
    match find_window_by_title(title) {
        Ok(window) => {
            if window.is_minimized {
                Ok(
                    AssertionResult::fail(format!("Window '{}' exists but is minimized", title))
                        .with_details(serde_json::json!({
                            "window_id": window.id,
                            "window_title": window.title,
                            "is_minimized": true,
                        })),
                )
            } else {
                Ok(AssertionResult::pass(format!(
                    "Window '{}' is visible (id: {}, {}x{})",
                    window.title, window.id, window.width, window.height
                ))
                .with_details(serde_json::json!({
                    "window_id": window.id,
                    "window_title": window.title,
                    "width": window.width,
                    "height": window.height,
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

fn assert_image_similar(
    image_path: &std::path::Path,
    baseline_path: &std::path::Path,
    threshold: f64,
) -> Result<AssertionResult, AssertError> {
    let request = DiffRequest::new(image_path, baseline_path).with_threshold(threshold);

    match compare_images(&request) {
        Ok(diff_result) => {
            if diff_result.is_similar {
                Ok(AssertionResult::pass(format!(
                    "Images are similar ({}% similarity, threshold: {}%)",
                    (diff_result.similarity * 100.0).round(),
                    (threshold * 100.0).round()
                ))
                .with_details(serde_json::json!({
                    "similarity": diff_result.similarity,
                    "threshold": threshold,
                    "is_similar": true,
                })))
            } else {
                Ok(AssertionResult::fail(format!(
                    "Images are not similar enough ({}% similarity, threshold: {}%)",
                    (diff_result.similarity * 100.0).round(),
                    (threshold * 100.0).round()
                ))
                .with_details(serde_json::json!({
                    "similarity": diff_result.similarity,
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
}
