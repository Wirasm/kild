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
    ) -> Self {
        debug_assert!(!role.is_empty(), "Element role must not be empty");
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

    /// Check if any text field contains the given substring (case-insensitive)
    pub fn matches_text(&self, text: &str) -> bool {
        let text_lower = text.to_lowercase();
        let check = |opt: &Option<String>| {
            opt.as_ref()
                .is_some_and(|s| s.to_lowercase().contains(&text_lower))
        };
        check(&self.title) || check(&self.value) || check(&self.description)
    }
}

/// Request to list all elements in a window
#[derive(Debug, Clone)]
pub struct ElementsRequest {
    target: InteractionTarget,
}

impl ElementsRequest {
    pub fn new(target: InteractionTarget) -> Self {
        Self { target }
    }

    pub fn target(&self) -> &InteractionTarget {
        &self.target
    }
}

/// Request to find a specific element by text
#[derive(Debug, Clone)]
pub struct FindRequest {
    target: InteractionTarget,
    text: String,
}

impl FindRequest {
    pub fn new(target: InteractionTarget, text: impl Into<String>) -> Self {
        Self {
            target,
            text: text.into(),
        }
    }

    pub fn target(&self) -> &InteractionTarget {
        &self.target
    }

    pub fn text(&self) -> &str {
        &self.text
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
        );
        let json = serde_json::to_string(&elem).unwrap();
        assert!(json.contains("\"role\":\"AXButton\""));
        assert!(json.contains("\"title\":\"OK\""));
        assert!(json.contains("\"x\":10"));
        assert!(json.contains("\"enabled\":true"));
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
        );
        assert!(elem.matches_text("日本語"));
        assert!(elem.matches_text("text"));
        assert!(elem.matches_text("mixed"));
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
        );
        // Empty string matches everything (contains("") is always true)
        assert!(elem.matches_text(""));
    }
}
