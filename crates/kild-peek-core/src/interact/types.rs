use serde::{Deserialize, Serialize};

/// Click modifier for right-click and double-click
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClickModifier {
    /// Standard left-click
    #[default]
    None,
    /// Right-click (context menu)
    Right,
    /// Double-click
    Double,
}

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
    target: InteractionTarget,
    x: i32,
    y: i32,
    modifier: ClickModifier,
    timeout_ms: Option<u64>,
}

impl ClickRequest {
    pub fn new(target: InteractionTarget, x: i32, y: i32) -> Self {
        Self {
            target,
            x,
            y,
            modifier: ClickModifier::default(),
            timeout_ms: None,
        }
    }

    pub fn with_wait(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    pub fn with_modifier(mut self, modifier: ClickModifier) -> Self {
        self.modifier = modifier;
        self
    }

    pub fn target(&self) -> &InteractionTarget {
        &self.target
    }

    pub fn x(&self) -> i32 {
        self.x
    }

    pub fn y(&self) -> i32 {
        self.y
    }

    pub fn modifier(&self) -> ClickModifier {
        self.modifier
    }

    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout_ms
    }
}

/// Request to type text into the focused element
#[derive(Debug, Clone)]
pub struct TypeRequest {
    target: InteractionTarget,
    text: String,
    timeout_ms: Option<u64>,
}

impl TypeRequest {
    pub fn new(target: InteractionTarget, text: impl Into<String>) -> Self {
        Self {
            target,
            text: text.into(),
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

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout_ms
    }
}

/// Request to send a key combination
#[derive(Debug, Clone)]
pub struct KeyComboRequest {
    target: InteractionTarget,
    /// Key combination string, e.g., "cmd+s", "enter", "cmd+shift+p"
    combo: String,
    timeout_ms: Option<u64>,
}

impl KeyComboRequest {
    pub fn new(target: InteractionTarget, combo: impl Into<String>) -> Self {
        Self {
            target,
            combo: combo.into(),
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

    pub fn combo(&self) -> &str {
        &self.combo
    }

    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout_ms
    }
}

/// Request to click an element identified by text content
#[derive(Debug, Clone)]
pub struct ClickTextRequest {
    target: InteractionTarget,
    text: String,
    modifier: ClickModifier,
    timeout_ms: Option<u64>,
}

impl ClickTextRequest {
    pub fn new(target: InteractionTarget, text: impl Into<String>) -> Self {
        Self {
            target,
            text: text.into(),
            modifier: ClickModifier::default(),
            timeout_ms: None,
        }
    }

    pub fn with_wait(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    pub fn with_modifier(mut self, modifier: ClickModifier) -> Self {
        self.modifier = modifier;
        self
    }

    pub fn target(&self) -> &InteractionTarget {
        &self.target
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn modifier(&self) -> ClickModifier {
        self.modifier
    }

    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout_ms
    }
}

/// Request to drag from one point to another within a window
#[derive(Debug, Clone)]
pub struct DragRequest {
    target: InteractionTarget,
    from_x: i32,
    from_y: i32,
    to_x: i32,
    to_y: i32,
    timeout_ms: Option<u64>,
}

impl DragRequest {
    pub fn new(target: InteractionTarget, from_x: i32, from_y: i32, to_x: i32, to_y: i32) -> Self {
        Self {
            target,
            from_x,
            from_y,
            to_x,
            to_y,
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

    pub fn from_x(&self) -> i32 {
        self.from_x
    }

    pub fn from_y(&self) -> i32 {
        self.from_y
    }

    pub fn to_x(&self) -> i32 {
        self.to_x
    }

    pub fn to_y(&self) -> i32 {
        self.to_y
    }

    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout_ms
    }
}

/// Request to scroll within a window
#[derive(Debug, Clone)]
pub struct ScrollRequest {
    target: InteractionTarget,
    delta_x: i32,
    delta_y: i32,
    at_x: Option<i32>,
    at_y: Option<i32>,
    timeout_ms: Option<u64>,
}

impl ScrollRequest {
    pub fn new(target: InteractionTarget, delta_x: i32, delta_y: i32) -> Self {
        Self {
            target,
            delta_x,
            delta_y,
            at_x: None,
            at_y: None,
            timeout_ms: None,
        }
    }

    pub fn with_at(mut self, x: i32, y: i32) -> Self {
        self.at_x = Some(x);
        self.at_y = Some(y);
        self
    }

    pub fn with_wait(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    pub fn target(&self) -> &InteractionTarget {
        &self.target
    }

    pub fn delta_x(&self) -> i32 {
        self.delta_x
    }

    pub fn delta_y(&self) -> i32 {
        self.delta_y
    }

    pub fn at_x(&self) -> Option<i32> {
        self.at_x
    }

    pub fn at_y(&self) -> Option<i32> {
        self.at_y
    }

    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout_ms
    }
}

/// Request to hover (move mouse without clicking) at coordinates within a window
#[derive(Debug, Clone)]
pub struct HoverRequest {
    target: InteractionTarget,
    x: i32,
    y: i32,
    timeout_ms: Option<u64>,
}

impl HoverRequest {
    pub fn new(target: InteractionTarget, x: i32, y: i32) -> Self {
        Self {
            target,
            x,
            y,
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

    pub fn x(&self) -> i32 {
        self.x
    }

    pub fn y(&self) -> i32 {
        self.y
    }

    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout_ms
    }
}

/// Request to hover over an element identified by text content
#[derive(Debug, Clone)]
pub struct HoverTextRequest {
    target: InteractionTarget,
    text: String,
    timeout_ms: Option<u64>,
}

impl HoverTextRequest {
    pub fn new(target: InteractionTarget, text: impl Into<String>) -> Self {
        Self {
            target,
            text: text.into(),
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

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout_ms
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
        assert_eq!(req.x(), 100);
        assert_eq!(req.y(), 50);
        match req.target() {
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
        assert_eq!(req.text(), "hello world");
        match req.target() {
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
        assert_eq!(req.combo(), "cmd+s");
        match req.target() {
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
    fn test_click_text_request_new() {
        let req = ClickTextRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            "Submit",
        );
        assert_eq!(req.text(), "Submit");
        match req.target() {
            InteractionTarget::App { app } => assert_eq!(app, "Finder"),
            _ => panic!("Expected App target"),
        }
    }

    #[test]
    fn test_click_text_request_with_string() {
        let req = ClickTextRequest::new(
            InteractionTarget::Window {
                title: "KILD".to_string(),
            },
            String::from("Create"),
        );
        assert_eq!(req.text(), "Create");
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

    #[test]
    fn test_click_request_default_timeout_none() {
        let req = ClickRequest::new(
            InteractionTarget::Window {
                title: "Terminal".to_string(),
            },
            100,
            50,
        );
        assert!(req.timeout_ms().is_none());
    }

    #[test]
    fn test_click_request_with_wait() {
        let req = ClickRequest::new(
            InteractionTarget::Window {
                title: "Terminal".to_string(),
            },
            100,
            50,
        )
        .with_wait(5000);
        assert_eq!(req.timeout_ms(), Some(5000));
    }

    #[test]
    fn test_type_request_with_wait() {
        let req = TypeRequest::new(
            InteractionTarget::App {
                app: "TextEdit".to_string(),
            },
            "hello",
        )
        .with_wait(3000);
        assert_eq!(req.timeout_ms(), Some(3000));
    }

    #[test]
    fn test_key_combo_request_with_wait() {
        let req = KeyComboRequest::new(
            InteractionTarget::App {
                app: "Ghostty".to_string(),
            },
            "cmd+s",
        )
        .with_wait(10000);
        assert_eq!(req.timeout_ms(), Some(10000));
    }

    #[test]
    fn test_click_text_request_with_wait() {
        let req = ClickTextRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            "File",
        )
        .with_wait(5000);
        assert_eq!(req.timeout_ms(), Some(5000));
    }

    #[test]
    fn test_click_text_request_default_timeout_none() {
        let req = ClickTextRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            "File",
        );
        assert!(req.timeout_ms().is_none());
    }

    #[test]
    fn test_click_modifier_default() {
        let modifier = ClickModifier::default();
        assert_eq!(modifier, ClickModifier::None);
    }

    #[test]
    fn test_click_modifier_variants() {
        assert_ne!(ClickModifier::Right, ClickModifier::None);
        assert_ne!(ClickModifier::Double, ClickModifier::None);
        assert_ne!(ClickModifier::Right, ClickModifier::Double);
    }

    #[test]
    fn test_click_request_default_modifier() {
        let req = ClickRequest::new(
            InteractionTarget::Window {
                title: "Test".to_string(),
            },
            100,
            50,
        );
        assert_eq!(req.modifier(), ClickModifier::None);
    }

    #[test]
    fn test_click_request_with_modifier() {
        let req = ClickRequest::new(
            InteractionTarget::Window {
                title: "Test".to_string(),
            },
            100,
            50,
        )
        .with_modifier(ClickModifier::Right);
        assert_eq!(req.modifier(), ClickModifier::Right);
    }

    #[test]
    fn test_click_text_request_default_modifier() {
        let req = ClickTextRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            "File",
        );
        assert_eq!(req.modifier(), ClickModifier::None);
    }

    #[test]
    fn test_click_text_request_with_modifier() {
        let req = ClickTextRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            "File",
        )
        .with_modifier(ClickModifier::Double);
        assert_eq!(req.modifier(), ClickModifier::Double);
    }

    #[test]
    fn test_drag_request_new() {
        let req = DragRequest::new(
            InteractionTarget::Window {
                title: "Terminal".to_string(),
            },
            10,
            20,
            300,
            200,
        );
        assert_eq!(req.from_x(), 10);
        assert_eq!(req.from_y(), 20);
        assert_eq!(req.to_x(), 300);
        assert_eq!(req.to_y(), 200);
        assert!(req.timeout_ms().is_none());
    }

    #[test]
    fn test_drag_request_with_wait() {
        let req = DragRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            0,
            0,
            100,
            100,
        )
        .with_wait(5000);
        assert_eq!(req.timeout_ms(), Some(5000));
    }

    #[test]
    fn test_scroll_request_new() {
        let req = ScrollRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            0,
            5,
        );
        assert_eq!(req.delta_x(), 0);
        assert_eq!(req.delta_y(), 5);
        assert!(req.at_x().is_none());
        assert!(req.at_y().is_none());
        assert!(req.timeout_ms().is_none());
    }

    #[test]
    fn test_scroll_request_with_at() {
        let req = ScrollRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            0,
            -3,
        )
        .with_at(100, 200);
        assert_eq!(req.at_x(), Some(100));
        assert_eq!(req.at_y(), Some(200));
    }

    #[test]
    fn test_scroll_request_with_wait() {
        let req = ScrollRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            2,
            0,
        )
        .with_wait(3000);
        assert_eq!(req.timeout_ms(), Some(3000));
    }

    #[test]
    fn test_hover_request_new() {
        let req = HoverRequest::new(
            InteractionTarget::Window {
                title: "Terminal".to_string(),
            },
            150,
            75,
        );
        assert_eq!(req.x(), 150);
        assert_eq!(req.y(), 75);
        assert!(req.timeout_ms().is_none());
    }

    #[test]
    fn test_hover_request_with_wait() {
        let req = HoverRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            50,
            50,
        )
        .with_wait(2000);
        assert_eq!(req.timeout_ms(), Some(2000));
    }

    #[test]
    fn test_hover_text_request_new() {
        let req = HoverTextRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            "File",
        );
        assert_eq!(req.text(), "File");
        assert!(req.timeout_ms().is_none());
    }

    #[test]
    fn test_hover_text_request_with_wait() {
        let req = HoverTextRequest::new(
            InteractionTarget::App {
                app: "Finder".to_string(),
            },
            "Edit",
        )
        .with_wait(4000);
        assert_eq!(req.timeout_ms(), Some(4000));
    }
}
