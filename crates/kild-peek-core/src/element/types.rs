use serde::{Deserialize, Serialize};

use crate::interact::InteractionTarget;

/// Information about a UI element discovered via Accessibility API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementInfo {
    role: String,
    title: Option<String>,
    value: Option<String>,
    description: Option<String>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    enabled: bool,
    /// Depth in the accessibility tree (0 = window element itself, 1 = direct children, etc.)
    depth: usize,
}

impl ElementInfo {
    /// Create a new ElementInfo
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        role: String,
        title: Option<String>,
        value: Option<String>,
        description: Option<String>,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        enabled: bool,
        depth: usize,
    ) -> Self {
        debug_assert!(!role.is_empty(), "Element role must not be empty");
        debug_assert!(depth <= 20, "Depth exceeds maximum traversal depth (20)");
        Self {
            role,
            title,
            value,
            description,
            x,
            y,
            width,
            height,
            enabled,
            depth,
        }
    }

    pub fn role(&self) -> &str {
        &self.role
    }
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    pub fn x(&self) -> i32 {
        self.x
    }
    pub fn y(&self) -> i32 {
        self.y
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Check if any text field contains the given substring (case-insensitive)
    pub fn matches_text(&self, text: &str) -> bool {
        let text_lower = text.to_lowercase();
        let check = |opt: &Option<String>| {
            opt.as_ref()
                .is_some_and(|s| s.to_lowercase().contains(&text_lower))
        };
        check(&self.title) || check(&self.value) || check(&self.description)
    }

    /// Check if any text field matches the given regex pattern.
    /// Case-sensitive by default; use `(?i)` prefix in the pattern for case-insensitive matching.
    pub fn matches_regex(&self, pattern: &regex::Regex) -> bool {
        let check = |opt: &Option<String>| opt.as_ref().is_some_and(|s| pattern.is_match(s));
        check(&self.title) || check(&self.value) || check(&self.description)
    }
}

/// Request to list all elements in a window
#[derive(Debug, Clone)]
pub struct ElementsRequest {
    target: InteractionTarget,
    timeout_ms: Option<u64>,
}

impl ElementsRequest {
    pub fn new(target: InteractionTarget) -> Self {
        Self {
            target,
            timeout_ms: None,
        }
    }

    pub fn with_wait(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    pub fn target(&self) -> &InteractionTarget {
        &self.target
    }

    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout_ms
    }
}

/// How to match element text in find operations (used by `FindRequest`)
#[derive(Debug, Clone, Default)]
pub enum FindMode {
    /// Case-insensitive substring match (default)
    #[default]
    Substring,
    /// Regex pattern match (case-sensitive by default)
    Regex,
}

/// Request to find a specific element by text
#[derive(Debug, Clone)]
pub struct FindRequest {
    target: InteractionTarget,
    text: String,
    timeout_ms: Option<u64>,
    mode: FindMode,
}

impl FindRequest {
    pub fn new(target: InteractionTarget, text: impl Into<String>) -> Self {
        Self {
            target,
            text: text.into(),
            timeout_ms: None,
            mode: FindMode::default(),
        }
    }

    pub fn with_wait(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    pub fn with_regex(mut self) -> Self {
        self.mode = FindMode::Regex;
        self
    }

    pub fn target(&self) -> &InteractionTarget {
        &self.target
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout_ms
    }

    pub fn mode(&self) -> &FindMode {
        &self.mode
    }
}

/// Request to wait for an element to appear or disappear
#[derive(Debug, Clone)]
pub struct WaitRequest {
    target: InteractionTarget,
    text: String,
    until_gone: bool,
    timeout_ms: u64,
}

impl WaitRequest {
    pub fn new(target: InteractionTarget, text: impl Into<String>, timeout_ms: u64) -> Self {
        debug_assert!(timeout_ms > 0, "timeout_ms must be > 0");
        Self {
            target,
            text: text.into(),
            until_gone: false,
            timeout_ms,
        }
    }

    pub fn with_until_gone(mut self) -> Self {
        self.until_gone = true;
        self
    }

    pub fn target(&self) -> &InteractionTarget {
        &self.target
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn until_gone(&self) -> bool {
        self.until_gone
    }

    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }
}

/// Result of a wait operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitResult {
    text: String,
    until_gone: bool,
    elapsed_ms: u64,
}

impl WaitResult {
    pub fn appeared(text: impl Into<String>, elapsed_ms: u64) -> Self {
        Self {
            text: text.into(),
            until_gone: false,
            elapsed_ms,
        }
    }

    pub fn gone(text: impl Into<String>, elapsed_ms: u64) -> Self {
        Self {
            text: text.into(),
            until_gone: true,
            elapsed_ms,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn until_gone(&self) -> bool {
        self.until_gone
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms
    }
}

/// Result of listing elements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementsResult {
    elements: Vec<ElementInfo>,
    window: String,
    count: usize,
}

impl ElementsResult {
    pub fn new(elements: Vec<ElementInfo>, window: String) -> Self {
        let count = elements.len();
        Self {
            elements,
            window,
            count,
        }
    }

    pub fn elements(&self) -> &[ElementInfo] {
        &self.elements
    }

    pub fn window(&self) -> &str {
        &self.window
    }

    pub fn count(&self) -> usize {
        self.count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_info_new() {
        let elem = ElementInfo::new(
            "AXButton".to_string(),
            Some("Submit".to_string()),
            None,
            Some("Submit button".to_string()),
            100,
            200,
            80,
            30,
            true,
            5,
        );
        assert_eq!(elem.role(), "AXButton");
        assert_eq!(elem.title(), Some("Submit"));
        assert!(elem.value().is_none());
        assert_eq!(elem.description(), Some("Submit button"));
        assert_eq!(elem.x(), 100);
        assert_eq!(elem.y(), 200);
        assert_eq!(elem.width(), 80);
        assert_eq!(elem.height(), 30);
        assert!(elem.enabled());
        assert_eq!(elem.depth(), 5);
    }

    #[test]
    fn test_element_info_matches_text_title() {
        let elem = ElementInfo::new(
            "AXButton".to_string(),
            Some("Submit Form".to_string()),
            None,
            None,
            0,
            0,
            80,
            30,
            true,
            0,
        );
        assert!(elem.matches_text("submit"));
        assert!(elem.matches_text("Submit"));
        assert!(elem.matches_text("SUBMIT FORM"));
        assert!(!elem.matches_text("Cancel"));
    }

    #[test]
    fn test_element_info_matches_text_value() {
        let elem = ElementInfo::new(
            "AXTextField".to_string(),
            None,
            Some("hello world".to_string()),
            None,
            0,
            0,
            200,
            30,
            true,
            0,
        );
        assert!(elem.matches_text("hello"));
        assert!(elem.matches_text("WORLD"));
        assert!(!elem.matches_text("goodbye"));
    }

    #[test]
    fn test_element_info_matches_text_description() {
        let elem = ElementInfo::new(
            "AXButton".to_string(),
            None,
            None,
            Some("Close window".to_string()),
            0,
            0,
            20,
            20,
            true,
            0,
        );
        assert!(elem.matches_text("close"));
        assert!(!elem.matches_text("open"));
    }

    #[test]
    fn test_element_info_matches_text_no_match() {
        let elem = ElementInfo::new(
            "AXGroup".to_string(),
            None,
            None,
            None,
            0,
            0,
            100,
            100,
            true,
            0,
        );
        assert!(!elem.matches_text("anything"));
    }

    #[test]
    fn test_elements_request_new() {
        let req = ElementsRequest::new(InteractionTarget::App {
            app: "Finder".to_string(),
        });
        match req.target() {
            InteractionTarget::App { app } => assert_eq!(app, "Finder"),
            _ => panic!("Expected App target"),
        }
    }

    #[test]
    fn test_find_request_new() {
        let req = FindRequest::new(
            InteractionTarget::Window {
                title: "KILD".to_string(),
            },
            "Submit",
        );
        assert_eq!(req.text(), "Submit");
    }

    #[test]
    fn test_elements_result_new() {
        let elements = vec![
            ElementInfo::new(
                "AXButton".to_string(),
                Some("OK".to_string()),
                None,
                None,
                0,
                0,
                50,
                30,
                true,
                0,
            ),
            ElementInfo::new(
                "AXButton".to_string(),
                Some("Cancel".to_string()),
                None,
                None,
                60,
                0,
                50,
                30,
                true,
                0,
            ),
        ];
        let result = ElementsResult::new(elements, "Test Window".to_string());
        assert_eq!(result.count(), 2);
        assert_eq!(result.window(), "Test Window");
        assert_eq!(result.elements().len(), 2);
    }

    #[test]
    fn test_element_info_serialization() {
        let elem = ElementInfo::new(
            "AXButton".to_string(),
            Some("OK".to_string()),
            None,
            None,
            10,
            20,
            50,
            30,
            true,
            2,
        );
        let json = serde_json::to_string(&elem).unwrap();
        assert!(json.contains("\"role\":\"AXButton\""));
        assert!(json.contains("\"title\":\"OK\""));
        assert!(json.contains("\"x\":10"));
        assert!(json.contains("\"enabled\":true"));
        assert!(json.contains("\"depth\":2"));
    }

    #[test]
    fn test_elements_result_serialization() {
        let result = ElementsResult::new(vec![], "Window".to_string());
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"count\":0"));
        assert!(json.contains("\"window\":\"Window\""));
    }

    #[test]
    fn test_element_info_matches_text_unicode_accented() {
        let elem = ElementInfo::new(
            "AXButton".to_string(),
            Some("Café résumé".to_string()),
            None,
            None,
            0,
            0,
            80,
            30,
            true,
            0,
        );
        // Case-insensitive matching with accented characters
        assert!(elem.matches_text("café"));
        assert!(elem.matches_text("CAFÉ"));
        assert!(elem.matches_text("résumé"));
        assert!(elem.matches_text("RÉSUMÉ"));
    }

    #[test]
    fn test_element_info_matches_text_unicode_emoji() {
        let elem = ElementInfo::new(
            "AXButton".to_string(),
            Some("Save 💾".to_string()),
            None,
            None,
            0,
            0,
            80,
            30,
            true,
            0,
        );
        assert!(elem.matches_text("Save"));
        assert!(elem.matches_text("💾"));
        assert!(elem.matches_text("Save 💾"));
    }

    #[test]
    fn test_element_info_matches_text_mixed_unicode_ascii() {
        let elem = ElementInfo::new(
            "AXStaticText".to_string(),
            None,
            Some("日本語 text mixed".to_string()),
            None,
            0,
            0,
            200,
            30,
            true,
            0,
        );
        assert!(elem.matches_text("日本語"));
        assert!(elem.matches_text("text"));
        assert!(elem.matches_text("mixed"));
    }

    #[test]
    fn test_elements_request_with_wait() {
        let req = ElementsRequest::new(InteractionTarget::App {
            app: "Finder".to_string(),
        })
        .with_wait(5000);
        assert_eq!(req.timeout_ms(), Some(5000));
    }

    #[test]
    fn test_elements_request_default_timeout_none() {
        let req = ElementsRequest::new(InteractionTarget::App {
            app: "Finder".to_string(),
        });
        assert!(req.timeout_ms().is_none());
    }

    #[test]
    fn test_find_request_with_wait() {
        let req = FindRequest::new(
            InteractionTarget::Window {
                title: "KILD".to_string(),
            },
            "Submit",
        )
        .with_wait(10000);
        assert_eq!(req.timeout_ms(), Some(10000));
    }

    #[test]
    fn test_find_request_default_timeout_none() {
        let req = FindRequest::new(
            InteractionTarget::Window {
                title: "KILD".to_string(),
            },
            "Submit",
        );
        assert!(req.timeout_ms().is_none());
    }

    #[test]
    fn test_matches_regex_exact_match() {
        let elem = ElementInfo::new(
            "AXButton".to_string(),
            Some("File".to_string()),
            None,
            None,
            0,
            0,
            80,
            30,
            true,
            0,
        );
        let pattern = regex::Regex::new("^File$").unwrap();
        assert!(elem.matches_regex(&pattern));

        let pattern_partial = regex::Regex::new("^Fi$").unwrap();
        assert!(!elem.matches_regex(&pattern_partial));
    }

    #[test]
    fn test_matches_regex_alternation() {
        let elem = ElementInfo::new(
            "AXButton".to_string(),
            Some("Create".to_string()),
            None,
            None,
            0,
            0,
            80,
            30,
            true,
            0,
        );
        let pattern = regex::Regex::new("Create|Destroy").unwrap();
        assert!(elem.matches_regex(&pattern));

        let no_match = regex::Regex::new("Open|Close").unwrap();
        assert!(!elem.matches_regex(&no_match));
    }

    #[test]
    fn test_matches_regex_case_sensitive() {
        let elem = ElementInfo::new(
            "AXButton".to_string(),
            Some("Submit".to_string()),
            None,
            None,
            0,
            0,
            80,
            30,
            true,
            0,
        );
        let case_exact = regex::Regex::new("Submit").unwrap();
        assert!(elem.matches_regex(&case_exact));

        let case_wrong = regex::Regex::new("submit").unwrap();
        assert!(!elem.matches_regex(&case_wrong));

        // Case-insensitive via (?i) prefix
        let case_insensitive = regex::Regex::new("(?i)submit").unwrap();
        assert!(elem.matches_regex(&case_insensitive));
    }

    #[test]
    fn test_matches_regex_checks_all_fields() {
        let elem = ElementInfo::new(
            "AXTextField".to_string(),
            None,
            Some("hello world".to_string()),
            Some("input field".to_string()),
            0,
            0,
            200,
            30,
            true,
            0,
        );
        // Matches value
        let value_pattern = regex::Regex::new("hello").unwrap();
        assert!(elem.matches_regex(&value_pattern));

        // Matches description
        let desc_pattern = regex::Regex::new("input").unwrap();
        assert!(elem.matches_regex(&desc_pattern));
    }

    #[test]
    fn test_matches_regex_no_text_fields() {
        let elem = ElementInfo::new(
            "AXGroup".to_string(),
            None,
            None,
            None,
            0,
            0,
            100,
            100,
            true,
            0,
        );
        let pattern = regex::Regex::new(".*").unwrap();
        assert!(!elem.matches_regex(&pattern));
    }

    #[test]
    fn test_element_info_matches_text_empty_search() {
        let elem = ElementInfo::new(
            "AXButton".to_string(),
            Some("Submit".to_string()),
            None,
            None,
            0,
            0,
            80,
            30,
            true,
            0,
        );
        // Empty string matches everything (contains("") is always true)
        assert!(elem.matches_text(""));
    }

    #[test]
    fn test_wait_request_new() {
        let req = WaitRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            "Submit",
            5000,
        );
        assert_eq!(req.text(), "Submit");
        assert_eq!(req.timeout_ms(), 5000);
        assert!(!req.until_gone());
    }

    #[test]
    fn test_wait_request_with_until_gone() {
        let req = WaitRequest::new(
            InteractionTarget::Window {
                title: "KILD".to_string(),
            },
            "Loading...",
            3000,
        )
        .with_until_gone();
        assert!(req.until_gone());
    }

    #[test]
    fn test_wait_result_appeared() {
        let result = WaitResult::appeared("Submit", 150);
        assert_eq!(result.text(), "Submit");
        assert!(!result.until_gone());
        assert_eq!(result.elapsed_ms(), 150);
    }

    #[test]
    fn test_wait_result_gone() {
        let result = WaitResult::gone("Loading...", 200);
        assert_eq!(result.text(), "Loading...");
        assert!(result.until_gone());
        assert_eq!(result.elapsed_ms(), 200);
    }
}
