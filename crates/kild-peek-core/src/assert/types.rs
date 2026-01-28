use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Query parameters for finding UI elements (used in assertions)
#[derive(Debug, Clone, Default)]
pub struct ElementQuery {
    /// Filter by accessibility role
    pub role: Option<String>,
    /// Filter by title (partial match)
    pub title: Option<String>,
    /// Filter by label (partial match)
    pub label: Option<String>,
}

impl ElementQuery {
    /// Create a new empty query
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by role
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }

    /// Filter by title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Filter by label
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Check if query has any filters
    pub fn has_filters(&self) -> bool {
        self.role.is_some() || self.title.is_some() || self.label.is_some()
    }
}

/// Types of assertions that can be performed
#[derive(Debug, Clone)]
pub enum Assertion {
    /// Assert that a window with the given title exists
    WindowExists { title: String },
    /// Assert that a window with the given title is visible (not minimized)
    WindowVisible { title: String },
    /// Assert that a UI element matching the query exists in a window
    ElementExists {
        window_title: String,
        query: ElementQuery,
    },
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
    pub fn element_exists(window_title: impl Into<String>, query: ElementQuery) -> Self {
        Assertion::ElementExists {
            window_title: window_title.into(),
            query,
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
    fn test_element_query_builder() {
        let query = ElementQuery::new()
            .with_role("button")
            .with_title("Submit")
            .with_label("submit-btn");

        assert_eq!(query.role, Some("button".to_string()));
        assert_eq!(query.title, Some("Submit".to_string()));
        assert_eq!(query.label, Some("submit-btn".to_string()));
        assert!(query.has_filters());
    }

    #[test]
    fn test_element_query_empty() {
        let query = ElementQuery::new();
        assert!(!query.has_filters());
    }

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
}
