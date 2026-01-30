# Feature: peek Element Finding via Accessibility API (Phase 2)

## Summary

Add macOS Accessibility API-based element enumeration and text-based element finding to kild-peek. This enables three new capabilities: listing all UI elements in a window (`elements`), finding a specific element by text (`find`), and clicking an element by its text content (`click --text`). Uses `accessibility-sys` crate for raw AX FFI bindings with `core-foundation` for memory management.

## User Story

As a developer automating native macOS UI testing
I want to find and interact with UI elements by their text content
So that I can write E2E tests without fragile coordinate-based clicking

## Problem Statement

Phase 1 only supports coordinate-based interactions (`click --at 100,50`). Users must manually determine pixel positions, which breaks when windows resize, layouts change, or DPI differs. Text-based element targeting (`click --text "Submit"`) is resilient to layout changes and self-documenting in test scripts.

## Solution Statement

Add an `element` module to `kild-peek-core` that wraps macOS Accessibility API via `accessibility-sys`. This module provides element tree traversal, text-based search, and position extraction. The existing `click` command is extended with `--text` as an alternative to `--at`. Two new CLI commands (`elements`, `find`) expose the element enumeration directly.

## Metadata

| Field            | Value                                              |
| ---------------- | -------------------------------------------------- |
| Type             | NEW_CAPABILITY                                     |
| Complexity       | HIGH                                               |
| Systems Affected | kild-peek-core (new module), kild-peek (CLI)       |
| Dependencies     | `accessibility-sys = "0.2"`, `core-foundation = "0.10"` |
| Estimated Tasks  | 10                                                 |

---

## UX Design

### Before State

```
User wants to click "Create" button in KILD app:

  1. Run: kild-peek list windows              → find window
  2. Run: kild-peek screenshot --app KILD     → take screenshot
  3. Manually inspect screenshot to find coordinates of "Create" button
  4. Run: kild-peek click --app KILD --at 1650,46

  PAIN: Coordinates are fragile. Window move/resize breaks the test.
  PAIN: No way to discover what elements exist in a window.
  PAIN: Test scripts are opaque - "at 1650,46" says nothing about intent.
```

### After State

```
User wants to click "Create" button in KILD app:

  1. Run: kild-peek elements --app KILD       → see all elements
  2. Run: kild-peek click --app KILD --text "+ Create"

  Or for discovery:
  1. Run: kild-peek find --app KILD --text "Create"  → get element details
  2. Run: kild-peek click --app KILD --text "Create"

  VALUE: Text-based targeting is layout-independent.
  VALUE: Element listing enables discovery without screenshots.
  VALUE: Test scripts are self-documenting.
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| `click` command | Only `--at x,y` | `--at x,y` OR `--text "x"` (mutually exclusive) | Can click by text instead of coordinates |
| New `elements` command | N/A | Lists all UI elements in window | Discover interactive elements |
| New `find` command | N/A | Find element by text, show details | Get element position/role before interacting |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-peek-core/src/interact/handler.rs` | all | Pattern to MIRROR for new handlers, existing click flow to extend |
| P0 | `crates/kild-peek-core/src/interact/types.rs` | all | Request/Result type pattern to FOLLOW |
| P0 | `crates/kild-peek-core/src/interact/errors.rs` | all | Error pattern with PeekError impl to FOLLOW |
| P0 | `crates/kild-peek-core/src/interact/mod.rs` | all | Export pattern to FOLLOW |
| P1 | `crates/kild-peek-core/src/window/types.rs` | all | WindowInfo struct - need PID field |
| P1 | `crates/kild-peek-core/src/window/handler.rs` | 55-194 | Window enumeration - need to add PID capture |
| P1 | `crates/kild-peek-core/src/errors/mod.rs` | all | PeekError trait definition |
| P1 | `crates/kild-peek/src/app.rs` | 168-254 | CLI subcommand definition pattern |
| P1 | `crates/kild-peek/src/commands.rs` | 21-37, 390-485 | Command dispatch and handler pattern |
| P2 | `crates/kild-peek/src/table.rs` | all | Table rendering pattern for elements output |
| P2 | `crates/kild-peek-core/src/lib.rs` | all | Public re-export pattern |
| P2 | `crates/kild-peek-core/src/assert/types.rs` | 1-43 | Existing ElementQuery - may reuse or replace |

---

## Patterns to Mirror

**REQUEST/RESULT_TYPES:**
```rust
// SOURCE: crates/kild-peek-core/src/interact/types.rs:14-26
// COPY THIS PATTERN for ElementsRequest, FindRequest:
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
```

**ERROR_HANDLING:**
```rust
// SOURCE: crates/kild-peek-core/src/interact/errors.rs:3-78
// COPY THIS PATTERN - thiserror + PeekError impl:
#[derive(Debug, thiserror::Error)]
pub enum InteractionError {
    #[error("Window not found: '{title}'")]
    WindowNotFound { title: String },
    // ...
}

impl PeekError for InteractionError {
    fn error_code(&self) -> &'static str {
        match self {
            InteractionError::WindowNotFound { .. } => "INTERACTION_WINDOW_NOT_FOUND",
            // ...
        }
    }
    fn is_user_error(&self) -> bool {
        matches!(self, InteractionError::WindowNotFound { .. } | ...)
    }
}
```

**HANDLER_PATTERN:**
```rust
// SOURCE: crates/kild-peek-core/src/interact/handler.rs:167-230
// COPY THIS PATTERN for list_elements, find_element:
pub fn click(request: &ClickRequest) -> Result<InteractionResult, InteractionError> {
    info!(event = "peek.core.interact.click_started", ...);
    check_accessibility_permission()?;
    let window = resolve_and_focus_window(&request.target)?;
    // ... do work ...
    info!(event = "peek.core.interact.click_completed", ...);
    Ok(InteractionResult::success("click", serde_json::json!({...})))
}
```

**CLI_SUBCOMMAND:**
```rust
// SOURCE: crates/kild-peek/src/app.rs:168-196
// COPY THIS PATTERN for elements and find commands:
.subcommand(
    Command::new("click")
        .about("Click at coordinates within a window")
        .arg(Arg::new("window").long("window").short('w').help("Target window by title"))
        .arg(Arg::new("app").long("app").short('a').help("Target window by app name"))
        .arg(Arg::new("at").long("at").required(true).help("Coordinates: x,y"))
        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue)),
)
```

**CLI_DISPATCH:**
```rust
// SOURCE: crates/kild-peek/src/commands.rs:442-485
// COPY THIS PATTERN for handle_elements_command, handle_find_command:
fn handle_click_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let target = parse_interaction_target(matches)?;
    let json_output = matches.get_flag("json");
    // ... call core handler ...
    match result {
        Ok(result) => { if json_output { println!("{}", serde_json::to_string_pretty(&result)?); } else { /* human output */ } }
        Err(e) => { eprintln!("Failed: {}", e); error!(event = "cli.interact.failed", error = %e); Err(e.into()) }
    }
}
```

**MODULE_EXPORTS:**
```rust
// SOURCE: crates/kild-peek-core/src/interact/mod.rs:1-8
// COPY THIS PATTERN:
mod errors;
mod handler;
mod operations;
mod types;

pub use errors::InteractionError;
pub use handler::{click, send_key_combo, type_text};
pub use types::{ClickRequest, InteractionResult, InteractionTarget, KeyComboRequest, TypeRequest};
```

**TABLE_RENDERING:**
```rust
// SOURCE: crates/kild-peek/src/table.rs:4-97
// FOLLOW THIS PATTERN for print_elements_table
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `Cargo.toml` (workspace) | UPDATE | Add `accessibility-sys` and `core-foundation` workspace deps |
| `crates/kild-peek-core/Cargo.toml` | UPDATE | Add `accessibility-sys` and `core-foundation` deps |
| `crates/kild-peek-core/src/window/types.rs` | UPDATE | Add `pid` field to `WindowInfo` |
| `crates/kild-peek-core/src/window/handler.rs` | UPDATE | Capture PID during window enumeration |
| `crates/kild-peek-core/src/element/mod.rs` | CREATE | New element module exports |
| `crates/kild-peek-core/src/element/types.rs` | CREATE | ElementInfo, ElementsRequest, FindRequest, ElementsResult |
| `crates/kild-peek-core/src/element/errors.rs` | CREATE | ElementError enum with PeekError impl |
| `crates/kild-peek-core/src/element/handler.rs` | CREATE | list_elements, find_element handlers |
| `crates/kild-peek-core/src/element/accessibility.rs` | CREATE | AX API wrapper: tree traversal, attribute access |
| `crates/kild-peek-core/src/lib.rs` | UPDATE | Add `pub mod element` + re-exports |
| `crates/kild-peek-core/src/interact/types.rs` | UPDATE | Add `ClickTextRequest` type |
| `crates/kild-peek-core/src/interact/errors.rs` | UPDATE | Add element-related error variants |
| `crates/kild-peek-core/src/interact/handler.rs` | UPDATE | Add `click_text()` handler |
| `crates/kild-peek-core/src/interact/mod.rs` | UPDATE | Export new types and handler |
| `crates/kild-peek/src/app.rs` | UPDATE | Add `elements`, `find` subcommands; extend `click` with `--text` |
| `crates/kild-peek/src/commands.rs` | UPDATE | Add handlers for new commands |
| `crates/kild-peek/src/table.rs` | UPDATE | Add `print_elements_table()` |

---

## NOT Building (Scope Limits)

- **Accessibility label matching** (`--label "btn-id"`) - Phase 4 scope
- **Wait integration** (`--wait` on elements/find) - Phase 3 scope
- **Element tree visualization** (`elements --tree`) - Phase 5 scope
- **Role-based filtering** (`elements --role button`) - defer; text matching is the MVP
- **Fuzzy/regex text matching** - Phase 5 scope; exact substring matching only
- **Cross-platform support** - macOS only by design
- **OCR-based element finding** - out of scope entirely
- **Double-click/right-click on text** - Phase 4 scope
- **Drag/scroll/hover** - Phase 4 scope

---

## Step-by-Step Tasks

### Task 1: Add workspace dependencies

- **ACTION**: Add `accessibility-sys` and `core-foundation` to workspace Cargo.toml
- **IMPLEMENT**: Add to `[workspace.dependencies]` section
- **FILE**: `Cargo.toml` (workspace root)
- **DETAILS**:
  ```toml
  accessibility-sys = "0.2"
  core-foundation = "0.10"
  ```
- **THEN**: Add to `crates/kild-peek-core/Cargo.toml`:
  ```toml
  accessibility-sys.workspace = true
  core-foundation.workspace = true
  ```
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 2: Add PID to WindowInfo

- **ACTION**: Add `pid` field to `WindowInfo` struct
- **FILE**: `crates/kild-peek-core/src/window/types.rs`
- **IMPLEMENT**:
  - Add `pid: Option<i32>` field to `WindowInfo` struct (Option because PID might not always be available)
  - Add `pid()` getter method returning `Option<i32>`
  - Update `new()` constructor to accept pid parameter
- **FILE**: `crates/kild-peek-core/src/window/handler.rs`
- **IMPLEMENT**:
  - In `list_windows()`, capture PID via `w.pid().ok()` (xcap::Window provides this)
  - In `build_window_info()`, capture PID via `w.pid().ok()`
  - Pass PID to `WindowInfo::new()`
- **GOTCHA**: All existing callers of `WindowInfo::new()` must be updated (search for `WindowInfo::new(` in test code). There are calls in `interact/handler.rs` tests that construct WindowInfo directly - those need the pid parameter too.
- **VALIDATE**: `cargo test --all && cargo clippy --all -- -D warnings`

### Task 3: Create element module - types

- **ACTION**: Create `crates/kild-peek-core/src/element/types.rs`
- **MIRROR**: `crates/kild-peek-core/src/interact/types.rs`
- **IMPLEMENT**:
  ```rust
  /// Information about a UI element discovered via Accessibility API
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ElementInfo {
      pub role: String,           // AXRole: "AXButton", "AXTextField", etc.
      pub title: Option<String>,  // AXTitle
      pub value: Option<String>,  // AXValue (text content)
      pub description: Option<String>, // AXDescription (accessibility label)
      pub x: i32,                 // Window-relative x position
      pub y: i32,                 // Window-relative y position
      pub width: u32,
      pub height: u32,
      pub enabled: bool,
  }

  /// Request to list all elements in a window
  #[derive(Debug, Clone)]
  pub struct ElementsRequest {
      pub target: InteractionTarget,
      pub role_filter: Option<String>,  // Optional role filter (future use)
  }

  /// Request to find a specific element by text
  #[derive(Debug, Clone)]
  pub struct FindRequest {
      pub target: InteractionTarget,
      pub text: String,
  }

  /// Result of listing elements
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ElementsResult {
      pub elements: Vec<ElementInfo>,
      pub window: String,
      pub count: usize,
  }
  ```
- **IMPORTS**: `use super::super::interact::InteractionTarget;` (reuse existing target)
- **TESTS**: Constructor tests, serialization tests for ElementInfo and ElementsResult
- **VALIDATE**: `cargo test -p kild-peek-core`

### Task 4: Create element module - errors

- **ACTION**: Create `crates/kild-peek-core/src/element/errors.rs`
- **MIRROR**: `crates/kild-peek-core/src/interact/errors.rs`
- **IMPLEMENT**:
  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum ElementError {
      #[error("Accessibility permission required: enable in System Settings > Privacy & Security > Accessibility")]
      AccessibilityPermissionDenied,

      #[error("Window not found: '{title}'")]
      WindowNotFound { title: String },

      #[error("Window not found for app: '{app}'")]
      WindowNotFoundByApp { app: String },

      #[error("No element found with text: '{text}'")]
      ElementNotFound { text: String },

      #[error("Multiple elements found with text '{text}': found {count}, expected 1")]
      ElementAmbiguous { text: String, count: usize },

      #[error("Accessibility query failed: {reason}")]
      AccessibilityQueryFailed { reason: String },

      #[error("Window has no PID available (required for Accessibility API)")]
      NoPidAvailable,

      #[error("Window is minimized: '{title}'")]
      WindowMinimized { title: String },

      #[error("Window lookup failed: {reason}")]
      WindowLookupFailed { reason: String },
  }
  ```
- **PeekError impl**: Error codes with `ELEMENT_` prefix (e.g., `ELEMENT_NOT_FOUND`, `ELEMENT_AMBIGUOUS`, `ELEMENT_ACCESSIBILITY_DENIED`, `ELEMENT_QUERY_FAILED`, `ELEMENT_NO_PID`)
- **TESTS**: All error display messages, error codes, is_user_error, Send+Sync
- **VALIDATE**: `cargo test -p kild-peek-core`

### Task 5: Create element module - accessibility wrapper

- **ACTION**: Create `crates/kild-peek-core/src/element/accessibility.rs`
- **IMPLEMENT**: Unsafe FFI wrapper around macOS Accessibility API
- **KEY FUNCTIONS**:
  ```rust
  use accessibility_sys::*;
  use core_foundation::string::CFString;
  use core_foundation::array::CFArray;
  use core_foundation::base::TCFType;

  /// Query all UI elements from an application by PID
  pub fn query_elements(pid: i32) -> Result<Vec<RawElement>, String>

  /// Internal struct holding raw AX attribute values
  pub struct RawElement {
      pub role: String,
      pub title: Option<String>,
      pub value: Option<String>,
      pub description: Option<String>,
      pub position: Option<(f64, f64)>,  // Screen-absolute
      pub size: Option<(f64, f64)>,
      pub enabled: bool,
  }
  ```
- **IMPLEMENTATION DETAILS**:
  1. `AXUIElementCreateApplication(pid)` to get app AX element
  2. Get `kAXWindowsAttribute` to find windows
  3. Recursively traverse `kAXChildrenAttribute` on each window
  4. For each element, read: AXRole, AXTitle, AXValue, AXDescription, AXPosition, AXSize, AXEnabled
  5. AXPosition returns screen-absolute CGPoint via `AXValueGetValue(kAXValueTypeCGPoint)`
  6. AXSize returns CGSize via `AXValueGetValue(kAXValueTypeCGSize)`
  7. Convert screen-absolute position to window-relative in the handler (not here)
- **MEMORY MANAGEMENT**: Use `core_foundation` wrappers (`wrap_under_create_rule`) for auto-release of CF objects returned by Copy functions
- **SAFETY**: All unsafe blocks must have SAFETY comments explaining the invariants
- **GOTCHAS**:
  - Check `AXIsProcessTrusted()` before any AX calls
  - Some elements have no children (leaf nodes) - handle gracefully
  - Some attributes may not exist on all elements - return `None` for missing
  - Use `AXUIElementSetMessagingTimeout` on the app element to prevent hangs (set to 1.0 second)
  - Limit recursion depth to prevent infinite traversal (max ~20 levels)
- **TESTS**: Unit tests for helper functions (string extraction, position parsing). The main `query_elements` function requires accessibility permission so test with `#[ignore]` attribute for CI.
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 6: Create element module - handler

- **ACTION**: Create `crates/kild-peek-core/src/element/handler.rs`
- **MIRROR**: `crates/kild-peek-core/src/interact/handler.rs`
- **IMPLEMENT**:
  ```rust
  /// List all UI elements in a window
  pub fn list_elements(request: &ElementsRequest) -> Result<ElementsResult, ElementError>

  /// Find a specific element by text content
  pub fn find_element(request: &FindRequest) -> Result<ElementInfo, ElementError>
  ```
- **FLOW for list_elements**:
  1. Log `peek.core.element.list_started`
  2. Check accessibility permission (reuse FFI from interact handler)
  3. Find window by target (reuse `find_window_by_target` pattern)
  4. Get PID from WindowInfo (error if None)
  5. Call `accessibility::query_elements(pid)`
  6. Convert `RawElement` → `ElementInfo` (screen-absolute → window-relative coordinates using `element_x - window.x()`)
  7. Log `peek.core.element.list_completed` with count
  8. Return `ElementsResult`
- **FLOW for find_element**:
  1. Log `peek.core.element.find_started`
  2. Call `list_elements` internally
  3. Filter elements where title, value, or description contains the search text (case-insensitive)
  4. If 0 matches → `ElementError::ElementNotFound`
  5. If 1 match → return it
  6. If >1 matches → `ElementError::ElementAmbiguous` (return the first match but warn)
  7. Log `peek.core.element.find_completed`
- **COORDINATE CONVERSION**: `element.x = raw.position.x as i32 - window.x()`, same for y
- **LOGGING**: Follow `peek.core.element.*` event naming
- **TESTS**: Test coordinate conversion logic, text matching logic (using mock data). Handler tests that call AX API use `#[ignore]`.
- **VALIDATE**: `cargo test -p kild-peek-core`

### Task 7: Create element module - mod.rs + wire into lib.rs

- **ACTION**: Create `crates/kild-peek-core/src/element/mod.rs` and update `lib.rs`
- **IMPLEMENT mod.rs**:
  ```rust
  mod accessibility;
  mod errors;
  mod handler;
  mod types;

  pub use errors::ElementError;
  pub use handler::{find_element, list_elements};
  pub use types::{ElementInfo, ElementsRequest, ElementsResult, FindRequest};
  ```
- **UPDATE lib.rs**: Add `pub mod element;` and re-exports:
  ```rust
  pub use element::{ElementError, ElementInfo, ElementsRequest, ElementsResult, FindRequest};
  ```
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 8: Extend click command with text targeting

- **ACTION**: Add `click_text()` handler to interact module
- **FILE**: `crates/kild-peek-core/src/interact/types.rs`
- **IMPLEMENT**: Add `ClickTextRequest` type:
  ```rust
  #[derive(Debug, Clone)]
  pub struct ClickTextRequest {
      pub target: InteractionTarget,
      pub text: String,
  }
  ```
- **FILE**: `crates/kild-peek-core/src/interact/errors.rs`
- **IMPLEMENT**: Add element-related variants:
  ```rust
  #[error("No element found with text: '{text}'")]
  ElementNotFound { text: String },

  #[error("Multiple elements found with text '{text}': found {count}")]
  ElementAmbiguous { text: String, count: usize },

  #[error("Element has no position data")]
  ElementNoPosition,
  ```
- **FILE**: `crates/kild-peek-core/src/interact/handler.rs`
- **IMPLEMENT**: `click_text()` function:
  1. Check accessibility permission
  2. Find window (no focus yet)
  3. Use `element::find_element()` to locate element by text
  4. Get element's window-relative coordinates (center of element: x + width/2, y + height/2)
  5. Focus window
  6. Validate coordinates within bounds
  7. Convert to screen coordinates and post CGEvent click
  8. Return InteractionResult with element info in details
- **FILE**: `crates/kild-peek-core/src/interact/mod.rs`
- **UPDATE**: Export `click_text` and `ClickTextRequest`
- **TESTS**: Test ClickTextRequest construction, new error variants
- **VALIDATE**: `cargo test --all && cargo clippy --all -- -D warnings`

### Task 9: Add CLI commands (elements, find, extend click)

- **FILE**: `crates/kild-peek/src/app.rs`
- **IMPLEMENT**: Add three changes:
  1. **`elements` subcommand**:
     ```
     kild-peek elements --window "KILD"
     kild-peek elements --app KILD
     kild-peek elements --app KILD --json
     ```
     Args: `--window`, `--app`, `--json`
  2. **`find` subcommand**:
     ```
     kild-peek find --window "KILD" --text "Submit"
     kild-peek find --app KILD --text "Create"
     kild-peek find --app KILD --text "Create" --json
     ```
     Args: `--window`, `--app`, `--text` (required), `--json`
  3. **Extend `click` subcommand**: Add `--text` arg, make `--at` and `--text` mutually exclusive (use `conflicts_with`). Remove `required(true)` from `--at` since either `--at` or `--text` is needed.
- **FILE**: `crates/kild-peek/src/commands.rs`
- **IMPLEMENT**:
  1. Add dispatch cases: `Some(("elements", ..))` and `Some(("find", ..))`
  2. `handle_elements_command()`: parse target, call `list_elements()`, output table or JSON
  3. `handle_find_command()`: parse target + text, call `find_element()`, output details or JSON
  4. Update `handle_click_command()`: check for `--text`, if present call `click_text()` instead of `click()`
  5. Add validation: exactly one of `--at` or `--text` must be provided
- **FILE**: `crates/kild-peek/src/table.rs`
- **IMPLEMENT**: `print_elements_table()` following existing table pattern. Columns: Role, Title, Value, Position, Size, Enabled
- **CLI TESTS**: Test new command parsing (elements, find, click with --text, click --at and --text conflict)
- **VALIDATE**: `cargo test --all && cargo clippy --all -- -D warnings`

### Task 10: Update CLAUDE.md

- **ACTION**: Update CLAUDE.md with new commands and module documentation
- **IMPLEMENT**:
  1. Add `elements` module to "Key modules in kild-peek-core" section
  2. Add CLI usage examples for `elements`, `find`, and `click --text` to Build & Development Commands
  3. Add `element` to the Domains list in Structured Logging section
  4. Add `peek.core.element.*` to the Filtering Logs section
- **VALIDATE**: Visual review of CLAUDE.md changes

---

## Testing Strategy

### Unit Tests to Write

| Test File | Test Cases | Validates |
|-----------|-----------|-----------|
| `element/types.rs` | ElementInfo construction, serialization, ElementsResult count | Type correctness |
| `element/errors.rs` | All error variants display, error codes, is_user_error, Send+Sync | Error handling |
| `element/handler.rs` | Coordinate conversion, text matching (case-insensitive, partial), ambiguous matches | Core logic |
| `element/accessibility.rs` | Helper functions (string extraction, position parsing) | FFI wrappers |
| `interact/types.rs` | ClickTextRequest construction | New type |
| `interact/errors.rs` | New element-related variants | Error coverage |
| `kild-peek/app.rs` | elements/find/click --text CLI parsing, --at/--text conflict | CLI correctness |

### Edge Cases Checklist

- [ ] Window with no accessible elements (returns empty list)
- [ ] Element with no text (title, value, description all None) - skipped during text search
- [ ] Element with no position data - included in listing, error when clicked
- [ ] Multiple elements with same text - `find` returns first with warning, `click --text` errors with `ElementAmbiguous`
- [ ] App without accessibility permission - clear error message
- [ ] Window PID unavailable - clear error about PID requirement
- [ ] Very deep element hierarchy (>20 levels) - depth limit prevents infinite recursion
- [ ] Empty search text - validation error
- [ ] Click neither `--at` nor `--text` provided - validation error
- [ ] Case-insensitive text matching: "submit" matches "Submit Button"

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

**EXPECT**: All tests pass (existing + new). New tests should add ~30+ tests.

### Level 3: FULL_SUITE

```bash
cargo test --all && cargo build --all
```

**EXPECT**: All tests pass, full workspace builds cleanly.

### Level 4: MANUAL_VALIDATION

```bash
# Test elements listing
cargo run -p kild-peek -- elements --app Finder
cargo run -p kild-peek -- elements --app Finder --json

# Test find
cargo run -p kild-peek -- find --app Finder --text "File"
cargo run -p kild-peek -- find --app Finder --text "File" --json

# Test click by text
cargo run -p kild-peek -- click --app TextEdit --text "Format"

# Test error cases
cargo run -p kild-peek -- click --app Finder --text "NONEXISTENT_ELEMENT_XYZ"
cargo run -p kild-peek -- click --app Finder --at 100,50 --text "File"  # Should error: conflict
```

---

## Acceptance Criteria

- [ ] `kild-peek elements --app X` lists UI elements with role, title, value, position, size
- [ ] `kild-peek find --app X --text "Y"` finds element and shows its details
- [ ] `kild-peek click --app X --text "Y"` clicks the center of the matching element
- [ ] `--at` and `--text` are mutually exclusive on `click`
- [ ] JSON output works for all new commands (`--json` flag)
- [ ] Clear error message when accessibility permission is missing
- [ ] Clear error message when element text is not found
- [ ] Level 1-3 validation commands pass with exit 0
- [ ] No regressions in existing tests
- [ ] WindowInfo now exposes PID
- [ ] Code follows existing module patterns (errors.rs, types.rs, handler.rs)

---

## Completion Checklist

- [ ] All tasks completed in dependency order (1→2→3→4→5→6→7→8→9→10)
- [ ] Each task validated immediately after completion
- [ ] Level 1: `cargo fmt --check && cargo clippy --all -- -D warnings` passes
- [ ] Level 2: `cargo test --all` passes
- [ ] Level 3: Full test suite + build succeeds
- [ ] Level 4: Manual validation with real macOS apps
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| `accessibility-sys` crate doesn't compile on current Rust edition (2024) | LOW | HIGH | Fall back to manual FFI declarations like existing `AXIsProcessTrusted` pattern |
| Accessibility API returns empty trees for some apps (SwiftUI, Electron) | MEDIUM | MEDIUM | Document limitation; still works for standard Cocoa apps. Future: add OCR fallback in Phase 5 |
| AX tree traversal too slow for complex apps (>500ms) | MEDIUM | LOW | Set `AXUIElementSetMessagingTimeout(1.0s)`, depth limit of 20 levels, log traversal time |
| Memory leaks from CF object mismanagement | LOW | MEDIUM | Use `core_foundation` wrappers exclusively; never manual `CFRelease`. Review all unsafe blocks. |
| PID addition to WindowInfo breaks existing tests | HIGH | LOW | Simple fix: update all `WindowInfo::new()` calls in tests to include `None` for PID |
| Thread safety of AX API calls | LOW | MEDIUM | All kild-peek operations are single-threaded CLI commands; no concern for now |

---

## Notes

- **Why a separate `element` module?** The `interact` module handles CGEvent-based interaction (mouse/keyboard). Element discovery via Accessibility API is a distinct concern with different FFI, error types, and data structures. Keeping them separate follows the existing module-per-domain pattern.
- **Why `accessibility-sys` over high-level `accessibility` crate?** The high-level crate has "spotty" coverage per its maintainer. `accessibility-sys` provides complete raw bindings. This matches the project's existing pattern of using `core-graphics` (raw bindings) rather than higher-level wrappers.
- **Why `Option<i32>` for PID?** The `xcap` crate's `Window::pid()` returns `Result`, and some windows (e.g., system windows) may not have a PID. Using Option keeps the WindowInfo construction infallible.
- **ElementAmbiguous behavior**: `find_element` returns the first match but warns in logs. `click_text` errors on ambiguity because clicking the wrong element is a worse failure mode than not clicking at all. Users can disambiguate with `--app` + `--window` targeting.
- **Coordinate system**: AXPosition returns screen-absolute coordinates. We convert to window-relative (`element_pos - window_pos`) for user-facing output and to match the `click --at` coordinate system. When posting CGEvents, we convert back to screen-absolute.
