use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Types of assertions that can be performed
#[derive(Debug, Clone)]
pub enum Assertion {
    /// Assert that a window with the given title exists
    WindowExists { title: String },
    /// Assert that a window with the given title is visible (not minimized)
    WindowVisible { title: String },
    /// Assert that a UI element with the given text exists in a window
    ElementExists { window_title: String, text: String },
    /// Assert that a screenshot is similar to a baseline image
    ImageSimilar {
        image_path: PathBuf,
        baseline_path: PathBuf,
        threshold: f64,
    },
}

impl Assertion {
    /// Create a window exists assertion
    pub fn window_exists(title: impl Into<String>) -> Self {
        Assertion::WindowExists {
            title: title.into(),
        }
    }

    /// Create a window visible assertion
    pub fn window_visible(title: impl Into<String>) -> Self {
        Assertion::WindowVisible {
            title: title.into(),
        }
    }

    /// Create an element exists assertion
    pub fn element_exists(window_title: impl Into<String>, text: impl Into<String>) -> Self {
        Assertion::ElementExists {
            window_title: window_title.into(),
            text: text.into(),
        }
    }

    /// Create an image similarity assertion
    pub fn image_similar(
        image: impl Into<PathBuf>,
        baseline: impl Into<PathBuf>,
        threshold: f64,
    ) -> Self {
        Assertion::ImageSimilar {
            image_path: image.into(),
            baseline_path: baseline.into(),
            threshold: threshold.clamp(0.0, 1.0),
        }
    }
}

/// Result of running an assertion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionResult {
    /// Whether the assertion passed
    pub passed: bool,
    /// Human-readable message describing the result
    pub message: String,
    /// Optional additional details (JSON-serializable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl AssertionResult {
    /// Create a passing assertion result
    pub fn pass(message: impl Into<String>) -> Self {
        Self {
            passed: true,
            message: message.into(),
            details: None,
        }
    }

    /// Create a failing assertion result
    pub fn fail(message: impl Into<String>) -> Self {
        Self {
            passed: false,
            message: message.into(),
            details: None,
        }
    }

    /// Add details to the result
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assertion_window_exists() {
        let assertion = Assertion::window_exists("Terminal");
        match assertion {
            Assertion::WindowExists { title } => assert_eq!(title, "Terminal"),
            _ => panic!("Expected WindowExists"),
        }
    }

    #[test]
    fn test_assertion_image_similar() {
        let assertion =
            Assertion::image_similar("/path/to/current.png", "/path/to/baseline.png", 0.95);
        match assertion {
            Assertion::ImageSimilar { threshold, .. } => {
                assert!((threshold - 0.95).abs() < f64::EPSILON);
            }
            _ => panic!("Expected ImageSimilar"),
        }
    }

    #[test]
    fn test_assertion_result_pass() {
        let result = AssertionResult::pass("Window exists");
        assert!(result.passed);
        assert_eq!(result.message, "Window exists");
        assert!(result.details.is_none());
    }

    #[test]
    fn test_assertion_result_fail_with_details() {
        let result = AssertionResult::fail("Window not found")
            .with_details(serde_json::json!({"searched_title": "Test"}));
        assert!(!result.passed);
        assert!(result.details.is_some());
    }

    #[test]
    fn test_assertion_image_similar_threshold_clamped() {
        // Threshold > 1.0 should be clamped to 1.0
        let assertion_high = Assertion::image_similar("/path/a.png", "/path/b.png", 1.5);
        match assertion_high {
            Assertion::ImageSimilar { threshold, .. } => {
                assert!((threshold - 1.0).abs() < f64::EPSILON);
            }
            _ => panic!("Expected ImageSimilar"),
        }

        // Threshold < 0.0 should be clamped to 0.0
        let assertion_low = Assertion::image_similar("/path/a.png", "/path/b.png", -0.5);
        match assertion_low {
            Assertion::ImageSimilar { threshold, .. } => {
                assert!(threshold.abs() < f64::EPSILON);
            }
            _ => panic!("Expected ImageSimilar"),
        }

        // Normal threshold should be preserved
        let assertion_normal = Assertion::image_similar("/path/a.png", "/path/b.png", 0.85);
        match assertion_normal {
            Assertion::ImageSimilar { threshold, .. } => {
                assert!((threshold - 0.85).abs() < f64::EPSILON);
            }
            _ => panic!("Expected ImageSimilar"),
        }
    }
}
