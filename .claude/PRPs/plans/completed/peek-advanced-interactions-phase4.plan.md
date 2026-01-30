# Feature: Peek Advanced Interactions (Phase 4)

## Summary

Add advanced mouse interaction capabilities to kild-peek: right-click, double-click, drag-and-drop, scroll, and hover. These extend the existing click infrastructure with new CGEvent types, click-count fields, scroll-wheel events, and mouse-move events. The click command gains `--right` and `--double` flags; three new CLI subcommands (`drag`, `scroll`, `hover`) are added.

## User Story

As a developer using kild-peek for native UI automation
I want to right-click, double-click, drag, scroll, and hover in application windows
So that I can automate complex UI workflows beyond simple left-clicks

## Problem Statement

kild-peek currently supports only left-click (coordinate and text-based), keyboard input, and key combos. Many UI workflows require right-click context menus, double-click to select/open, drag-and-drop for reordering, scroll to reach off-screen content, and hover for tooltips. Without these, E2E automation of native apps is incomplete.

## Solution Statement

Extend the existing interact module with new handler functions, request types, and error variants. Add `--right` and `--double` flags to the existing `click` command. Add `drag`, `scroll`, and `hover` as new CLI subcommands. All new interactions use the same CGEvent dispatch pattern (source creation, event creation, posting to HID) and reuse the existing window targeting, coordinate validation, and focus management infrastructure.

## Metadata

| Field            | Value                                              |
| ---------------- | -------------------------------------------------- |
| Type             | NEW_CAPABILITY                                     |
| Complexity       | MEDIUM                                             |
| Systems Affected | kild-peek-core (interact module), kild-peek (CLI)  |
| Dependencies     | core-graphics 0.24 (needs `highsierra` feature)    |
| Estimated Tasks  | 12                                                 |

---

## UX Design

### Before State

```
kild-peek interaction commands:
  click --at x,y           Left-click at coordinates
  click --text "x"         Left-click element by text
  type "text"              Type text
  key "combo"              Send key combination

LIMITATION: Only left-click available. No right-click,
double-click, drag, scroll, or hover.

User wanting to right-click a Finder item:    NOT POSSIBLE
User wanting to double-click to open a file:  NOT POSSIBLE
User wanting to scroll a list:                NOT POSSIBLE
User wanting to drag an item to reorder:      NOT POSSIBLE
User wanting to trigger a tooltip via hover:  NOT POSSIBLE
```

### After State

```
kild-peek interaction commands:
  click --at x,y                  Left-click at coordinates
  click --at x,y --right          Right-click at coordinates
  click --at x,y --double         Double-click at coordinates
  click --text "x"                Left-click element by text
  click --text "x" --right        Right-click element by text
  click --text "x" --double       Double-click element by text
  drag --from x1,y1 --to x2,y2   Drag from point to point
  scroll --down 5                 Scroll down 5 lines
  scroll --up 3                   Scroll up 3 lines
  scroll --left 2                 Scroll left 2 lines
  scroll --right 4                Scroll right 4 lines
  scroll --at x,y --down 5        Scroll at specific position
  hover --at x,y                  Move mouse to coordinates
  hover --text "x"                Move mouse to element
  type "text"                     Type text (unchanged)
  key "combo"                     Send key combination (unchanged)
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| `click --at` | Left-click only | `--right`, `--double` flags | Can right-click context menus, double-click to open |
| `click --text` | Left-click only | `--right`, `--double` flags | Can right-click/double-click elements by text |
| New `drag` cmd | N/A | Drag from point A to B | Can reorder items, move windows, drag files |
| New `scroll` cmd | N/A | Scroll in any direction | Can reach off-screen content in lists/pages |
| New `hover` cmd | N/A | Move mouse without clicking | Can trigger tooltips, hover states |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-peek-core/src/interact/handler.rs` | 209-272 | `click()` function - pattern to MIRROR for all new interactions |
| P0 | `crates/kild-peek-core/src/interact/handler.rs` | 409-532 | `click_text()` function - pattern for text-based click variants |
| P0 | `crates/kild-peek-core/src/interact/types.rs` | all | Request types pattern to FOLLOW |
| P0 | `crates/kild-peek-core/src/interact/errors.rs` | all | Error pattern to FOLLOW |
| P1 | `crates/kild-peek-core/src/interact/handler.rs` | 1-45 | Imports, constants, permission check |
| P1 | `crates/kild-peek-core/src/interact/handler.rs` | 46-66 | `resolve_and_focus_window()` - reuse this |
| P1 | `crates/kild-peek-core/src/interact/handler.rs` | 181-207 | Coordinate validation and conversion helpers |
| P1 | `crates/kild-peek-core/src/interact/mod.rs` | all | Public exports to update |
| P1 | `crates/kild-peek/src/app.rs` | 247-293 | CLI click command definition - pattern to FOLLOW |
| P1 | `crates/kild-peek/src/commands.rs` | 550-614 | CLI click handler - pattern to FOLLOW |
| P2 | `crates/kild-peek/src/commands.rs` | 394-444 | `parse_interaction_target()` and `parse_coordinates()` - reuse these |
| P2 | `Cargo.toml` | 39 | core-graphics dependency (needs `highsierra` feature for scroll) |

---

## Patterns to Mirror

**CLICK_HANDLER (core pattern for all new interaction handlers):**
```rust
// SOURCE: crates/kild-peek-core/src/interact/handler.rs:209-272
pub fn click(request: &ClickRequest) -> Result<InteractionResult, InteractionError> {
    info!(event = "peek.core.interact.click_started", x = request.x(), y = request.y(), target = ?request.target());
    check_accessibility_permission()?;
    let window = resolve_and_focus_window(request.target(), request.timeout_ms())?;
    validate_coordinates(request.x(), request.y(), &window)?;
    let (screen_x, screen_y) = to_screen_coordinates(request.x(), request.y(), &window);
    let point = CGPoint::new(screen_x, screen_y);
    // ... CGEvent creation and posting ...
    Ok(InteractionResult::success("click", serde_json::json!({...})))
}
```

**REQUEST_TYPE (all request types follow this pattern):**
```rust
// SOURCE: crates/kild-peek-core/src/interact/types.rs:14-53
#[derive(Debug, Clone)]
pub struct ClickRequest {
    target: InteractionTarget,
    x: i32,
    y: i32,
    timeout_ms: Option<u64>,
}
impl ClickRequest {
    pub fn new(target: InteractionTarget, x: i32, y: i32) -> Self { ... }
    pub fn with_wait(mut self, timeout_ms: u64) -> Self { ... }
    pub fn target(&self) -> &InteractionTarget { &self.target }
    // ... getters ...
}
```

**ERROR_HANDLING (error variants with PeekError):**
```rust
// SOURCE: crates/kild-peek-core/src/interact/errors.rs:3-123
#[derive(Debug, thiserror::Error)]
pub enum InteractionError {
    #[error("...")]
    VariantName { field: Type },
}
impl PeekError for InteractionError {
    fn error_code(&self) -> &'static str { match self { ... } }
    fn is_user_error(&self) -> bool { matches!(self, ...) }
}
```

**CLI_COMMAND (clap subcommand definition):**
```rust
// SOURCE: crates/kild-peek/src/app.rs:247-293
.subcommand(
    Command::new("click")
        .about("...")
        .arg(Arg::new("window").long("window").short('w').help("..."))
        .arg(Arg::new("app").long("app").short('a').help("..."))
        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue))
        .arg(Arg::new("wait").long("wait").action(ArgAction::SetTrue))
        .arg(Arg::new("timeout").long("timeout").value_parser(clap::value_parser!(u64)).default_value("30000"))
)
```

**CLI_HANDLER (command handler dispatching to core):**
```rust
// SOURCE: crates/kild-peek/src/commands.rs:550-614
fn handle_click_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let target = parse_interaction_target(matches)?;
    let json_output = matches.get_flag("json");
    let wait_flag = matches.get_flag("wait");
    let timeout_ms = *matches.get_one::<u64>("timeout").unwrap_or(&30000);
    let wait_timeout = wait_flag.then_some(timeout_ms);
    // ... build request, call core function, handle output ...
}
```

**CGEVENT_MOUSE_CLICK (down + delay + up):**
```rust
// SOURCE: crates/kild-peek-core/src/interact/handler.rs:225-254
let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
    .map_err(|()| InteractionError::EventSourceFailed)?;
let mouse_down = CGEvent::new_mouse_event(source.clone(), CGEventType::LeftMouseDown, point, CGMouseButton::Left)
    .map_err(|()| InteractionError::MouseEventFailed { x: screen_x, y: screen_y })?;
let mouse_up = CGEvent::new_mouse_event(source, CGEventType::LeftMouseUp, point, CGMouseButton::Left)
    .map_err(|()| InteractionError::MouseEventFailed { x: screen_x, y: screen_y })?;
mouse_down.post(CGEventTapLocation::HID);
thread::sleep(MOUSE_EVENT_DELAY);
mouse_up.post(CGEventTapLocation::HID);
```

**LOGGING_PATTERN:**
```rust
// SOURCE: crates/kild-peek-core/src/interact/handler.rs:210-215 (core)
info!(event = "peek.core.interact.click_started", x = request.x(), y = request.y(), target = ?request.target());
// SOURCE: crates/kild-peek/src/commands.rs:572-579 (cli)
info!(event = "cli.interact.click_started", x = x, y = y, target = ?target, wait = wait_flag, timeout_ms = timeout_ms);
```

**TEST_STRUCTURE:**
```rust
// SOURCE: crates/kild-peek-core/src/interact/types.rs:190-390
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_click_request_new() { ... }
    #[test]
    fn test_click_request_with_wait() { ... }
}
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `Cargo.toml` | UPDATE | Add `highsierra` feature to core-graphics for scroll events |
| `crates/kild-peek-core/src/interact/types.rs` | UPDATE | Add `ClickModifier` enum, `DragRequest`, `ScrollRequest`, `HoverRequest`, `HoverTextRequest`; add `modifier` field to `ClickRequest`/`ClickTextRequest` |
| `crates/kild-peek-core/src/interact/errors.rs` | UPDATE | Add `ScrollEventFailed`, `DragEventFailed` error variants |
| `crates/kild-peek-core/src/interact/handler.rs` | UPDATE | Add `right_click()`, `double_click()`, `drag()`, `scroll()`, `hover()`, `hover_text()` handlers; refactor click to accept modifier |
| `crates/kild-peek-core/src/interact/mod.rs` | UPDATE | Export new handler functions and types |
| `crates/kild-peek/src/app.rs` | UPDATE | Add `--right`/`--double` flags to click; add `drag`, `scroll`, `hover` subcommands |
| `crates/kild-peek/src/commands.rs` | UPDATE | Add `handle_drag_command`, `handle_scroll_command`, `handle_hover_command`; update click handler for modifiers; route new subcommands |

---

## NOT Building (Scope Limits)

- **`click --label`** (click by accessibility label): Deferred. The Accessibility API returns labels inconsistently across apps. Text-based matching (`--text`) covers most use cases. Can be added later if demand exists.
- **Smooth/interpolated drag** (multiple intermediate drag events): Start with single-step drag (down at A, drag to B, up at B). This handles most drag-and-drop. Multi-step smooth drag can be added if apps don't respond to single-step.
- **`scroll --at` with text targeting**: Scroll at element position. Complex interaction of element finding + scroll. Start with coordinate-based scroll targeting only.
- **Pixel-based scroll**: Only line-based scroll initially (simpler, more predictable). Pixel scroll can be added via `--pixels` flag later.
- **`focus` subcommand** (focus an element): The focus management is already internal to all interactions. Exposing it separately adds complexity without clear use case.
- **`elements --tree`** (hierarchical element view): Belongs to Phase 5 (Polish), not Phase 4.

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `Cargo.toml` - Add `highsierra` feature to core-graphics

- **ACTION**: Add `highsierra` feature to core-graphics dependency for scroll wheel event support
- **IMPLEMENT**: Change `core-graphics = "0.24"` to `core-graphics = { version = "0.24", features = ["highsierra"] }`
- **MIRROR**: Existing workspace dependency patterns at `Cargo.toml:39`
- **GOTCHA**: The `highsierra` feature gates `CGEventCreateScrollWheelEvent2` which is needed for `CGEvent::new_scroll_wheel_event2()`. Without it, scroll events won't compile.
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 2: UPDATE `crates/kild-peek-core/src/interact/types.rs` - Add new request types and modifier enum

- **ACTION**: Add `ClickModifier` enum (None, Right, Double), modify `ClickRequest` and `ClickTextRequest` to include modifier, add `DragRequest`, `ScrollRequest`, `HoverRequest`, `HoverTextRequest`
- **IMPLEMENT**:
  - `ClickModifier` enum: `None`, `Right`, `Double` (derive Debug, Clone, Copy, PartialEq, Eq, Default)
  - Add `modifier: ClickModifier` field to `ClickRequest` (default None via `ClickModifier::default()`)
  - Add `with_modifier(self, modifier: ClickModifier) -> Self` builder method to `ClickRequest`
  - Add `modifier(&self) -> ClickModifier` getter to `ClickRequest`
  - Same modifier field/builder/getter for `ClickTextRequest`
  - `DragRequest`: `target: InteractionTarget`, `from_x: i32`, `from_y: i32`, `to_x: i32`, `to_y: i32`, `timeout_ms: Option<u64>`, with constructor, `with_wait()` builder, getters
  - `ScrollRequest`: `target: InteractionTarget`, `delta_x: i32` (horizontal lines), `delta_y: i32` (vertical lines), `at_x: Option<i32>`, `at_y: Option<i32>` (optional position), `timeout_ms: Option<u64>`, with constructor, builder methods, getters
  - `HoverRequest`: `target: InteractionTarget`, `x: i32`, `y: i32`, `timeout_ms: Option<u64>`, with constructor, `with_wait()`, getters
  - `HoverTextRequest`: `target: InteractionTarget`, `text: String`, `timeout_ms: Option<u64>`, with constructor, `with_wait()`, getters
  - Unit tests for all new types following existing pattern
- **MIRROR**: `types.rs:14-53` (ClickRequest pattern), `types.rs:190-390` (test pattern)
- **GOTCHA**: `ClickModifier::default()` must be `None` so existing callers are unaffected. The modifier is additive - no breaking changes to existing API.
- **VALIDATE**: `cargo build -p kild-peek-core && cargo test -p kild-peek-core`

### Task 3: UPDATE `crates/kild-peek-core/src/interact/errors.rs` - Add new error variants

- **ACTION**: Add error variants for new interaction types
- **IMPLEMENT**:
  - `ScrollEventFailed` - "Failed to create scroll event"
  - `DragEventFailed { from_x: f64, from_y: f64, to_x: f64, to_y: f64 }` - "Failed to create drag event from ({from_x}, {from_y}) to ({to_x}, {to_y})"
  - Add to `error_code()`: `INTERACTION_SCROLL_EVENT_FAILED`, `INTERACTION_DRAG_EVENT_FAILED`
  - Add to `is_user_error()`: neither is user error (system failures)
  - Unit tests for new variants following existing pattern
- **MIRROR**: `errors.rs:3-123` (existing error pattern)
- **VALIDATE**: `cargo build -p kild-peek-core && cargo test -p kild-peek-core`

### Task 4: UPDATE `crates/kild-peek-core/src/interact/handler.rs` - Add import for EventField and new CGEvent types

- **ACTION**: Add `EventField` to the core_graphics imports (needed for double-click `MOUSE_EVENT_CLICK_STATE`), add `CGScrollEventUnit` for scroll events
- **IMPLEMENT**: Update import line 4 to include `EventField` and add `CGScrollEventUnit` import:
  ```rust
  use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton, EventField, CGScrollEventUnit};
  ```
- **MIRROR**: Existing import line at `handler.rs:4`
- **GOTCHA**: `EventField` is the enum that contains `MOUSE_EVENT_CLICK_STATE`. `CGScrollEventUnit` has variants `Line` and `Pixel`.
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 5: UPDATE `crates/kild-peek-core/src/interact/handler.rs` - Refactor `click()` to support modifiers (right-click, double-click)

- **ACTION**: Modify existing `click()` function to use `ClickModifier` from the request to select CGEventType and set click-count field
- **IMPLEMENT**:
  - Read `request.modifier()` to determine behavior
  - For `ClickModifier::None`: existing behavior (LeftMouseDown/Up, no click-state change)
  - For `ClickModifier::Right`: use `CGEventType::RightMouseDown`/`RightMouseUp` with `CGMouseButton::Right`
  - For `ClickModifier::Double`: use LeftMouseDown/Up but call `set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, 2)` on both down and up events
  - Extract mouse event creation into a helper: `fn create_and_post_mouse_click(point: CGPoint, screen_x: f64, screen_y: f64, modifier: ClickModifier) -> Result<(), InteractionError>`
  - Update the `action` field in `InteractionResult` based on modifier: `"click"`, `"right_click"`, `"double_click"`
  - Add modifier info to the JSON details
  - Update logging: include modifier in event fields
  - Update unit tests for coordinate conversion (existing tests unchanged)
  - Add `#[ignore]` integration tests for right-click and double-click
- **MIRROR**: `handler.rs:225-254` (mouse event creation pattern)
- **GOTCHA**: Double-click must set click-state on BOTH mouse_down and mouse_up events. Only setting it on mouse_down will not be recognized by some apps.
- **VALIDATE**: `cargo build -p kild-peek-core && cargo test -p kild-peek-core`

### Task 6: UPDATE `crates/kild-peek-core/src/interact/handler.rs` - Refactor `click_text()` to support modifiers

- **ACTION**: Modify `click_text()` to use `ClickModifier` from `ClickTextRequest`, reusing the same helper from Task 5
- **IMPLEMENT**:
  - Read `request.modifier()` to determine click behavior
  - Use the same `create_and_post_mouse_click()` helper as Task 5
  - Update action and details in result
  - Update logging
- **MIRROR**: `handler.rs:409-532` (click_text pattern)
- **VALIDATE**: `cargo build -p kild-peek-core && cargo test -p kild-peek-core`

### Task 7: UPDATE `crates/kild-peek-core/src/interact/handler.rs` - Add `drag()` handler

- **ACTION**: Add `drag()` function implementing drag-and-drop via CGEvent
- **IMPLEMENT**:
  - Function signature: `pub fn drag(request: &DragRequest) -> Result<InteractionResult, InteractionError>`
  - Log `peek.core.interact.drag_started` with from/to coordinates
  - Check accessibility permission
  - Resolve and focus window
  - Validate BOTH from and to coordinates
  - Convert both coordinate pairs to screen coordinates
  - Create CGEventSource
  - Post sequence: `LeftMouseDown` at from-point, sleep 25ms (`DRAG_EVENT_DELAY` constant), `LeftMouseDragged` at to-point, sleep 25ms, `LeftMouseUp` at to-point
  - Add `DRAG_EVENT_DELAY: Duration = Duration::from_millis(25)` constant
  - Return `InteractionResult::success("drag", ...)` with from/to coordinates, screen coordinates, window
  - Log `peek.core.interact.drag_completed`
- **MIRROR**: `handler.rs:209-272` (click handler structure)
- **GOTCHA**: Must use `CGEventType::LeftMouseDragged` (not `MouseMoved`) between down and up. Using MouseMoved won't trigger drag-and-drop in apps.
- **VALIDATE**: `cargo build -p kild-peek-core && cargo test -p kild-peek-core`

### Task 8: UPDATE `crates/kild-peek-core/src/interact/handler.rs` - Add `scroll()` handler

- **ACTION**: Add `scroll()` function implementing scroll wheel events
- **IMPLEMENT**:
  - Function signature: `pub fn scroll(request: &ScrollRequest) -> Result<InteractionResult, InteractionError>`
  - Log `peek.core.interact.scroll_started` with delta_x, delta_y
  - Check accessibility permission
  - Resolve and focus window
  - If `at_x`/`at_y` specified, validate those coordinates and move mouse there first via `CGEventType::MouseMoved` event (so scroll targets the right position)
  - Create CGEventSource
  - Determine wheel_count: 1 if delta_x is 0, 2 if both axes have values
  - Use `CGEvent::new_scroll_wheel_event2(Some(&source), CGScrollEventUnit::Line, wheel_count, delta_y, delta_x, 0)` - note: delta_y is wheel1 (vertical), delta_x is wheel2 (horizontal)
  - Map error to `InteractionError::ScrollEventFailed`
  - Post to `CGEventTapLocation::HID`
  - Return `InteractionResult::success("scroll", ...)` with delta and position info
  - Log `peek.core.interact.scroll_completed`
- **MIRROR**: `handler.rs:209-272` (handler structure)
- **GOTCHA**: `new_scroll_wheel_event2` takes `Option<&CGEventSource>` (Some(&source)), not owned source. The vertical delta (delta_y) is wheel1, horizontal (delta_x) is wheel2. Negative = up/left, positive = down/right.
- **VALIDATE**: `cargo build -p kild-peek-core && cargo test -p kild-peek-core`

### Task 9: UPDATE `crates/kild-peek-core/src/interact/handler.rs` - Add `hover()` and `hover_text()` handlers

- **ACTION**: Add `hover()` and `hover_text()` functions for mouse movement without clicking
- **IMPLEMENT**:
  - `hover()`:
    - Function signature: `pub fn hover(request: &HoverRequest) -> Result<InteractionResult, InteractionError>`
    - Log `peek.core.interact.hover_started`
    - Check accessibility, resolve/focus window, validate coordinates, convert to screen
    - Create `CGEventType::MouseMoved` event at target point
    - Post to HID
    - Return `InteractionResult::success("hover", ...)`
    - Log `peek.core.interact.hover_completed`
  - `hover_text()`:
    - Function signature: `pub fn hover_text(request: &HoverTextRequest) -> Result<InteractionResult, InteractionError>`
    - Same pattern as `click_text()` but posts `MouseMoved` instead of mouse down/up
    - Find element by text, compute center, focus window, move mouse to center
    - Return `InteractionResult::success("hover", ...)` with element details
- **MIRROR**: `handler.rs:209-272` (hover) and `handler.rs:409-532` (hover_text)
- **GOTCHA**: `CGMouseButton::Left` parameter in `new_mouse_event` is ignored for `MouseMoved` events, but must still be provided.
- **VALIDATE**: `cargo build -p kild-peek-core && cargo test -p kild-peek-core`

### Task 10: UPDATE `crates/kild-peek-core/src/interact/mod.rs` - Export new functions and types

- **ACTION**: Add new handler functions and types to public exports
- **IMPLEMENT**:
  - Add to handler exports: `drag`, `hover`, `hover_text`, `scroll`
  - Add to type exports: `ClickModifier`, `DragRequest`, `HoverRequest`, `HoverTextRequest`, `ScrollRequest`
- **MIRROR**: `mod.rs:7-11` (existing exports)
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 11: UPDATE `crates/kild-peek/src/app.rs` - Add CLI flags and subcommands

- **ACTION**: Add `--right` and `--double` flags to click command; add `drag`, `scroll`, `hover` subcommands
- **IMPLEMENT**:
  - **Click modifications**: Add `--right` (ArgAction::SetTrue, conflicts with `double`) and `--double` (ArgAction::SetTrue, conflicts with `right`) flags
  - **Drag subcommand**:
    - `--window`/`--app` (standard window targeting)
    - `--from` (required, "x,y" format)
    - `--to` (required, "x,y" format)
    - `--json`, `--wait`, `--timeout` (standard flags)
  - **Scroll subcommand**:
    - `--window`/`--app` (standard window targeting)
    - `--up` (i32, lines to scroll up)
    - `--down` (i32, lines to scroll down)
    - `--left` (i32, lines to scroll left)
    - `--right` (i32, lines to scroll right)
    - `--at` (optional, "x,y" for scroll position)
    - `--json`, `--wait`, `--timeout`
    - Note: `--up` conflicts with `--down`, `--left` conflicts with `--right`. At least one direction required.
  - **Hover subcommand**:
    - `--window`/`--app` (standard window targeting)
    - `--at` (coordinates, conflicts with `--text`)
    - `--text` (element text, conflicts with `--at`)
    - `--json`, `--wait`, `--timeout`
  - Add CLI tests for all new flags and subcommands following the existing test pattern
- **MIRROR**: `app.rs:247-293` (click subcommand pattern)
- **GOTCHA**: Scroll direction args are mutually exclusive on each axis (--up vs --down, --left vs --right). Use `conflicts_with` in clap.
- **VALIDATE**: `cargo build -p kild-peek && cargo test -p kild-peek`

### Task 12: UPDATE `crates/kild-peek/src/commands.rs` - Add command handlers and routing

- **ACTION**: Update click handler for modifiers; add `handle_drag_command`, `handle_scroll_command`, `handle_hover_command`; route new subcommands in dispatch
- **IMPLEMENT**:
  - **Click handler update**: Read `--right`/`--double` flags, map to `ClickModifier`, pass to `ClickRequest` via `.with_modifier()`. Update both coordinate and text click paths. Update human-readable output to say "Right-clicked" or "Double-clicked".
  - **handle_drag_command**: Parse `--from` and `--to` coordinates using `parse_coordinates()`, build `DragRequest`, call `drag()`, handle output
  - **handle_scroll_command**: Parse direction flags, compute `delta_x`/`delta_y` (down/right positive, up/left negative), optionally parse `--at` coordinates, build `ScrollRequest`, call `scroll()`, handle output
  - **handle_hover_command**: Parse `--at` or `--text`, dispatch to `hover()` or `hover_text()`, handle output (similar to click handler structure)
  - **Routing**: Add `"drag" | "scroll" | "hover"` arms to the subcommand match in `run_command()`
  - Logging: Follow existing cli.interact.{action}_started/completed/failed pattern
- **MIRROR**: `commands.rs:550-614` (click handler), `commands.rs:32-34` (subcommand routing)
- **VALIDATE**: `cargo build --all && cargo test --all && cargo clippy --all -- -D warnings && cargo fmt --check`

---

## Testing Strategy

### Unit Tests to Write

| Test File | Test Cases | Validates |
|-----------|------------|-----------|
| `interact/types.rs` | `ClickModifier` default/variants, `DragRequest` new/with_wait/getters, `ScrollRequest` new/with_at/getters, `HoverRequest`/`HoverTextRequest` new/with_wait/getters, modifier on ClickRequest/ClickTextRequest | New type definitions |
| `interact/errors.rs` | `ScrollEventFailed` display/code/is_user_error, `DragEventFailed` display/code/is_user_error | New error variants |
| `interact/handler.rs` | Existing coordinate/validation tests unchanged; new `#[ignore]` integration tests for right_click, double_click, drag, scroll, hover | Handler logic |
| `app.rs` | CLI parsing for `--right`/`--double` on click, `drag` args, `scroll` args, `hover` args, conflict validation | CLI definition |

### Edge Cases Checklist

- [ ] `click --right --double` conflicts (should error)
- [ ] `scroll` with no direction flags (should error)
- [ ] `scroll --up` and `--down` together (should error)
- [ ] `scroll --left` and `--right` together (should error)
- [ ] `drag --from` and `--to` same coordinates (should succeed, no-op drag)
- [ ] `hover --at` and `--text` together (should error)
- [ ] `hover` with neither `--at` nor `--text` (should error)
- [ ] Drag with from-coordinates out of bounds (should error)
- [ ] Drag with to-coordinates out of bounds (should error)
- [ ] Scroll with 0 delta (allowed but no-op)
- [ ] Double-click on text element (modifier + text-based click)
- [ ] Right-click on text element (modifier + text-based click)

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test --all
```

**EXPECT**: All tests pass

### Level 3: FULL_SUITE

```bash
cargo test --all && cargo build --all
```

**EXPECT**: All tests pass, build succeeds

### Level 6: MANUAL_VALIDATION

1. Open Finder, then run: `cargo run -p kild-peek -- click --app Finder --at 100,50 --right` (should show context menu)
2. Run: `cargo run -p kild-peek -- click --app Finder --at 100,50 --double` (should open item)
3. Run: `cargo run -p kild-peek -- scroll --app Finder --down 5` (should scroll down)
4. Run: `cargo run -p kild-peek -- hover --app Finder --at 100,50` (should move cursor)
5. Run: `cargo run -p kild-peek -- drag --app Finder --from 100,100 --to 300,100` (should drag)
6. Verify `--json` output for each new command
7. Verify `--wait` works for each new command

---

## Acceptance Criteria

- [ ] `click --right` sends right-click events (RightMouseDown/Up with CGMouseButton::Right)
- [ ] `click --double` sends double-click events (click-state=2 on both down and up)
- [ ] `click --text "X" --right` right-clicks element found by text
- [ ] `click --text "X" --double` double-clicks element found by text
- [ ] `drag --from x1,y1 --to x2,y2` performs drag-and-drop sequence
- [ ] `scroll --down N` scrolls N lines down in target window
- [ ] `scroll --up N` scrolls N lines up
- [ ] `scroll --left N` / `scroll --right N` scroll horizontally
- [ ] `scroll --at x,y --down N` scrolls at specific position
- [ ] `hover --at x,y` moves mouse without clicking
- [ ] `hover --text "X"` moves mouse to element center without clicking
- [ ] All commands support `--json`, `--wait`, `--timeout` flags
- [ ] All commands support `--window`/`--app` targeting
- [ ] Level 1-3 validation commands pass with exit 0
- [ ] No regressions in existing tests

---

## Completion Checklist

- [ ] All 12 tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] Level 1: Static analysis (fmt + clippy) passes
- [ ] Level 2: Unit tests pass
- [ ] Level 3: Full test suite + build succeeds
- [ ] All acceptance criteria met
- [ ] CLAUDE.md updated with new commands in Build & Development Commands section

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| `highsierra` feature breaks build or conflicts with gpui's core-graphics pin | LOW | HIGH | Test build with feature flag first (Task 1). The feature only adds scroll event FFI, shouldn't conflict with existing APIs. |
| `EventField::MOUSE_EVENT_CLICK_STATE` not available in core-graphics 0.24 | LOW | MEDIUM | The field exists in core-graphics 0.24 source. If missing, use raw field ID (1) with `set_integer_value_field()`. |
| Double-click timing not recognized by some apps | LOW | LOW | Set click-state on both down and up events. 10ms delay is within system double-click interval. |
| Drag not recognized by some apps (single-step drag) | MEDIUM | LOW | Use LeftMouseDragged event type (not MouseMoved). 25ms delays between events. If apps need smooth drag, can add multi-step in future. |
| Scroll direction API confusion (negative vs positive) | LOW | LOW | Document clearly: positive delta_y = scroll down, negative = up. Test manually. |

---

## Notes

- The `click --label` feature (click by accessibility label) is explicitly deferred. The issue mentions it in Phase 4, but accessibility labels are inconsistently supported across apps. Text matching (`--text`) handles the primary use case.
- `EventField` enum in core-graphics 0.24 uses the naming `MOUSE_EVENT_CLICK_STATE` which corresponds to macOS `kCGMouseEventClickState` (field ID 1).
- The `new_scroll_wheel_event2` function signature takes `Option<&CGEventSource>` - pass `Some(&source)` not just `source`.
- All new interactions require Accessibility permissions, same as existing click/type/key.
- Timing constants: 10ms for mouse click delay (existing), 25ms for drag event delay (new, based on research from clickdrag and enigo libraries), 50ms for focus settle (existing).
