# Feature: peek native UI interaction — Phase 1 (Basic Interactions)

## Summary

Add three new CLI commands to kild-peek (`click`, `type`, `key`) that use macOS CGEvent APIs to perform coordinate-based mouse clicks, text input, and keyboard combos on targeted windows. This enables automated interaction with native macOS applications from the command line, complementing the existing observation capabilities (screenshot, assert, diff).

## User Story

As a developer automating native macOS app testing
I want to click, type text, and send key combinations to application windows
So that I can build E2E test scripts for native apps without manual interaction

## Problem Statement

kild-peek can observe native UIs (screenshot, diff, assert) but cannot interact with them. There is no way to automate clicking buttons, typing into fields, or sending keyboard shortcuts to native macOS applications from the CLI.

## Solution Statement

Add an `interact` module to kild-peek-core that uses macOS `core-graphics` CGEvent APIs to post synthetic mouse and keyboard events. Expose three new CLI subcommands (`click`, `type`, `key`) that resolve a target window, convert window-relative coordinates to screen-absolute coordinates, and dispatch events. Include accessibility permission detection with clear error messages.

## Metadata

| Field            | Value                                           |
| ---------------- | ----------------------------------------------- |
| Type             | NEW_CAPABILITY                                  |
| Complexity       | MEDIUM                                          |
| Systems Affected | kild-peek-core, kild-peek                       |
| Dependencies     | core-graphics 0.24.0 (already in lockfile)      |
| Estimated Tasks  | 8                                               |
| GitHub Issue     | #141 (Phase 1)                                  |

---

## UX Design

### Before State

```
┌──────────────────────────────────────────────────────┐
│                   kild-peek CLI                      │
│                                                      │
│  Observation Only:                                   │
│  ┌──────────┐ ┌──────────┐ ┌──────┐ ┌────────┐     │
│  │   list   │ │screenshot│ │ diff │ │ assert │     │
│  └──────────┘ └──────────┘ └──────┘ └────────┘     │
│                                                      │
│  User Flow:                                          │
│  1. Take screenshot → see what's on screen           │
│  2. Assert state → verify something exists           │
│  3. ???  → no way to click/type/interact             │
│  4. Take screenshot → check nothing changed          │
│                                                      │
│  PAIN: Cannot automate UI interaction in scripts     │
└──────────────────────────────────────────────────────┘
```

### After State

```
┌──────────────────────────────────────────────────────┐
│                   kild-peek CLI                      │
│                                                      │
│  Observation:                                        │
│  ┌──────────┐ ┌──────────┐ ┌──────┐ ┌────────┐     │
│  │   list   │ │screenshot│ │ diff │ │ assert │     │
│  └──────────┘ └──────────┘ └──────┘ └────────┘     │
│                                                      │
│  Interaction (NEW):                                  │
│  ┌──────────┐ ┌──────────┐ ┌──────┐                 │
│  │  click   │ │   type   │ │ key  │                 │
│  └──────────┘ └──────────┘ └──────┘                 │
│                                                      │
│  User Flow:                                          │
│  1. Assert window exists                             │
│  2. Click at coordinates (100,50) in window          │
│  3. Type "my-branch" into focused field              │
│  4. Send "enter" key to submit                       │
│  5. Screenshot → verify result                       │
└──────────────────────────────────────────────────────┘
```

### Interaction Changes

| Command | Before | After | User Impact |
|---------|--------|-------|-------------|
| `kild-peek click --window "KILD" --at 100,50` | N/A | Clicks at (100,50) relative to window | Can click buttons/elements |
| `kild-peek type --window "KILD" "text"` | N/A | Types text into focused element | Can fill form fields |
| `kild-peek key --window "KILD" "cmd+s"` | N/A | Sends Cmd+S to window | Can trigger shortcuts |
| `kild-peek key --window "KILD" "enter"` | N/A | Sends Enter key | Can submit forms |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-peek-core/src/window/errors.rs` | all | Error pattern to MIRROR exactly |
| P0 | `crates/kild-peek-core/src/errors/mod.rs` | all | PeekError trait to IMPLEMENT |
| P0 | `crates/kild-peek-core/src/screenshot/handler.rs` | 1-60 | Handler pattern to MIRROR |
| P0 | `crates/kild-peek-core/src/window/types.rs` | all | WindowInfo — need x,y,width,height for coord conversion |
| P1 | `crates/kild-peek/src/app.rs` | all | CLI subcommand pattern to MIRROR |
| P1 | `crates/kild-peek/src/commands.rs` | all | Command dispatch pattern to MIRROR |
| P1 | `crates/kild-peek-core/src/lib.rs` | all | Re-export pattern to FOLLOW |
| P2 | `crates/kild-peek-core/src/screenshot/types.rs` | 1-50 | Builder pattern (request types) |
| P2 | `crates/kild-peek-core/src/assert/types.rs` | all | Result type pattern |
| P2 | `Cargo.toml` | all | Workspace dependency management |

---

## Patterns to Mirror

**ERROR_TYPE:**
```rust
// SOURCE: crates/kild-peek-core/src/window/errors.rs:3-64
#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("Failed to enumerate windows: {message}")]
    EnumerationFailed { message: String },
    // ...
}

impl PeekError for WindowError {
    fn error_code(&self) -> &'static str {
        match self { /* each variant → SCREAMING_CASE string */ }
    }
    fn is_user_error(&self) -> bool {
        matches!(self, /* user-actionable variants */)
    }
}
```

**HANDLER:**
```rust
// SOURCE: crates/kild-peek-core/src/screenshot/handler.rs:16-31
pub fn capture(request: &CaptureRequest) -> Result<CaptureResult, ScreenshotError> {
    info!(event = "core.screenshot.capture_started", target = ?request.target);
    // ... business logic ...
    // Returns typed Result, logs started/completed/failed
}
```

**CLI_SUBCOMMAND:**
```rust
// SOURCE: crates/kild-peek/src/app.rs:54-131
Command::new("screenshot")
    .about("Capture a screenshot")
    .arg(Arg::new("window").long("window").short('w').help("..."))
    // ... more args ...
```

**CLI_DISPATCH:**
```rust
// SOURCE: crates/kild-peek/src/commands.rs:18-31
match matches.subcommand() {
    Some(("screenshot", sub_matches)) => handle_screenshot_command(sub_matches),
    // ...
}
```

**LOGGING:**
```rust
// SOURCE: crates/kild-peek/src/commands.rs:48-73
info!(event = "cli.screenshot_started", window_title = ?window_title, ...);
// ... do work ...
info!(event = "cli.screenshot_completed", width = result.width(), ...);
error!(event = "cli.screenshot_failed", error = %e);
```

**RE-EXPORTS:**
```rust
// SOURCE: crates/kild-peek-core/src/lib.rs:12-34
pub mod interact;
pub use interact::{ClickRequest, TypeRequest, KeyComboRequest, InteractionResult};
```

---

## Files to Change

| File | Action | Justification |
| ---- | ------ | ------------- |
| `Cargo.toml` | UPDATE | Add `core-graphics = "0.24"` to workspace deps |
| `crates/kild-peek-core/Cargo.toml` | UPDATE | Add `core-graphics` workspace dep |
| `crates/kild-peek-core/src/interact/mod.rs` | CREATE | Module exports |
| `crates/kild-peek-core/src/interact/errors.rs` | CREATE | InteractionError enum |
| `crates/kild-peek-core/src/interact/types.rs` | CREATE | ClickRequest, TypeRequest, KeyComboRequest, InteractionResult |
| `crates/kild-peek-core/src/interact/handler.rs` | CREATE | click(), type_text(), send_key_combo() |
| `crates/kild-peek-core/src/interact/keymap.rs` | CREATE | Key name → virtual keycode + modifier flags mapping |
| `crates/kild-peek-core/src/lib.rs` | UPDATE | Add `pub mod interact` + re-exports |
| `crates/kild-peek/src/app.rs` | UPDATE | Add click, type, key subcommands |
| `crates/kild-peek/src/commands.rs` | UPDATE | Add handle_click, handle_type, handle_key dispatch |

---

## NOT Building (Scope Limits)

- **Element-based clicking** (`--text "Submit"`, `--label "btn"`) — Phase 2, requires Accessibility API element finding
- **Wait integration on interaction commands** (`--wait` on click/type/key) — Phase 3
- **Right-click, double-click** (`--right`, `--double`) — Phase 4
- **Drag, scroll, hover** — Phase 4
- **Element tree inspection** (`elements`, `find` subcommands) — Phase 2
- **Cross-platform support** — macOS only, explicit in project scope
- **`post_to_pid` targeting** — requires `elcapitan` feature; we use window focus + global event posting instead
- **OCR-based element finding** — out of scope entirely

---

## Step-by-Step Tasks

### Task 1: ADD `core-graphics` workspace dependency

- **ACTION**: Update workspace Cargo.toml and kild-peek-core Cargo.toml
- **IMPLEMENT**:
  - Add `core-graphics = "0.24"` to `[workspace.dependencies]` in root `Cargo.toml`
  - Add `core-graphics.workspace = true` to kild-peek-core's `[dependencies]`
- **GOTCHA**: Version 0.24.0 is already in the lockfile (used by gpui's font-kit chain). Using the same version avoids adding another copy. Do NOT use 0.25.0 — it conflicts with the `core-text = "=21.0.0"` pin documented in workspace Cargo.toml.
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 2: CREATE `interact/errors.rs` — InteractionError

- **ACTION**: Create error type for all interaction failures
- **IMPLEMENT**:
  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum InteractionError {
      #[error("Accessibility permission required: enable in System Settings > Privacy & Security > Accessibility")]
      AccessibilityPermissionDenied,

      #[error("Window not found: '{title}'")]
      WindowNotFound { title: String },

      #[error("Window not found for app: '{app}'")]
      WindowNotFoundByApp { app: String },

      #[error("Failed to create event source")]
      EventSourceFailed,

      #[error("Failed to create mouse event at ({x}, {y})")]
      MouseEventFailed { x: f64, y: f64 },

      #[error("Failed to create keyboard event for keycode {keycode}")]
      KeyboardEventFailed { keycode: u16 },

      #[error("Unknown key name: '{name}'")]
      UnknownKey { name: String },

      #[error("Invalid coordinate: ({x}, {y}) is outside window bounds ({width}x{height})")]
      CoordinateOutOfBounds { x: i32, y: i32, width: u32, height: u32 },

      #[error("Window is minimized: '{title}'")]
      WindowMinimized { title: String },
  }
  ```
- **MIRROR**: `crates/kild-peek-core/src/window/errors.rs:3-64`
- **PATTERN**: Implement `PeekError` with error codes like `INTERACTION_ACCESSIBILITY_DENIED`, `INTERACTION_WINDOW_NOT_FOUND`, etc. Mark user errors: AccessibilityPermissionDenied, WindowNotFound, WindowNotFoundByApp, UnknownKey, CoordinateOutOfBounds, WindowMinimized.
- **TESTS**: Error display messages, error codes, is_user_error, Send+Sync bounds
- **VALIDATE**: `cargo test -p kild-peek-core`

### Task 3: CREATE `interact/keymap.rs` — Key name mapping

- **ACTION**: Create mapping from human-readable key names to macOS virtual keycodes and modifier flags
- **IMPLEMENT**:
  - Struct `KeyMapping { keycode: u16, flags: u64 }` (flags = 0 for regular keys)
  - Function `parse_key_combo(combo: &str) -> Result<Vec<KeyMapping>, InteractionError>` that parses strings like `"cmd+s"`, `"enter"`, `"cmd+shift+p"`, `"tab"`, `"escape"`
  - Modifier detection: `cmd`/`command` → CGEventFlagCommand (0x00100000), `shift` → CGEventFlagShift (0x00020000), `ctrl`/`control` → CGEventFlagControl (0x00040000), `opt`/`option`/`alt` → CGEventFlagAlternate (0x00080000)
  - Common keycodes: return=36, tab=48, space=49, delete=51, escape=53, a=0, s=1, d=2, f=3, etc.
  - Handle single keys like `"enter"`, `"tab"`, `"escape"`, `"space"`, `"delete"`, `"up"`, `"down"`, `"left"`, `"right"`, `"f1"`-`"f12"`
  - Handle combos like `"cmd+s"` → keycode=1 with command flag
  - Case-insensitive parsing
- **TESTS**: Parse common combos (cmd+s, cmd+shift+p, enter, tab, escape, ctrl+c, alt+tab), reject unknown keys, case insensitivity
- **VALIDATE**: `cargo test -p kild-peek-core`

### Task 4: CREATE `interact/types.rs` — Request and result types

- **ACTION**: Create typed request/result structs for each interaction
- **IMPLEMENT**:
  ```rust
  /// Target window for interaction
  #[derive(Debug, Clone)]
  pub enum InteractionTarget {
      Window { title: String },
      App { app: String },
      AppAndWindow { app: String, title: String },
  }

  /// Request to click at coordinates within a window
  #[derive(Debug, Clone)]
  pub struct ClickRequest {
      pub target: InteractionTarget,
      pub x: i32,
      pub y: i32,
  }

  /// Request to type text into the focused element
  #[derive(Debug, Clone)]
  pub struct TypeRequest {
      pub target: InteractionTarget,
      pub text: String,
  }

  /// Request to send a key combination
  #[derive(Debug, Clone)]
  pub struct KeyComboRequest {
      pub target: InteractionTarget,
      pub combo: String,  // e.g., "cmd+s", "enter"
  }

  /// Result of an interaction operation
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct InteractionResult {
      pub success: bool,
      pub action: String,
      #[serde(skip_serializing_if = "Option::is_none")]
      pub details: Option<serde_json::Value>,
  }
  ```
- **MIRROR**: `crates/kild-peek-core/src/screenshot/types.rs` (builder pattern), `crates/kild-peek-core/src/assert/types.rs` (result pattern)
- **PATTERN**: Builders on ClickRequest/TypeRequest/KeyComboRequest. InteractionResult has `success(action, details)` and `from_action(action)` constructors.
- **TESTS**: Builder construction, result construction, serialization
- **VALIDATE**: `cargo test -p kild-peek-core`

### Task 5: CREATE `interact/handler.rs` — Core interaction logic

- **ACTION**: Implement the three interaction handlers using CGEvent APIs
- **IMPLEMENT**:
  - **Accessibility check**: Use `core_graphics::access::screen_capture_access()` or call `AXIsProcessTrusted` via raw FFI to check before any operation. Return `InteractionError::AccessibilityPermissionDenied` with clear instructions.
  - **`click(request: &ClickRequest) -> Result<InteractionResult, InteractionError>`**:
    1. Log `info!(event = "core.interact.click_started", x = request.x, y = request.y)`
    2. Find target window via existing `find_window_by_title` / `find_window_by_app` / `find_window_by_app_and_title`
    3. Validate coordinates are within window bounds (0..width, 0..height)
    4. Convert window-relative to screen-absolute: `screen_x = window.x() + request.x`, `screen_y = window.y() + request.y`
    5. Focus the window (bring to front) — use AppleScript via `std::process::Command`: `osascript -e 'tell application "System Events" to set frontmost of process "AppName" to true'`
    6. Brief sleep (50ms) for focus to settle
    7. Create CGEventSource (HIDSystemState)
    8. Post LeftMouseDown then LeftMouseUp at screen coordinates, with 10ms delay between
    9. Log `info!(event = "core.interact.click_completed", screen_x, screen_y)`
    10. Return InteractionResult with details (screen coords, window)
  - **`type_text(request: &TypeRequest) -> Result<InteractionResult, InteractionError>`**:
    1. Log `info!(event = "core.interact.type_started", text_len = request.text.len())`
    2. Find and focus target window (same as click)
    3. Create CGEventSource
    4. Create keyboard event with keycode 0, keydown=true
    5. Call `event.set_string(&request.text)` to type entire string
    6. Post event at HID
    7. Log completed
  - **`send_key_combo(request: &KeyComboRequest) -> Result<InteractionResult, InteractionError>`**:
    1. Log `info!(event = "core.interact.key_started", combo = &request.combo)`
    2. Find and focus target window
    3. Parse combo via `keymap::parse_key_combo(&request.combo)`
    4. Create CGEventSource
    5. For each key in the parsed combo: create key-down event, set modifier flags, post at HID, sleep 10ms, then key-up event
    6. Log completed
  - **Helper: `resolve_and_focus_window`**: shared logic to find window, check not minimized, focus via AppleScript, return WindowInfo. Map WindowError variants to InteractionError variants.
  - **Helper: `check_accessibility_permission`**: FFI call to `AXIsProcessTrusted()` from ApplicationServices framework. Simple extern "C" declaration — no extra crate needed.
- **GOTCHA**: CGEvent uses global screen coordinates (origin top-left of main display). Window-relative conversion uses WindowInfo's x() and y(). CGEvent::new_mouse_event and new_keyboard_event return `Result<CGEvent, ()>` — map the `()` to InteractionError.
- **GOTCHA**: `core-graphics` 0.24.0 CGEventSource::new signature uses `CGEventSourceStateID` enum. The `event.set_string()` method types the full string in one event — efficient for text input.
- **GOTCHA**: For accessibility check, declare raw FFI: `extern "C" { fn AXIsProcessTrusted() -> bool; }` with `#[link(name = "ApplicationServices", kind = "framework")]`. No extra crate needed.
- **TESTS**: Unit tests for coordinate conversion logic, keymap integration, error mapping. Handler functions require real windows, so keep integration tests minimal.
- **VALIDATE**: `cargo test -p kild-peek-core && cargo clippy -p kild-peek-core -- -D warnings`

### Task 6: CREATE `interact/mod.rs` — Module exports

- **ACTION**: Create module file exposing public API
- **IMPLEMENT**:
  ```rust
  mod errors;
  mod handler;
  mod keymap;
  mod types;

  pub use errors::InteractionError;
  pub use handler::{click, type_text, send_key_combo};
  pub use types::{ClickRequest, TypeRequest, KeyComboRequest, InteractionResult, InteractionTarget};
  ```
- **MIRROR**: `crates/kild-peek-core/src/screenshot/mod.rs` pattern
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 7: UPDATE `lib.rs` — Add interact module and re-exports

- **ACTION**: Register interact module in kild-peek-core's public API
- **IMPLEMENT**:
  - Add `pub mod interact;` to module declarations
  - Add re-exports: `pub use interact::{ClickRequest, TypeRequest, KeyComboRequest, InteractionResult, InteractionTarget};`
- **MIRROR**: `crates/kild-peek-core/src/lib.rs:12-34`
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 8: UPDATE CLI — Add click, type, key subcommands

- **ACTION**: Add three new subcommands to kild-peek CLI
- **IMPLEMENT in `app.rs`**:
  ```rust
  // Click subcommand
  Command::new("click")
      .about("Click at coordinates within a window")
      .arg(Arg::new("window").long("window").short('w').help("Target window by title"))
      .arg(Arg::new("app").long("app").short('a').help("Target window by app name"))
      .arg(Arg::new("at").long("at").required(true)
           .help("Coordinates to click: x,y (relative to window top-left)"))
      .arg(Arg::new("json").long("json").help("Output in JSON format").action(ArgAction::SetTrue))

  // Type subcommand
  Command::new("type")
      .about("Type text into the focused element of a window")
      .arg(Arg::new("window").long("window").short('w').help("Target window by title"))
      .arg(Arg::new("app").long("app").short('a').help("Target window by app name"))
      .arg(Arg::new("text").required(true).index(1).help("Text to type"))
      .arg(Arg::new("json").long("json").help("Output in JSON format").action(ArgAction::SetTrue))

  // Key subcommand
  Command::new("key")
      .about("Send a key combination to a window")
      .arg(Arg::new("window").long("window").short('w').help("Target window by title"))
      .arg(Arg::new("app").long("app").short('a').help("Target window by app name"))
      .arg(Arg::new("combo").required(true).index(1)
           .help("Key combination (e.g., \"enter\", \"tab\", \"cmd+s\", \"cmd+shift+p\")"))
      .arg(Arg::new("json").long("json").help("Output in JSON format").action(ArgAction::SetTrue))
  ```
- **IMPLEMENT in `commands.rs`**:
  - Add `handle_click_command`, `handle_type_command`, `handle_key_command`
  - Parse `--at` as "x,y" string → split → i32 coordinates
  - Build ClickRequest/TypeRequest/KeyComboRequest with InteractionTarget from --window/--app
  - Call core handlers, format output (human-readable or JSON)
  - Log events: `cli.click_started/completed/failed`, `cli.type_started/completed/failed`, `cli.key_started/completed/failed`
  - Exit code 0 on success, non-zero on error
  - Add window/app target validation: at least one of --window or --app required
- **MIRROR**: `crates/kild-peek/src/commands.rs:147-229` (handle_screenshot_command pattern)
- **TESTS**: CLI argument parsing tests in app.rs (match existing test pattern)
- **VALIDATE**: `cargo test -p kild-peek && cargo clippy -p kild-peek -- -D warnings`

---

## Testing Strategy

### Unit Tests to Write

| Test File | Test Cases | Validates |
| --------- | ---------- | --------- |
| `interact/errors.rs` | Display messages, error codes, is_user_error, Send+Sync | Error type correctness |
| `interact/keymap.rs` | Parse cmd+s, enter, tab, escape, cmd+shift+p, unknown key, case insensitivity | Key mapping correctness |
| `interact/types.rs` | Builder construction, serialization, result constructors | Type API |
| `interact/handler.rs` | Coordinate conversion math (unit-testable helper), accessibility check existence | Core logic |
| `kild-peek/app.rs` | CLI parsing for click/type/key with all arg combos | CLI integration |

### Edge Cases Checklist

- [ ] Accessibility permission not granted → clear error message
- [ ] Window not found → appropriate error (reuse WindowError mapping)
- [ ] Window is minimized → error, cannot interact
- [ ] Coordinates outside window bounds → CoordinateOutOfBounds error
- [ ] Unknown key name in combo → UnknownKey error with the name
- [ ] Empty text string → handled gracefully (type nothing)
- [ ] Multi-modifier combo (cmd+shift+p) → all flags combined
- [ ] Single key (enter, tab) → no modifier flags
- [ ] Negative coordinates → CoordinateOutOfBounds
- [ ] --window and --app combined → use find_window_by_app_and_title
- [ ] Neither --window nor --app → clear error message

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

**EXPECT**: All tests pass (existing + new)

### Level 3: FULL_SUITE

```bash
cargo test --all && cargo build --all
```

**EXPECT**: All tests pass, full workspace builds cleanly

### Level 4: MANUAL_VALIDATION

```bash
# 1. List windows to find a target
cargo run -p kild-peek -- list windows

# 2. Test click (use a safe target like Finder)
cargo run -p kild-peek -- click --app "Finder" --at 100,50

# 3. Test type (open TextEdit first)
cargo run -p kild-peek -- type --app "TextEdit" "hello world"

# 4. Test key combo
cargo run -p kild-peek -- key --app "TextEdit" "cmd+a"

# 5. Test special keys
cargo run -p kild-peek -- key --app "TextEdit" "enter"
cargo run -p kild-peek -- key --app "TextEdit" "tab"

# 6. Test JSON output
cargo run -p kild-peek -- click --app "Finder" --at 100,50 --json

# 7. Test error cases
cargo run -p kild-peek -- click --window "NONEXISTENT" --at 50,50  # Should fail
cargo run -p kild-peek -- key --app "Finder" "unknownkey"          # Should fail
cargo run -p kild-peek -- click --app "Finder" --at 99999,99999    # Should fail
```

---

## Acceptance Criteria

- [ ] `kild-peek click --window "X" --at x,y` clicks at window-relative coordinates
- [ ] `kild-peek type --window "X" "text"` types text into focused element
- [ ] `kild-peek key --window "X" "combo"` sends key combination
- [ ] `--app` flag works as alternative to `--window` for all three commands
- [ ] `--app` and `--window` can be combined for precision targeting
- [ ] `--json` flag outputs structured JSON result
- [ ] Accessibility permission denied produces clear, actionable error message
- [ ] Window not found, minimized, and out-of-bounds coordinates produce appropriate errors
- [ ] All existing tests continue to pass
- [ ] Follows existing logging conventions (peek.core.interact.*, cli.click_*, etc.)
- [ ] Level 1-3 validation commands pass

---

## Completion Checklist

- [ ] All tasks completed in dependency order (1→2→3→4→5→6→7→8)
- [ ] Each task validated immediately after completion
- [ ] Level 1: `cargo fmt --check && cargo clippy --all -- -D warnings` passes
- [ ] Level 2: `cargo test --all` passes
- [ ] Level 3: `cargo build --all` succeeds
- [ ] Level 4: Manual validation with real windows succeeds

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
| ---- | ---------- | ------ | ---------- |
| core-graphics 0.24 CGEvent API differs from 0.25 docs | MED | MED | Use 0.24 docs.rs, fall back to raw FFI if needed |
| Accessibility permission prompt doesn't appear | LOW | LOW | Raw FFI to AXIsProcessTrusted, manual instructions in error |
| Window focus via AppleScript is flaky | MED | LOW | Add 50ms delay after focus; document as known limitation |
| Events sent too fast for macOS to process | MED | LOW | 10ms delay between down/up events; configurable in future |
| core-graphics version conflict with gpui pin | LOW | HIGH | Using exact same version (0.24) already in lockfile |

---

## Notes

- **Coordinate system**: CGEvent uses screen-absolute coordinates with top-left origin. We convert window-relative to absolute using WindowInfo's x()/y() position. This is correct for the CG coordinate system.
- **Window focus strategy**: AppleScript `tell application "System Events"` is the most reliable cross-app focus method. Alternative (NSRunningApplication) would require objc crate.
- **Text typing via set_string**: CGEvent's `set_string()` method types the entire string in one event, which is more efficient and reliable than simulating individual keystrokes. This works for basic text input. For key combos, we use individual key events with modifier flags.
- **core-graphics version**: We pin to 0.24 because it's already resolved in the lockfile via gpui's dependency chain. The Cargo.toml has a documented `core-text = "=21.0.0"` pin specifically to avoid core-graphics version conflicts.
