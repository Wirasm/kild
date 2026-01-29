# Feature: Add --app filter to kild-peek commands

## Summary

Add `--app` flag to `screenshot`, `assert`, and `list windows` commands to filter windows by application name. This provides unambiguous window targeting when multiple windows share similar titles (e.g., "KILD" appearing in kild-ui app, Ghostty terminal, and Zed editor).

## User Story

As a developer using kild-peek for UI verification
I want to filter windows by application name
So that I can reliably target a specific application when multiple windows have similar titles

## Problem Statement

When testing kild-ui, multiple windows may contain "KILD" in their title:
- "KILD" (kild-ui app)
- "Build UI with Kild-Peek" (Ghostty terminal)
- "kild — CLAUDE.md" (Zed editor)

The current `--window` flag uses title matching which can be ambiguous when apps share naming patterns. The app_name field is already captured during window enumeration but not exposed for filtering.

## Solution Statement

Add `--app` flag that filters windows by application name (e.g., "kild-ui", "Ghostty", "Zed"). The implementation will:
1. Add new `find_window_by_app()` function in kild-peek-core using the same 4-level matching pattern as `find_window_by_title()`
2. Add `--app` CLI flag to `screenshot`, `assert`, and `list windows` commands
3. Allow combining `--app` with `--window` for precise matching (filter by app first, then match title within that app's windows)

## Metadata

| Field            | Value                               |
| ---------------- | ----------------------------------- |
| Type             | NEW_CAPABILITY                      |
| Complexity       | MEDIUM                              |
| Systems Affected | kild-peek-core (window), kild-peek (CLI) |
| Dependencies     | xcap (existing)                     |
| Estimated Tasks  | 9                                   |

---

## UX Design

### Before State
```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           SCREENSHOT BY TITLE                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  User wants to screenshot kild-ui window titled "KILD"                      │
│                                                                             │
│  $ kild-peek screenshot --window "KILD" -o /tmp/ui.png                      │
│                                                                             │
│  PROBLEM: Multiple windows match "KILD":                                    │
│    - kild-ui app: "KILD"                                                    │
│    - Ghostty: "Build UI with Kild-Peek"                                     │
│    - Zed: "kild — CLAUDE.md"                                                │
│                                                                             │
│  RESULT: First match returned (unpredictable which app)                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### After State
```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           SCREENSHOT BY APP                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  User wants to screenshot kild-ui window                                    │
│                                                                             │
│  $ kild-peek screenshot --app "kild-ui" -o /tmp/ui.png                      │
│                                                                             │
│  RESULT: Captures the kild-ui window unambiguously                          │
│                                                                             │
│  ─────────────────────────────────────────────────────────                  │
│                                                                             │
│  For maximum precision, combine --app and --window:                         │
│                                                                             │
│  $ kild-peek screenshot --app "Ghostty" --window "KILD" -o /tmp/term.png    │
│                                                                             │
│  RESULT: Captures Ghostty window that contains "KILD" in title              │
│                                                                             │
│  ─────────────────────────────────────────────────────────                  │
│                                                                             │
│  List windows filtered by app:                                              │
│                                                                             │
│  $ kild-peek list windows --app "Ghostty"                                   │
│                                                                             │
│  RESULT: Shows only Ghostty windows                                         │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| `screenshot` | Only `--window`, `--window-id`, `--monitor` | Also `--app` (can combine with `--window`) | Unambiguous app targeting |
| `assert` | Only `--window` | Also `--app` (can combine with `--window`) | Reliable assertions |
| `list windows` | No filtering | Optional `--app` filter | Quick app window discovery |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-peek-core/src/window/handler.rs` | 250-365 | `find_window_by_title()` pattern to MIRROR exactly for `find_window_by_app()` |
| P0 | `crates/kild-peek-core/src/window/errors.rs` | 1-40 | Error type pattern for new `WindowNotFoundByApp` variant |
| P0 | `crates/kild-peek/src/app.rs` | 48-98 | Screenshot CLI arg pattern to follow for `--app` |
| P1 | `crates/kild-peek/src/commands.rs` | 105-174 | `handle_screenshot_command()` extraction pattern |
| P1 | `crates/kild-peek-core/src/screenshot/handler.rs` | 132-179 | `capture_window_by_title()` to extend for app filtering |
| P2 | `crates/kild-peek-core/src/assert/handler.rs` | 47-131 | Assertion handlers that use window finding |

---

## Patterns to Mirror

**FIND_WINDOW_PATTERN:**
```rust
// SOURCE: crates/kild-peek-core/src/window/handler.rs:261-318
// COPY THIS PATTERN for find_window_by_app():
pub fn find_window_by_title(title: &str) -> Result<WindowInfo, WindowError> {
    info!(event = "core.window.find_started", title = title);

    let title_lower = title.to_lowercase();

    let xcap_windows = xcap::Window::all().map_err(|e| WindowError::EnumerationFailed {
        message: e.to_string(),
    })?;

    let windows_with_props: Vec<_> = xcap_windows
        .into_iter()
        .map(|w| {
            let window_title = w.title().ok().unwrap_or_default();
            let app_name = w.app_name().ok().unwrap_or_default();
            (w, window_title, app_name)
        })
        .collect();

    // Try each match type in priority order
    if let Some(result) = try_match(&windows_with_props, &title_lower, MatchType::ExactTitle, title) {
        return result;
    }
    // ... continue with other match types

    Err(WindowError::WindowNotFound { title: title.to_string() })
}
```

**ERROR_TYPE_PATTERN:**
```rust
// SOURCE: crates/kild-peek-core/src/window/errors.rs:3-19
// COPY THIS PATTERN for WindowNotFoundByApp variant:
#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("Window not found: '{title}'")]
    WindowNotFound { title: String },

    #[error("Window not found with id: {id}")]
    WindowNotFoundById { id: u32 },

    // Add new variant following this pattern:
    // #[error("Window not found for app: '{app}'")]
    // WindowNotFoundByApp { app: String },
}
```

**CLI_ARG_PATTERN:**
```rust
// SOURCE: crates/kild-peek/src/app.rs:51-63
// COPY THIS PATTERN for --app arg:
.arg(
    Arg::new("window")
        .long("window")
        .short('w')
        .help("Capture window by title (exact match preferred, falls back to partial)")
        .conflicts_with_all(["window-id", "monitor"]),
)
```

**COMMAND_EXTRACTION_PATTERN:**
```rust
// SOURCE: crates/kild-peek/src/commands.rs:106-108
// COPY THIS PATTERN for extracting --app:
let window_title = matches.get_one::<String>("window");
let window_id = matches.get_one::<u32>("window-id");
let monitor_index = matches.get_one::<usize>("monitor");
// Add: let app_name = matches.get_one::<String>("app");
```

**LOGGING_PATTERN:**
```rust
// SOURCE: crates/kild-peek-core/src/window/handler.rs:262, 336-340
// COPY THIS PATTERN for logging:
info!(event = "core.window.find_started", title = title);
info!(event = "core.window.find_completed", title = original_title, match_type = match_type.as_str());
// For --app: use "core.window.find_by_app_started", "core.window.find_by_app_completed"
```

**TEST_PATTERN:**
```rust
// SOURCE: crates/kild-peek-core/src/window/handler.rs:537-545
// COPY THIS PATTERN for tests:
#[test]
fn test_find_window_by_title_not_found() {
    let result = find_window_by_title("NONEXISTENT_WINDOW_12345_UNIQUE");
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.error_code(), "WINDOW_NOT_FOUND");
    }
}
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/kild-peek-core/src/window/errors.rs` | UPDATE | Add `WindowNotFoundByApp` error variant |
| `crates/kild-peek-core/src/window/handler.rs` | UPDATE | Add `find_window_by_app()` and `find_window_by_app_and_title()` functions |
| `crates/kild-peek-core/src/window/mod.rs` | UPDATE | Export new functions |
| `crates/kild-peek-core/src/screenshot/errors.rs` | UPDATE | Add `WindowNotFoundByApp` mapping |
| `crates/kild-peek-core/src/screenshot/handler.rs` | UPDATE | Add `capture_window_by_app()` function |
| `crates/kild-peek-core/src/screenshot/types.rs` | UPDATE | Add `CaptureTarget::WindowApp` variant |
| `crates/kild-peek/src/app.rs` | UPDATE | Add `--app` flag to screenshot, assert, list windows |
| `crates/kild-peek/src/commands.rs` | UPDATE | Extract `--app` and use new capture/find functions |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **No window-id + app combination**: If user provides `--window-id`, that's already unambiguous by definition
- **No app listing command**: Won't add `list apps` subcommand - users can use `list windows --json` and filter
- **No fuzzy app matching**: App matching will be exact or partial (case-insensitive), not fuzzy/typo-tolerant
- **No app-based monitor capture**: `--app` only applies to window capture, not monitor capture

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: ADD `WindowNotFoundByApp` error variant

- **ACTION**: UPDATE `crates/kild-peek-core/src/window/errors.rs`
- **IMPLEMENT**: Add new error variant and update `PeekError` impl
- **MIRROR**: Lines 8-12 for error format, lines 21-38 for impl
- **CODE**:
```rust
// Add after WindowNotFoundById (line 12):
#[error("Window not found for app: '{app}'")]
WindowNotFoundByApp { app: String },

// Update error_code match (after line 26):
WindowError::WindowNotFoundByApp { .. } => "WINDOW_NOT_FOUND_BY_APP",

// Update is_user_error match (add to matches! list after line 36):
| WindowError::WindowNotFoundByApp { .. }
```
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 2: ADD `find_window_by_app()` function

- **ACTION**: UPDATE `crates/kild-peek-core/src/window/handler.rs`
- **IMPLEMENT**: Add function that finds window by app name with 2-level matching (exact app, partial app)
- **MIRROR**: `find_window_by_title()` at lines 261-318, reuse `try_match` helper and `build_window_info`
- **CODE**:
```rust
/// Find a window by app name (exact match preferred, falls back to partial match)
///
/// Matching priority (returns first match at highest priority level):
/// 1. Exact case-insensitive match on app name
/// 2. Partial case-insensitive match on app name
pub fn find_window_by_app(app: &str) -> Result<WindowInfo, WindowError> {
    info!(event = "core.window.find_by_app_started", app = app);

    let app_lower = app.to_lowercase();

    let xcap_windows = xcap::Window::all().map_err(|e| WindowError::EnumerationFailed {
        message: e.to_string(),
    })?;

    let windows_with_props: Vec<_> = xcap_windows
        .into_iter()
        .map(|w| {
            let window_title = w.title().ok().unwrap_or_default();
            let app_name = w.app_name().ok().unwrap_or_default();
            (w, window_title, app_name)
        })
        .collect();

    // Try exact app match first
    if let Some(result) = try_match_app(&windows_with_props, &app_lower, true, app) {
        return result;
    }
    // Fall back to partial app match
    if let Some(result) = try_match_app(&windows_with_props, &app_lower, false, app) {
        return result;
    }

    Err(WindowError::WindowNotFoundByApp { app: app.to_string() })
}

/// Helper for app matching
fn try_match_app(
    windows: &[(xcap::Window, String, String)],
    app_lower: &str,
    exact: bool,
    original_app: &str,
) -> Option<Result<WindowInfo, WindowError>> {
    for (w, window_title, app_name) in windows {
        let matches = if exact {
            app_name.to_lowercase() == app_lower
        } else {
            app_name.to_lowercase().contains(app_lower)
        };

        if matches {
            let match_type = if exact { "exact_app" } else { "partial_app" };
            info!(
                event = "core.window.find_by_app_completed",
                app = original_app,
                match_type = match_type
            );
            return Some(build_window_info(w, window_title, app_name, original_app));
        }
    }
    None
}
```
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 3: ADD `find_window_by_app_and_title()` function

- **ACTION**: UPDATE `crates/kild-peek-core/src/window/handler.rs`
- **IMPLEMENT**: Add function that combines app and title filtering
- **MIRROR**: Same pattern as `find_window_by_app()`, but filter by app first, then match title within
- **CODE**:
```rust
/// Find a window by app name and title (for precise matching)
///
/// First filters windows to those matching the app, then applies title matching
/// within that filtered set. Returns error if app has no windows or no window matches title.
pub fn find_window_by_app_and_title(app: &str, title: &str) -> Result<WindowInfo, WindowError> {
    info!(
        event = "core.window.find_by_app_and_title_started",
        app = app,
        title = title
    );

    let app_lower = app.to_lowercase();
    let title_lower = title.to_lowercase();

    let xcap_windows = xcap::Window::all().map_err(|e| WindowError::EnumerationFailed {
        message: e.to_string(),
    })?;

    // Collect all windows and filter to app matches
    let app_windows: Vec<_> = xcap_windows
        .into_iter()
        .filter_map(|w| {
            let window_title = w.title().ok().unwrap_or_default();
            let app_name = w.app_name().ok().unwrap_or_default();
            // Include if app matches (exact or partial)
            if app_name.to_lowercase() == app_lower || app_name.to_lowercase().contains(&app_lower) {
                Some((w, window_title, app_name))
            } else {
                None
            }
        })
        .collect();

    if app_windows.is_empty() {
        return Err(WindowError::WindowNotFoundByApp { app: app.to_string() });
    }

    // Now apply title matching within app's windows
    // Priority: exact title > partial title
    if let Some(result) = try_match(&app_windows, &title_lower, MatchType::ExactTitle, title) {
        info!(
            event = "core.window.find_by_app_and_title_completed",
            app = app,
            title = title,
            match_type = "exact_title"
        );
        return result;
    }
    if let Some(result) = try_match(&app_windows, &title_lower, MatchType::PartialTitle, title) {
        info!(
            event = "core.window.find_by_app_and_title_completed",
            app = app,
            title = title,
            match_type = "partial_title"
        );
        return result;
    }

    Err(WindowError::WindowNotFound { title: title.to_string() })
}
```
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 4: EXPORT new functions in window module

- **ACTION**: UPDATE `crates/kild-peek-core/src/window/mod.rs`
- **IMPLEMENT**: Add exports for `find_window_by_app` and `find_window_by_app_and_title`
- **MIRROR**: Lines 6-9 for export pattern
- **CODE**:
```rust
pub use handler::{
    find_window_by_app, find_window_by_app_and_title, find_window_by_id, find_window_by_title,
    get_monitor, get_primary_monitor, list_monitors, list_windows,
};
```
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 5: UPDATE screenshot errors for app-based lookup

- **ACTION**: UPDATE `crates/kild-peek-core/src/screenshot/errors.rs`
- **IMPLEMENT**: Add `WindowNotFoundByApp` variant and update error mapping
- **MIRROR**: Lines 5-9 for error format
- **CODE**:
```rust
// Add new variant (after WindowNotFoundById line 9):
#[error("Window not found for app: '{app}'")]
WindowNotFoundByApp { app: String },

// Update error_code match (add after line 52):
ScreenshotError::WindowNotFoundByApp { .. } => "SCREENSHOT_WINDOW_NOT_FOUND_BY_APP",

// Update is_user_error match (add to list after line 71):
| ScreenshotError::WindowNotFoundByApp { .. }
```
- **ACTION**: UPDATE `crates/kild-peek-core/src/screenshot/handler.rs`
- **IMPLEMENT**: Update `map_window_error_to_screenshot_error` to handle new error variant
- **MIRROR**: Lines 57-98 for error mapping pattern
- **CODE**:
```rust
// Add to map_window_error_to_screenshot_error (after line 59):
WindowError::WindowNotFoundByApp { app } => ScreenshotError::WindowNotFoundByApp { app },
```
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 6: ADD `CaptureTarget::WindowApp` variant and capture functions

- **ACTION**: UPDATE `crates/kild-peek-core/src/screenshot/types.rs`
- **IMPLEMENT**: Add new capture target variants for app-based capture
- **MIRROR**: Lines 6-15 for enum pattern
- **CODE**:
```rust
// Update CaptureTarget enum (add after WindowId variant):
/// Capture a window by app name
WindowApp { app: String },
/// Capture a window by app name and title (for precision)
WindowAppAndTitle { app: String, title: String },
```
- **IMPLEMENT**: Add builder methods
- **MIRROR**: Lines 36-53 for builder pattern
- **CODE**:
```rust
// Add after window_id method:
/// Create a new capture request for a window by app name
pub fn window_app(app: impl Into<String>) -> Self {
    Self {
        target: CaptureTarget::WindowApp { app: app.into() },
        format: ImageFormat::default(),
    }
}

/// Create a new capture request for a window by app name and title
pub fn window_app_and_title(app: impl Into<String>, title: impl Into<String>) -> Self {
    Self {
        target: CaptureTarget::WindowAppAndTitle {
            app: app.into(),
            title: title.into(),
        },
        format: ImageFormat::default(),
    }
}
```
- **VALIDATE**: `cargo build -p kild-peek-core`

### Task 7: UPDATE screenshot handler to support app-based capture

- **ACTION**: UPDATE `crates/kild-peek-core/src/screenshot/handler.rs`
- **IMPLEMENT**: Add match arms for new capture targets and helper functions
- **MIRROR**: Lines 14-23 for match pattern, lines 132-179 for capture function pattern
- **CODE**:
```rust
// Update capture() match (add after WindowId arm):
CaptureTarget::WindowApp { app } => capture_window_by_app(app, &request.format),
CaptureTarget::WindowAppAndTitle { app, title } => {
    capture_window_by_app_and_title(app, title, &request.format)
}

// Add new capture functions (after capture_window_by_id):
fn capture_window_by_app(app: &str, format: &ImageFormat) -> Result<CaptureResult, ScreenshotError> {
    use crate::window::find_window_by_app;

    let window_info = find_window_by_app(app).map_err(|e| {
        debug!(event = "core.screenshot.window_by_app_error", original_error = %e);
        map_window_error_to_screenshot_error(e)
    })?;

    // Find the actual xcap window by ID to capture
    let windows = xcap::Window::all().map_err(|e| {
        let msg = e.to_string();
        if is_permission_error(&msg) {
            ScreenshotError::PermissionDenied
        } else {
            ScreenshotError::EnumerationFailed(msg)
        }
    })?;

    let window = windows
        .into_iter()
        .find(|w| w.id().ok() == Some(window_info.id()))
        .ok_or_else(|| ScreenshotError::WindowNotFoundByApp { app: app.to_string() })?;

    let title = window.title().unwrap_or_else(|_| app.to_string());
    check_window_not_minimized(&window, &title)?;

    let image = window
        .capture_image()
        .map_err(|e| ScreenshotError::CaptureFailed(e.to_string()))?;

    encode_image(image, format)
}

fn capture_window_by_app_and_title(
    app: &str,
    title: &str,
    format: &ImageFormat,
) -> Result<CaptureResult, ScreenshotError> {
    use crate::window::find_window_by_app_and_title;

    let window_info = find_window_by_app_and_title(app, title).map_err(|e| {
        debug!(event = "core.screenshot.window_by_app_and_title_error", original_error = %e);
        map_window_error_to_screenshot_error(e)
    })?;

    let windows = xcap::Window::all().map_err(|e| {
        let msg = e.to_string();
        if is_permission_error(&msg) {
            ScreenshotError::PermissionDenied
        } else {
            ScreenshotError::EnumerationFailed(msg)
        }
    })?;

    let window = windows
        .into_iter()
        .find(|w| w.id().ok() == Some(window_info.id()))
        .ok_or_else(|| ScreenshotError::WindowNotFound { title: title.to_string() })?;

    check_window_not_minimized(&window, title)?;

    let image = window
        .capture_image()
        .map_err(|e| ScreenshotError::CaptureFailed(e.to_string()))?;

    encode_image(image, format)
}
```
- **VALIDATE**: `cargo build -p kild-peek-core && cargo test -p kild-peek-core`

### Task 8: ADD `--app` CLI flag to all relevant commands

- **ACTION**: UPDATE `crates/kild-peek/src/app.rs`
- **IMPLEMENT**: Add `--app` flag to screenshot, assert, and list windows commands
- **MIRROR**: Lines 51-56 for arg pattern
- **CODE**:
```rust
// For screenshot command (add after window arg, around line 57):
.arg(
    Arg::new("app")
        .long("app")
        .short('a')
        .help("Capture window by app name (can combine with --window for precision)")
        .conflicts_with_all(["window-id", "monitor"]),
)

// For assert command (add after window arg, around line 140):
.arg(
    Arg::new("app")
        .long("app")
        .short('a')
        .help("Target window by app name (can combine with --window for precision)"),
)

// For list windows subcommand (add after json arg, around line 36):
.arg(
    Arg::new("app")
        .long("app")
        .short('a')
        .help("Filter windows by app name"),
)
```
- **VALIDATE**: `cargo build -p kild-peek`

### Task 9: UPDATE command handlers to use `--app` flag

- **ACTION**: UPDATE `crates/kild-peek/src/commands.rs`
- **IMPLEMENT**: Extract `--app` and build appropriate capture requests
- **MIRROR**: Lines 106-145 for screenshot, lines 232-266 for assert, lines 40-68 for list
- **CODE FOR SCREENSHOT**:
```rust
// In handle_screenshot_command, add extraction (after line 108):
let app_name = matches.get_one::<String>("app");

// Update capture request building (replace lines 136-145):
let request = if let Some(app) = app_name {
    if let Some(title) = window_title {
        // Both --app and --window: precise matching
        CaptureRequest::window_app_and_title(app, title).with_format(format)
    } else {
        // Just --app
        CaptureRequest::window_app(app).with_format(format)
    }
} else if let Some(title) = window_title {
    CaptureRequest::window(title).with_format(format)
} else if let Some(id) = window_id {
    CaptureRequest::window_id(*id).with_format(format)
} else if let Some(index) = monitor_index {
    CaptureRequest::monitor(*index).with_format(format)
} else {
    CaptureRequest::primary_monitor().with_format(format)
};
```
- **CODE FOR ASSERT**:
```rust
// In handle_assert_command, add extraction (after line 233):
let app_name = matches.get_one::<String>("app");

// Update assertion building to use app when provided:
let assertion = if exists_flag {
    if let Some(app) = app_name {
        if let Some(title) = window_title {
            // Use app+title for assertion (need to add this to Assertion enum)
            // For now, find window by app+title and use its actual title
            Assertion::window_exists_by_app_and_title(app, title)
        } else {
            Assertion::window_exists_by_app(app)
        }
    } else {
        let title = window_title.ok_or("--window or --app is required with --exists")?;
        Assertion::window_exists(title)
    }
} else if visible_flag {
    // Similar pattern for visible...
}
```
- **CODE FOR LIST WINDOWS**:
```rust
// In handle_list_windows, add extraction and filtering (update lines 40-68):
fn handle_list_windows(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let json_output = matches.get_flag("json");
    let app_filter = matches.get_one::<String>("app");

    info!(
        event = "cli.list_windows_started",
        json_output = json_output,
        app_filter = ?app_filter
    );

    match list_windows() {
        Ok(windows) => {
            // Apply app filter if provided
            let filtered: Vec<_> = if let Some(app) = app_filter {
                let app_lower = app.to_lowercase();
                windows
                    .into_iter()
                    .filter(|w| {
                        let name = w.app_name().to_lowercase();
                        name == app_lower || name.contains(&app_lower)
                    })
                    .collect()
            } else {
                windows
            };

            if json_output {
                println!("{}", serde_json::to_string_pretty(&filtered)?);
            } else if filtered.is_empty() {
                if app_filter.is_some() {
                    println!("No windows found for app filter.");
                } else {
                    println!("No visible windows found.");
                }
            } else {
                println!("Visible windows:");
                table::print_windows_table(&filtered);
            }

            info!(event = "cli.list_windows_completed", count = filtered.len());
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to list windows: {}", e);
            error!(event = "cli.list_windows_failed", error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
```
- **VALIDATE**: `cargo build -p kild-peek && cargo test -p kild-peek`

---

## Testing Strategy

### Unit Tests to Write

| Test File | Test Cases | Validates |
|-----------|------------|-----------|
| `crates/kild-peek-core/src/window/handler.rs` | `test_find_window_by_app_not_found`, `test_find_window_by_app_is_case_insensitive` | App finding logic |
| `crates/kild-peek/src/app.rs` | `test_cli_screenshot_app`, `test_cli_screenshot_app_and_window`, `test_cli_app_and_window_id_conflict` | CLI arg parsing |
| `crates/kild-peek-core/src/screenshot/handler.rs` | `test_capture_by_app_nonexistent` | Screenshot capture |

### Edge Cases Checklist

- [ ] App name not found (exact or partial)
- [ ] App found but no window matches title (when combining --app and --window)
- [ ] Case insensitivity for app matching
- [ ] Empty app name provided
- [ ] --app conflicts correctly with --window-id and --monitor
- [ ] --app combines correctly with --window (no conflict)
- [ ] List windows with --app filter returns empty list gracefully

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test -p kild-peek-core && cargo test -p kild-peek
```

**EXPECT**: All tests pass

### Level 3: FULL_SUITE

```bash
cargo test --all && cargo build --all
```

**EXPECT**: All tests pass, build succeeds

### Level 4: MANUAL_VALIDATION

```bash
# List windows and identify an app
cargo run -p kild-peek -- list windows

# Screenshot by app name
cargo run -p kild-peek -- screenshot --app "Ghostty" -o /tmp/app-test.png

# Screenshot with app + window combo
cargo run -p kild-peek -- screenshot --app "Ghostty" --window "kild" -o /tmp/combo-test.png

# List windows filtered by app
cargo run -p kild-peek -- list windows --app "Ghostty"

# Assert app window exists
cargo run -p kild-peek -- assert --app "Ghostty" --exists

# Verify conflicts work
cargo run -p kild-peek -- screenshot --app "Test" --window-id 123  # Should error
cargo run -p kild-peek -- screenshot --app "Test" --monitor 0      # Should error
```

---

## Acceptance Criteria

- [ ] `kild-peek screenshot --app <name>` captures window by app name
- [ ] `kild-peek screenshot --app <name> --window <title>` captures with both filters
- [ ] `kild-peek assert --app <name> --exists` works correctly
- [ ] `kild-peek assert --app <name> --visible` works correctly
- [ ] `kild-peek list windows --app <name>` filters by app name
- [ ] `--app` conflicts with `--window-id` and `--monitor` (as expected)
- [ ] `--app` can combine with `--window` for precision
- [ ] Case-insensitive app matching (exact match prioritized over partial)
- [ ] Clear error messages when app not found
- [ ] Level 1-3 validation commands pass with exit 0

---

## Completion Checklist

- [ ] All tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] Level 1: Static analysis (fmt + clippy) passes
- [ ] Level 2: Unit tests pass
- [ ] Level 3: Full test suite + build succeeds
- [ ] Level 4: Manual validation passes
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| App name varies across systems | LOW | MED | Use case-insensitive matching with exact/partial fallback |
| Multiple windows for same app | MED | LOW | Document that first match is returned; use --window combo for precision |
| Performance with many windows | LOW | LOW | Current pattern already iterates all windows; no additional overhead |

---

## Notes

- The `app_name` field is already captured by `xcap` during window enumeration, so this is exposing existing data through a new filter
- Matching priority mimics `find_window_by_title`: exact match first, then partial
- The `--app` + `--window` combination provides maximum precision for disambiguation
- Consider adding `--list-apps` convenience subcommand in future if users request it
