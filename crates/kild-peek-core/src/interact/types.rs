use serde::{Deserialize, Serialize};

/// Target window for interaction
#[derive(Debug, Clone)]
pub enum InteractionTarget {
    /// Target by window title
    Window { title: String },
    /// Target by app name
    App { app: String },
    /// Target by both app name and window title (for precision)
    AppAndWindow { app: String, title: String },
}

/// Request to click at coordinates within a window
#[derive(Debug, Clone)]
pub struct ClickRequest {
    pub target: InteractionTarget,
    pub x: i32,
    pub y: i32,
}

impl ClickRequest {
    pub fn new(target: InteractionTarget, x: i32, y: i32) -> Self {
        Self { target, x, y }
    }
}

/// Request to type text into the focused element
#[derive(Debug, Clone)]
pub struct TypeRequest {
    pub target: InteractionTarget,
    pub text: String,
}

impl TypeRequest {
    pub fn new(target: InteractionTarget, text: impl Into<String>) -> Self {
        Self {
            target,
            text: text.into(),
        }
    }
}

/// Request to send a key combination
#[derive(Debug, Clone)]
pub struct KeyComboRequest {
    pub target: InteractionTarget,
    /// Key combination string, e.g., "cmd+s", "enter", "cmd+shift+p"
    pub combo: String,
}

impl KeyComboRequest {
    pub fn new(target: InteractionTarget, combo: impl Into<String>) -> Self {
        Self {
            target,
            combo: combo.into(),
        }
    }
}

/// Result of an interaction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionResult {
    pub success: bool,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl InteractionResult {
    /// Create a successful result with details
    pub fn success(action: impl Into<String>, details: serde_json::Value) -> Self {
        Self {
            success: true,
            action: action.into(),
            details: Some(details),
        }
    }

    /// Create a successful result from an action name
    pub fn from_action(action: impl Into<String>) -> Self {
        Self {
            success: true,
            action: action.into(),
            details: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_click_request_new() {
        let req = ClickRequest::new(
            InteractionTarget::Window {
                title: "Terminal".to_string(),
            },
            100,
            50,
        );
        assert_eq!(req.x, 100);
        assert_eq!(req.y, 50);
        match &req.target {
            InteractionTarget::Window { title } => assert_eq!(title, "Terminal"),
            _ => panic!("Expected Window target"),
        }
    }

    #[test]
    fn test_type_request_new() {
        let req = TypeRequest::new(
            InteractionTarget::App {
                app: "TextEdit".to_string(),
            },
            "hello world",
        );
        assert_eq!(req.text, "hello world");
        match &req.target {
            InteractionTarget::App { app } => assert_eq!(app, "TextEdit"),
            _ => panic!("Expected App target"),
        }
    }

    #[test]
    fn test_key_combo_request_new() {
        let req = KeyComboRequest::new(
            InteractionTarget::AppAndWindow {
                app: "Ghostty".to_string(),
                title: "Terminal".to_string(),
            },
            "cmd+s",
        );
        assert_eq!(req.combo, "cmd+s");
        match &req.target {
            InteractionTarget::AppAndWindow { app, title } => {
                assert_eq!(app, "Ghostty");
                assert_eq!(title, "Terminal");
            }
            _ => panic!("Expected AppAndWindow target"),
        }
    }

    #[test]
    fn test_interaction_result_success() {
        let result = InteractionResult::success("click", serde_json::json!({"x": 100, "y": 50}));
        assert!(result.success);
        assert_eq!(result.action, "click");
        assert!(result.details.is_some());
    }

    #[test]
    fn test_interaction_result_from_action() {
        let result = InteractionResult::from_action("type");
        assert!(result.success);
        assert_eq!(result.action, "type");
        assert!(result.details.is_none());
    }

    #[test]
    fn test_interaction_result_serialization() {
        let result = InteractionResult::success(
            "click",
            serde_json::json!({"screen_x": 200, "screen_y": 150}),
        );
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"action\":\"click\""));
        assert!(json.contains("\"screen_x\":200"));
    }

    #[test]
    fn test_interaction_result_serialization_no_details() {
        let result = InteractionResult::from_action("key");
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"action\":\"key\""));
        // details should be skipped when None
        assert!(!json.contains("details"));
    }

    #[test]
    fn test_interaction_target_debug() {
        let target = InteractionTarget::Window {
            title: "Test".to_string(),
        };
        let debug = format!("{:?}", target);
        assert!(debug.contains("Window"));
        assert!(debug.contains("Test"));
    }
}
