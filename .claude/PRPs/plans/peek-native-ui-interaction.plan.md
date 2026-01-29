# Feature: Native UI Interaction Capabilities for kild-peek

## Summary

Transform kild-peek from a read-only visual verification tool into a full native UI automation framework by adding interaction capabilities: clicking, typing, key combinations, element inspection, and wait mechanisms. This enables automated E2E testing of native macOS applications (including kild-ui) without web-based testing frameworks.

## User Story

As a developer testing native macOS applications
I want to automate UI interactions through kild-peek
So that I can create reliable E2E tests for native apps without manual intervention

## Problem Statement

Currently kild-peek can **observe** native UIs (list windows, take screenshots, compare images, assert visibility) but cannot **interact** with them. This limits its usefulness for:
- Automated E2E testing of kild-ui itself
- Testing any native macOS application
- Building automated workflows for AI agents
- Regression testing with real UI interactions

## Solution Statement

Add interaction capabilities using:
1. **enigo** crate for cross-platform input simulation (mouse clicks, keyboard input)
2. **accessibility** crate for macOS Accessibility API (element inspection, finding UI elements)
3. New CLI commands: `click`, `type`, `key`, `elements`, `wait`
4. Permission detection and helpful error messages for Accessibility API access

---

## Metadata

| Field            | Value                                                        |
| ---------------- | ------------------------------------------------------------ |
| Type             | NEW_CAPABILITY                                               |
| Complexity       | HIGH                                                         |
| Systems Affected | kild-peek-core, kild-peek                                    |
| Dependencies     | enigo 0.2+, accessibility 0.2+, accessibility-sys            |
| Estimated Tasks  | 18                                                           |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │  kild-peek  │ ──────► │  list/      │ ──────► │ Visual      │            ║
║   │  CLI        │         │ screenshot/ │         │ Output      │            ║
║   └─────────────┘         │ diff/assert │         │ Only        │            ║
║                           └─────────────┘         └─────────────┘            ║
║                                                                               ║
║   USER_FLOW:                                                                  ║
║   1. User runs: kild-peek list windows                                        ║
║   2. User runs: kild-peek screenshot --window "App"                           ║
║   3. User runs: kild-peek assert --window "App" --exists                      ║
║   4. User CANNOT click, type, or interact - must do manually                  ║
║                                                                               ║
║   PAIN_POINT: No way to automate interactions for E2E testing                 ║
║   DATA_FLOW: kild-peek → xcap → macOS APIs → Read-only data                  ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │  kild-peek  │ ──────► │  Existing   │ ──────► │ Visual      │            ║
║   │  CLI        │         │  Commands   │         │ Output      │            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║         │                                                                     ║
║         │ NEW                                                                 ║
║         ▼                                                                     ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │  click      │ ──────► │  enigo +    │ ──────► │ Mouse/Key   │            ║
║   │  type       │         │  CGEvent    │         │ Events      │            ║
║   │  key        │         └─────────────┘         │ Sent        │            ║
║   └─────────────┘                                 └─────────────┘            ║
║         │                                                                     ║
║         ▼                                                                     ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │  elements   │ ──────► │ Accessibility│ ──────► │ UI Element  │            ║
║   │  wait       │         │ API         │         │ Tree        │            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                                                               ║
║   USER_FLOW:                                                                  ║
║   1. User runs: kild-peek click --window "App" --at 100,50                    ║
║   2. User runs: kild-peek type --window "App" "my-text"                       ║
║   3. User runs: kild-peek elements --window "App" --role button               ║
║   4. User runs: kild-peek wait --window "App" --text "Success"                ║
║   5. Full E2E test automation without manual intervention                     ║
║                                                                               ║
║   VALUE_ADD: Complete native UI automation framework                          ║
║   DATA_FLOW: kild-peek → enigo/accessibility → CGEvent/AXUIElement → App     ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| `kild-peek click` | N/A | Click at coordinates or on element | Can automate button clicks |
| `kild-peek type` | N/A | Type text into focused element | Can automate text input |
| `kild-peek key` | N/A | Send key combinations | Can send Enter, Tab, Cmd+S |
| `kild-peek elements` | N/A | List UI elements via Accessibility API | Can inspect app structure |
| `kild-peek wait` | N/A | Wait for element state | Can synchronize test steps |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-peek-core/src/window/types.rs` | 1-158 | Pattern for struct with private fields + getters |
| P0 | `crates/kild-peek-core/src/window/errors.rs` | 1-84 | Pattern for error types implementing PeekError |
| P0 | `crates/kild-peek-core/src/window/handler.rs` | 1-100 | Pattern for handler functions with structured logging |
| P0 | `crates/kild-peek-core/src/assert/types.rs` | 1-230 | ElementQuery pattern (already designed for accessibility) |
| P1 | `crates/kild-peek/src/app.rs` | 1-176 | CLI command builder pattern with clap |
| P1 | `crates/kild-peek/src/commands.rs` | 1-297 | Command dispatch and handler pattern |
| P2 | `crates/kild-peek-core/src/lib.rs` | 1-35 | Public API export pattern |

**External Documentation:**

| Source | Section | Why Needed |
|--------|---------|------------|
| [enigo docs](https://docs.rs/enigo/) | Key/Mouse traits | Input simulation API |
| [accessibility crate](https://docs.rs/accessibility/latest/accessibility/) | ui_element, TreeWalker | Element inspection API |
| [Apple AXIsProcessTrusted](https://developer.apple.com/documentation/applicationservices/1460720-axisprocesstrusted) | Permission check | Detecting accessibility access |

---

## Patterns to Mirror

**NAMING_CONVENTION:**
```rust
// SOURCE: crates/kild-peek-core/src/window/types.rs:4-14
// COPY THIS PATTERN:
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    id: u32,
    title: String,
    // ... private fields with public getters
}
```

**ERROR_HANDLING:**
```rust
// SOURCE: crates/kild-peek-core/src/window/errors.rs:3-40
// COPY THIS PATTERN:
#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("Failed to enumerate windows: {message}")]
    EnumerationFailed { message: String },
    // ...
}

impl PeekError for WindowError {
    fn error_code(&self) -> &'static str {
        match self {
            WindowError::EnumerationFailed { .. } => "WINDOW_ENUMERATION_FAILED",
            // ...
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(self, WindowError::WindowNotFound { .. } | ...)
    }
}
```

**LOGGING_PATTERN:**
```rust
// SOURCE: crates/kild-peek-core/src/window/handler.rs:7-9
// COPY THIS PATTERN:
pub fn list_windows() -> Result<Vec<WindowInfo>, WindowError> {
    info!(event = "core.window.list_started");
    // ... implementation ...
    info!(event = "core.window.list_completed", count = result.len());
    Ok(result)
}
```

**BUILDER_PATTERN:**
```rust
// SOURCE: crates/kild-peek-core/src/assert/types.rs:15-37
// COPY THIS PATTERN:
impl ElementQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }
}
```

**CLI_COMMAND_PATTERN:**
```rust
// SOURCE: crates/kild-peek/src/app.rs:48-99
// COPY THIS PATTERN:
.subcommand(
    Command::new("screenshot")
        .about("Capture a screenshot")
        .arg(
            Arg::new("window")
                .long("window")
                .short('w')
                .help("...")
                .conflicts_with_all(["window-id", "monitor"]),
        )
        // ...
)
```

**COMMAND_DISPATCH_PATTERN:**
```rust
// SOURCE: crates/kild-peek/src/commands.rs:14-27
// COPY THIS PATTERN:
pub fn run_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    events::log_app_startup();

    match matches.subcommand() {
        Some(("list", sub_matches)) => handle_list_command(sub_matches),
        Some(("screenshot", sub_matches)) => handle_screenshot_command(sub_matches),
        // ...
        _ => {
            error!(event = "cli.command_unknown");
            Err("Unknown command".into())
        }
    }
}
```

---

## Files to Change

| File                                              | Action | Justification                                |
| ------------------------------------------------- | ------ | -------------------------------------------- |
| `Cargo.toml` (workspace)                          | UPDATE | Add enigo, accessibility, accessibility-sys  |
| `crates/kild-peek-core/Cargo.toml`                | UPDATE | Add new dependencies                         |
| `crates/kild-peek-core/src/lib.rs`                | UPDATE | Export new modules                           |
| `crates/kild-peek-core/src/permission/mod.rs`     | CREATE | Accessibility permission checking            |
| `crates/kild-peek-core/src/permission/types.rs`   | CREATE | Permission status types                      |
| `crates/kild-peek-core/src/permission/errors.rs`  | CREATE | Permission errors                            |
| `crates/kild-peek-core/src/permission/handler.rs` | CREATE | Permission check implementation              |
| `crates/kild-peek-core/src/interact/mod.rs`       | CREATE | Input interaction module                     |
| `crates/kild-peek-core/src/interact/types.rs`     | CREATE | Click, Type, Key types                       |
| `crates/kild-peek-core/src/interact/errors.rs`    | CREATE | Interaction errors                           |
| `crates/kild-peek-core/src/interact/handler.rs`   | CREATE | Click, type, key implementations             |
| `crates/kild-peek-core/src/elements/mod.rs`       | CREATE | Element inspection module                    |
| `crates/kild-peek-core/src/elements/types.rs`     | CREATE | Element, ElementRole types                   |
| `crates/kild-peek-core/src/elements/errors.rs`    | CREATE | Element errors                               |
| `crates/kild-peek-core/src/elements/handler.rs`   | CREATE | Element finding, listing, waiting            |
| `crates/kild-peek/src/app.rs`                     | UPDATE | Add click, type, key, elements, wait commands|
| `crates/kild-peek/src/commands.rs`                | UPDATE | Add command handlers                         |
| `crates/kild-peek/src/table.rs`                   | UPDATE | Add elements table formatter                 |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Drag and drop** - Deferred to Phase 4 per issue spec
- **Scroll operations** - Deferred to Phase 4 per issue spec
- **Hover/tooltip** - Deferred to Phase 4 per issue spec
- **Element tree visualization** - Deferred to Phase 5 per issue spec
- **Fuzzy/regex text matching** - Deferred to Phase 5 per issue spec
- **Record mode** - Future work, not in this plan
- **Linux/Windows support** - macOS-only initially
- **OCR fallback** - Future work for apps without accessibility

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `Cargo.toml` (workspace dependencies)

- **ACTION**: ADD new dependencies to workspace
- **IMPLEMENT**: Add enigo, accessibility, accessibility-sys to workspace dependencies
- **MIRROR**: `Cargo.toml:34-38` - follow existing dependency format
- **IMPORTS**: N/A (TOML file)
- **GOTCHA**: Use `enigo = "0.2"` (version 0.2+ has Settings struct); accessibility = "0.2"
- **VALIDATE**: `cargo check -p kild-peek-core`

```toml
# Add to [workspace.dependencies] section:
enigo = "0.2"
accessibility = "0.2"
accessibility-sys = "0.1"
```

### Task 2: UPDATE `crates/kild-peek-core/Cargo.toml`

- **ACTION**: ADD dependencies to kild-peek-core
- **IMPLEMENT**: Reference workspace dependencies
- **MIRROR**: `crates/kild-peek-core/Cargo.toml:8-17`
- **IMPORTS**: N/A (TOML file)
- **GOTCHA**: Use `.workspace = true` pattern
- **VALIDATE**: `cargo check -p kild-peek-core`

```toml
# Add to [dependencies] section:
enigo.workspace = true
accessibility.workspace = true
accessibility-sys.workspace = true
```

### Task 3: CREATE `crates/kild-peek-core/src/permission/mod.rs`

- **ACTION**: CREATE permission module structure
- **IMPLEMENT**: Module exports for permission checking
- **MIRROR**: `crates/kild-peek-core/src/window/mod.rs` (pattern)
- **PATTERN**: `mod errors; mod types; mod handler; pub use ...`
- **VALIDATE**: `cargo check -p kild-peek-core`

### Task 4: CREATE `crates/kild-peek-core/src/permission/types.rs`

- **ACTION**: CREATE permission status types
- **IMPLEMENT**: `PermissionStatus` enum (Granted, Denied, Unknown), `PermissionCheck` struct
- **MIRROR**: `crates/kild-peek-core/src/window/types.rs:4-42`
- **PATTERN**: Private fields, public getters, Serialize/Deserialize
- **VALIDATE**: `cargo check -p kild-peek-core`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionStatus {
    Granted,
    Denied,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionCheck {
    accessibility: PermissionStatus,
    message: String,
}
```

### Task 5: CREATE `crates/kild-peek-core/src/permission/errors.rs`

- **ACTION**: CREATE permission error types
- **IMPLEMENT**: `PermissionError` enum with AccessibilityDenied variant
- **MIRROR**: `crates/kild-peek-core/src/window/errors.rs:1-40`
- **PATTERN**: thiserror, PeekError trait implementation
- **VALIDATE**: `cargo check -p kild-peek-core`

```rust
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    #[error("Accessibility permission denied. Enable in System Settings > Privacy & Security > Accessibility")]
    AccessibilityDenied,

    #[error("Failed to check permissions: {message}")]
    CheckFailed { message: String },
}
```

### Task 6: CREATE `crates/kild-peek-core/src/permission/handler.rs`

- **ACTION**: CREATE permission check implementation
- **IMPLEMENT**: `check_accessibility()` using AXIsProcessTrusted, `request_accessibility()` using AXIsProcessTrustedWithOptions
- **MIRROR**: `crates/kild-peek-core/src/window/handler.rs:6-10` (logging pattern)
- **IMPORTS**: `use accessibility_sys::{AXIsProcessTrusted, AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt}`
- **GOTCHA**: Use unsafe block for FFI calls; return helpful error message directing users to System Settings
- **VALIDATE**: `cargo check -p kild-peek-core && cargo test -p kild-peek-core permission`

```rust
pub fn check_accessibility() -> Result<PermissionCheck, PermissionError> {
    info!(event = "core.permission.check_started");

    let is_trusted = unsafe { AXIsProcessTrusted() != 0 };

    let status = if is_trusted {
        PermissionStatus::Granted
    } else {
        PermissionStatus::Denied
    };

    info!(event = "core.permission.check_completed", status = ?status);
    Ok(PermissionCheck::new(status))
}
```

### Task 7: CREATE `crates/kild-peek-core/src/interact/mod.rs`

- **ACTION**: CREATE interaction module structure
- **IMPLEMENT**: Module exports for click, type, key operations
- **MIRROR**: `crates/kild-peek-core/src/window/mod.rs`
- **VALIDATE**: `cargo check -p kild-peek-core`

### Task 8: CREATE `crates/kild-peek-core/src/interact/types.rs`

- **ACTION**: CREATE interaction types
- **IMPLEMENT**: `ClickTarget`, `ClickRequest`, `TypeRequest`, `KeyRequest`, `InteractionResult`
- **MIRROR**: `crates/kild-peek-core/src/screenshot/types.rs:5-84` (builder pattern)
- **PATTERN**: Builder pattern, Into trait for flexibility
- **VALIDATE**: `cargo check -p kild-peek-core`

```rust
#[derive(Debug, Clone)]
pub enum ClickTarget {
    Coordinates { x: i32, y: i32 },
    Text { text: String },
    Label { label: String },
}

#[derive(Debug, Clone)]
pub struct ClickRequest {
    pub window: String,
    pub target: ClickTarget,
    pub button: MouseButton,
    pub click_count: u8,  // 1=single, 2=double
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone)]
pub struct TypeRequest {
    pub window: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct KeyRequest {
    pub window: String,
    pub key: String,  // e.g., "enter", "tab", "cmd+s"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionResult {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}
```

### Task 9: CREATE `crates/kild-peek-core/src/interact/errors.rs`

- **ACTION**: CREATE interaction error types
- **IMPLEMENT**: `InteractionError` enum (PermissionDenied, WindowNotFound, ElementNotFound, InputFailed)
- **MIRROR**: `crates/kild-peek-core/src/window/errors.rs:1-40`
- **VALIDATE**: `cargo check -p kild-peek-core`

```rust
#[derive(Debug, thiserror::Error)]
pub enum InteractionError {
    #[error("Accessibility permission denied. Enable in System Settings > Privacy & Security > Accessibility")]
    PermissionDenied,

    #[error("Window not found: '{title}'")]
    WindowNotFound { title: String },

    #[error("Element not found: {query}")]
    ElementNotFound { query: String },

    #[error("Failed to send input: {message}")]
    InputFailed { message: String },

    #[error("Invalid key combination: {key}")]
    InvalidKey { key: String },
}
```

### Task 10: CREATE `crates/kild-peek-core/src/interact/handler.rs`

- **ACTION**: CREATE interaction implementations
- **IMPLEMENT**: `click()`, `type_text()`, `send_key()` using enigo
- **MIRROR**: `crates/kild-peek-core/src/window/handler.rs:250-329` (find_window_by_title pattern)
- **IMPORTS**: `use enigo::{Enigo, Key, Mouse, Keyboard, Settings, Coordinate, Button, Direction}`
- **GOTCHA**:
  - Check accessibility permission before any operation
  - Focus window before interaction
  - Use 12ms sleep after key events for proper character output
  - Parse key combos like "cmd+s" into Enigo Key variants
- **VALIDATE**: `cargo check -p kild-peek-core && cargo test -p kild-peek-core interact`

```rust
pub fn click(request: &ClickRequest) -> Result<InteractionResult, InteractionError> {
    info!(event = "core.interact.click_started", window = &request.window, target = ?request.target);

    // Check permission first
    ensure_accessibility_permission()?;

    // Find and focus window
    let window = find_and_focus_window(&request.window)?;

    // Get click coordinates
    let (x, y) = match &request.target {
        ClickTarget::Coordinates { x, y } => (*x, *y),
        ClickTarget::Text { text } => find_element_position_by_text(&window, text)?,
        ClickTarget::Label { label } => find_element_position_by_label(&window, label)?,
    };

    // Convert to absolute screen coordinates
    let abs_x = window.x() + x;
    let abs_y = window.y() + y;

    // Perform click using enigo
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| InteractionError::InputFailed { message: e.to_string() })?;

    enigo.move_mouse(abs_x, abs_y, Coordinate::Abs)
        .map_err(|e| InteractionError::InputFailed { message: e.to_string() })?;

    let button = match request.button {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
    };

    for _ in 0..request.click_count {
        enigo.button(button, Direction::Click)
            .map_err(|e| InteractionError::InputFailed { message: e.to_string() })?;
    }

    info!(event = "core.interact.click_completed", x = abs_x, y = abs_y);
    Ok(InteractionResult::success(format!("Clicked at ({}, {})", abs_x, abs_y)))
}
```

### Task 11: CREATE `crates/kild-peek-core/src/elements/mod.rs`

- **ACTION**: CREATE elements module structure
- **IMPLEMENT**: Module exports for element inspection
- **MIRROR**: `crates/kild-peek-core/src/window/mod.rs`
- **VALIDATE**: `cargo check -p kild-peek-core`

### Task 12: CREATE `crates/kild-peek-core/src/elements/types.rs`

- **ACTION**: CREATE element types
- **IMPLEMENT**: `UIElement`, `ElementRole`, `WaitCondition`, `WaitRequest`
- **MIRROR**: `crates/kild-peek-core/src/window/types.rs:4-42` (private fields pattern)
- **IMPORTS**: `use crate::assert::ElementQuery` (reuse existing)
- **VALIDATE**: `cargo check -p kild-peek-core`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIElement {
    role: String,
    label: Option<String>,
    title: Option<String>,
    value: Option<String>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone)]
pub enum WaitCondition {
    Exists,
    Gone,
    Visible,
    Hidden,
}

#[derive(Debug, Clone)]
pub struct WaitRequest {
    pub window: String,
    pub query: ElementQuery,
    pub condition: WaitCondition,
    pub timeout_ms: u64,
    pub poll_interval_ms: u64,
}
```

### Task 13: CREATE `crates/kild-peek-core/src/elements/errors.rs`

- **ACTION**: CREATE element error types
- **IMPLEMENT**: `ElementError` enum (PermissionDenied, WindowNotFound, Timeout, AccessError)
- **MIRROR**: `crates/kild-peek-core/src/window/errors.rs:1-40`
- **VALIDATE**: `cargo check -p kild-peek-core`

### Task 14: CREATE `crates/kild-peek-core/src/elements/handler.rs`

- **ACTION**: CREATE element inspection implementations
- **IMPLEMENT**: `list_elements()`, `find_element()`, `wait_for_element()` using accessibility crate
- **MIRROR**: `crates/kild-peek-core/src/window/handler.rs:6-146` (list pattern with filter_map)
- **IMPORTS**: `use accessibility::{ui_element::UIElementRef, attribute::*, TreeWalker, TreeVisitor, TreeWalkerFlow}`
- **GOTCHA**:
  - Use TreeWalker for element traversal
  - Filter by role using AXAttribute::role()
  - Get position using AXAttribute::position(), AXAttribute::size()
  - wait_for_element uses polling loop with timeout
- **VALIDATE**: `cargo check -p kild-peek-core && cargo test -p kild-peek-core elements`

```rust
pub fn list_elements(
    window_title: &str,
    query: &ElementQuery,
) -> Result<Vec<UIElement>, ElementError> {
    info!(event = "core.elements.list_started", window = window_title);

    ensure_accessibility_permission()?;

    let window = find_window_by_title(window_title)
        .map_err(|_| ElementError::WindowNotFound { title: window_title.to_string() })?;

    // Get app element from PID
    let app_element = get_app_element_for_window(&window)?;

    let mut elements = Vec::new();

    // Walk the element tree
    let walker = TreeWalker::new(app_element);
    walker.walk(&mut |element| {
        if let Ok(ui_element) = extract_ui_element(element) {
            if matches_query(&ui_element, query) {
                elements.push(ui_element);
            }
        }
        TreeWalkerFlow::Continue
    });

    info!(event = "core.elements.list_completed", count = elements.len());
    Ok(elements)
}
```

### Task 15: UPDATE `crates/kild-peek-core/src/lib.rs`

- **ACTION**: EXPORT new modules
- **IMPLEMENT**: Add `pub mod permission;`, `pub mod interact;`, `pub mod elements;` and re-exports
- **MIRROR**: `crates/kild-peek-core/src/lib.rs:12-35`
- **VALIDATE**: `cargo check -p kild-peek-core`

```rust
pub mod permission;
pub mod interact;
pub mod elements;

// Re-export permission types
pub use permission::{PermissionCheck, PermissionStatus, check_accessibility};

// Re-export interact types
pub use interact::{ClickRequest, ClickTarget, TypeRequest, KeyRequest, InteractionResult, click, type_text, send_key};

// Re-export elements types
pub use elements::{UIElement, WaitCondition, WaitRequest, list_elements, find_element, wait_for_element};
```

### Task 16: UPDATE `crates/kild-peek/src/app.rs`

- **ACTION**: ADD CLI commands for click, type, key, elements, wait
- **IMPLEMENT**: New subcommands with appropriate arguments
- **MIRROR**: `crates/kild-peek/src/app.rs:48-99` (screenshot command pattern)
- **GOTCHA**:
  - click: --window (required), --at "x,y" | --text "x" | --label "x", --double, --right
  - type: --window (required), text positional arg
  - key: --window (required), key combo positional arg
  - elements: --window (required), --role, --json
  - wait: --window (required), --text | --role, --timeout, --until-gone
- **VALIDATE**: `cargo test -p kild-peek app`

### Task 17: UPDATE `crates/kild-peek/src/commands.rs`

- **ACTION**: ADD command handlers
- **IMPLEMENT**: `handle_click_command()`, `handle_type_command()`, `handle_key_command()`, `handle_elements_command()`, `handle_wait_command()`
- **MIRROR**: `crates/kild-peek/src/commands.rs:105-174` (screenshot handler pattern)
- **IMPORTS**: `use kild_peek_core::{click, type_text, send_key, list_elements, wait_for_element, ...}`
- **GOTCHA**: Parse coordinates from "x,y" string format; handle permission errors with helpful message
- **VALIDATE**: `cargo build -p kild-peek`

### Task 18: UPDATE `crates/kild-peek/src/table.rs`

- **ACTION**: ADD elements table formatter
- **IMPLEMENT**: `print_elements_table()` for displaying UI elements
- **MIRROR**: Existing table patterns in the file
- **VALIDATE**: `cargo build -p kild-peek`

---

## Testing Strategy

### Unit Tests to Write

| Test File | Test Cases | Validates |
| --------- | ---------- | --------- |
| `permission/handler.rs` | check_accessibility returns status | Permission checking |
| `interact/types.rs` | ClickRequest builder, KeyRequest parsing | Type construction |
| `interact/errors.rs` | error codes, is_user_error | Error handling |
| `elements/types.rs` | UIElement getters, WaitRequest builder | Type construction |
| `elements/errors.rs` | error codes, is_user_error | Error handling |
| `kild-peek/src/app.rs` | CLI arg parsing for new commands | CLI interface |

### Edge Cases Checklist

- [ ] Accessibility permission denied → clear error message with instructions
- [ ] Window not found → helpful error with available windows list
- [ ] Element not found by text/label → timeout with query details
- [ ] Invalid key combination string → specific parsing error
- [ ] Coordinates outside window bounds → warn but allow (some apps have overflow)
- [ ] wait timeout exceeded → specific timeout error with elapsed time
- [ ] Empty elements list → not an error, return empty vec

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test -p kild-peek-core permission && \
cargo test -p kild-peek-core interact && \
cargo test -p kild-peek-core elements && \
cargo test -p kild-peek
```

**EXPECT**: All tests pass

### Level 3: FULL_SUITE

```bash
cargo test --all && cargo build --all
```

**EXPECT**: All tests pass, build succeeds

### Level 4: MANUAL_VALIDATION

1. Run `kild-peek elements --window "Finder"` - should list UI elements
2. Run `kild-peek click --window "Finder" --at 50,50` - should click
3. Run permission denied scenario (revoke access) - should show helpful error

---

## Acceptance Criteria

- [ ] `kild-peek click --window "X" --at 100,50` clicks at coordinates
- [ ] `kild-peek click --window "X" --text "Button"` clicks element by text
- [ ] `kild-peek type --window "X" "hello"` types text
- [ ] `kild-peek key --window "X" "cmd+s"` sends key combination
- [ ] `kild-peek elements --window "X"` lists UI elements
- [ ] `kild-peek elements --window "X" --role button` filters by role
- [ ] `kild-peek wait --window "X" --text "Success"` waits for element
- [ ] Permission denied shows clear instructions for System Settings
- [ ] All validation commands pass with exit 0
- [ ] Code mirrors existing kild-peek patterns exactly

---

## Completion Checklist

- [ ] All tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] Level 1: Static analysis (fmt + clippy) passes
- [ ] Level 2: Unit tests pass
- [ ] Level 3: Full test suite + build succeeds
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
| ---- | ---------- | ------ | ---------- |
| Accessibility permission issues | HIGH | HIGH | Clear error messages with System Settings path; permission check before every operation |
| enigo version incompatibility | LOW | MED | Pin to specific version (0.2); test on macOS before merging |
| Element tree traversal performance | MED | LOW | Add timeout to tree walking; limit depth if needed |
| Focus stealing during tests | MED | MED | Add `--no-focus` flag option for headless scenarios |
| CGEvent thread safety | LOW | HIGH | Run input events on main thread; add thread safety docs |

---

## Notes

**Phased implementation per issue #141:**
- This plan covers **Phases 1-3** (MVP interactions, element finding, smart waiting)
- Phases 4-5 (drag, scroll, hover, tree visualization, fuzzy matching) deferred to follow-up

**Key architectural decisions:**
1. Use `enigo` for input simulation (cross-platform, well-maintained)
2. Use `accessibility` crate for element inspection (Rust-native bindings)
3. Reuse existing `ElementQuery` type from assert module
4. Permission checking as a separate module for reuse

**External resources:**
- [enigo documentation](https://docs.rs/enigo/)
- [accessibility crate](https://docs.rs/accessibility/)
- [Apple Accessibility API](https://developer.apple.com/documentation/applicationservices/axuielement_h)
- [CGEvent Reference](https://developer.apple.com/documentation/coregraphics/cgevent)
