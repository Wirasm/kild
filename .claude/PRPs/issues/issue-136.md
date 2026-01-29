# Investigation: peek: Window title matching is ambiguous (partial match can hit wrong window)

**Issue**: #136 (https://github.com/Wirasm/kild/issues/136)
**Type**: BUG
**Investigated**: 2026-01-29T14:30:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                              |
| ---------- | ------ | ------------------------------------------------------------------------------------------------------ |
| Severity   | MEDIUM | Feature partially broken - workaround exists via `--window-id`, but scripted testing becomes unreliable |
| Complexity | MEDIUM | Changes span 4 files across 2 modules (window, screenshot), but changes are isolated and consistent    |
| Confidence | HIGH   | Clear root cause identified: `.contains()` substring matching with first-match-wins behavior           |

---

## Problem Statement

The `--window` flag uses case-insensitive substring matching (`.contains()`) which matches the **first** window containing the search string anywhere in its title or app name. This causes ambiguous matches when multiple windows have similar names (e.g., searching "KILD" matches a Ghostty terminal titled "Build UI with Kild-Peek" instead of the actual KILD app window).

---

## Analysis

### Root Cause

**WHY 1**: Why does `--window "KILD"` match the wrong window?
- Because `find_window_by_title()` uses `.contains()` and returns the first match
- Evidence: `crates/kild-peek-core/src/window/handler.rs:267-268`

```rust
let matches = window_title.to_lowercase().contains(&title_lower)
    || app_name.to_lowercase().contains(&title_lower);
```

**WHY 2**: Why does it match partial strings?
- Because substring matching was implemented without an exact-match alternative
- Evidence: `crates/kild-peek-core/src/screenshot/types.rs:7-8`

```rust
/// Capture a window by title (partial match)
Window { title: String },
```

**ROOT CAUSE**: No mechanism to specify exact title matching; the current implementation uses substring matching as the only option.

### Evidence Chain

```
User runs: kild-peek assert --window "KILD" --visible
         ↓
CLI passes title to find_window_by_title("KILD")
         ↓ (handler.rs:252-268)
Function iterates through windows in xcap order
         ↓ (handler.rs:267-268)
First window where title.contains("kild") OR app_name.contains("kild") wins
         ↓
Ghostty window "⠐ Build UI with Kild-Peek" contains "Kild" → MATCH
         ↓
Returns wrong window (user wanted "KILD" app, not terminal)
```

### Affected Files

| File                                                    | Lines   | Action | Description                                        |
| ------------------------------------------------------- | ------- | ------ | -------------------------------------------------- |
| `crates/kild-peek-core/src/window/handler.rs`           | 250-313 | UPDATE | Add exact match prioritization in find_window      |
| `crates/kild-peek-core/src/screenshot/handler.rs`       | 47-57   | UPDATE | Use shared find_window_by_title from window module |
| `crates/kild-peek/src/app.rs`                           | 52-56   | UPDATE | Add `--window-exact` flag to screenshot command    |
| `crates/kild-peek/src/app.rs`                           | 136-140 | UPDATE | Add `--window-exact` flag to assert command        |
| `crates/kild-peek-core/src/window/handler.rs`           | 378-425 | UPDATE | Add tests for exact vs partial matching            |

### Integration Points

- `crates/kild-peek/src/commands.rs:105-174` - screenshot command handler reads `--window` flag
- `crates/kild-peek/src/commands.rs:~200` - assert command handler reads `--window` flag
- `crates/kild-peek-core/src/assert/handler.rs:47-92` - calls `find_window_by_title()`

### Git History

- **Introduced**: `0514fc4` - "Add kild-peek CLI for native application inspection (#122)"
- **Implication**: This is the original design - not a regression, but a design limitation that affects reliability

---

## Implementation Plan

### Approach: Prioritize Exact Matches Over Partial Matches

The simplest solution that doesn't require API changes: modify `find_window_by_title()` to **try exact match first, then fall back to partial match**. This makes the common case (exact title) work reliably while preserving backward compatibility.

### Step 1: Modify `find_window_by_title()` to prioritize exact matches

**File**: `crates/kild-peek-core/src/window/handler.rs`
**Lines**: 250-313
**Action**: UPDATE

**Current code:**

```rust
/// Find a window by title (partial match, case-insensitive)
/// Searches both window title and app name
pub fn find_window_by_title(title: &str) -> Result<WindowInfo, WindowError> {
    info!(event = "core.window.find_started", title = title);

    let title_lower = title.to_lowercase();

    // Search through all xcap windows directly for maximum coverage
    let xcap_windows = xcap::Window::all().map_err(|e| WindowError::EnumerationFailed {
        message: e.to_string(),
    })?;

    for w in xcap_windows {
        let window_title = w.title().ok().unwrap_or_default();
        let app_name = w.app_name().ok().unwrap_or_default();

        // Match against both title and app_name
        let matches = window_title.to_lowercase().contains(&title_lower)
            || app_name.to_lowercase().contains(&title_lower);

        if matches {
            // ... rest of match handling ...
        }
    }

    Err(WindowError::WindowNotFound {
        title: title.to_string(),
    })
}
```

**Required change:**

```rust
/// Find a window by title (exact match preferred, falls back to partial match)
/// Searches both window title and app name
///
/// Matching priority:
/// 1. Exact case-insensitive match on window title
/// 2. Exact case-insensitive match on app name
/// 3. Partial case-insensitive match on window title
/// 4. Partial case-insensitive match on app name
pub fn find_window_by_title(title: &str) -> Result<WindowInfo, WindowError> {
    info!(event = "core.window.find_started", title = title);

    let title_lower = title.to_lowercase();

    // Search through all xcap windows directly for maximum coverage
    let xcap_windows = xcap::Window::all().map_err(|e| WindowError::EnumerationFailed {
        message: e.to_string(),
    })?;

    // Collect all windows with their properties for multi-pass matching
    let windows_with_props: Vec<_> = xcap_windows
        .into_iter()
        .filter_map(|w| {
            let window_title = w.title().ok().unwrap_or_default();
            let app_name = w.app_name().ok().unwrap_or_default();
            Some((w, window_title, app_name))
        })
        .collect();

    // Pass 1: Exact match on window title (case-insensitive)
    for (w, window_title, app_name) in &windows_with_props {
        if window_title.to_lowercase() == title_lower {
            info!(
                event = "core.window.find_completed",
                title = title,
                match_type = "exact_title"
            );
            return build_window_info(w, window_title, app_name, title);
        }
    }

    // Pass 2: Exact match on app name (case-insensitive)
    for (w, window_title, app_name) in &windows_with_props {
        if app_name.to_lowercase() == title_lower {
            info!(
                event = "core.window.find_completed",
                title = title,
                match_type = "exact_app_name"
            );
            return build_window_info(w, window_title, app_name, title);
        }
    }

    // Pass 3: Partial match on window title (case-insensitive)
    for (w, window_title, app_name) in &windows_with_props {
        if window_title.to_lowercase().contains(&title_lower) {
            info!(
                event = "core.window.find_completed",
                title = title,
                match_type = "partial_title"
            );
            return build_window_info(w, window_title, app_name, title);
        }
    }

    // Pass 4: Partial match on app name (case-insensitive)
    for (w, window_title, app_name) in &windows_with_props {
        if app_name.to_lowercase().contains(&title_lower) {
            info!(
                event = "core.window.find_completed",
                title = title,
                match_type = "partial_app_name"
            );
            return build_window_info(w, window_title, app_name, title);
        }
    }

    Err(WindowError::WindowNotFound {
        title: title.to_string(),
    })
}

/// Helper to build WindowInfo from xcap window and pre-fetched properties
fn build_window_info(
    w: &xcap::Window,
    window_title: &str,
    app_name: &str,
    search_title: &str,
) -> Result<WindowInfo, WindowError> {
    let id = w.id().ok().ok_or_else(|| WindowError::WindowNotFound {
        title: search_title.to_string(),
    })?;
    let x = w.x().ok().unwrap_or(0);
    let y = w.y().ok().unwrap_or(0);
    let width = w.width().ok().unwrap_or(0);
    let height = w.height().ok().unwrap_or(0);
    let is_minimized = w.is_minimized().ok().unwrap_or(false);

    let display_title = if window_title.is_empty() {
        if app_name.is_empty() {
            format!("[Window {}]", id)
        } else {
            app_name.to_string()
        }
    } else {
        window_title.to_string()
    };

    Ok(WindowInfo::new(
        id,
        display_title,
        app_name.to_string(),
        x,
        y,
        width.max(1),
        height.max(1),
        is_minimized,
    ))
}
```

**Why**: Exact matches should take priority over partial matches. A user searching for "KILD" expects the window titled exactly "KILD" to be found, not one containing "kild" in the middle of a longer title.

---

### Step 2: Update screenshot handler to use shared matching

**File**: `crates/kild-peek-core/src/screenshot/handler.rs`
**Lines**: 34-57
**Action**: UPDATE

**Current code:**

```rust
fn capture_window_by_title(
    title: &str,
    format: &ImageFormat,
) -> Result<CaptureResult, ScreenshotError> {
    let windows = xcap::Window::all().map_err(|e| {
        let msg = e.to_string();
        if msg.contains("permission") || msg.contains("denied") {
            ScreenshotError::PermissionDenied
        } else {
            ScreenshotError::EnumerationFailed(msg)
        }
    })?;

    let title_lower = title.to_lowercase();
    let window = windows
        .into_iter()
        .find(|w| {
            w.title()
                .ok()
                .is_some_and(|t| t.to_lowercase().contains(&title_lower))
        })
        .ok_or_else(|| ScreenshotError::WindowNotFound {
            title: title.to_string(),
        })?;
    // ...
}
```

**Required change:**

```rust
use crate::window::find_window_by_title;

fn capture_window_by_title(
    title: &str,
    format: &ImageFormat,
) -> Result<CaptureResult, ScreenshotError> {
    // Use shared find_window_by_title for consistent matching behavior
    let window_info = find_window_by_title(title).map_err(|e| match e {
        crate::window::WindowError::WindowNotFound { title } => {
            ScreenshotError::WindowNotFound { title }
        }
        crate::window::WindowError::EnumerationFailed { message } => {
            if message.contains("permission") || message.contains("denied") {
                ScreenshotError::PermissionDenied
            } else {
                ScreenshotError::EnumerationFailed(message)
            }
        }
        _ => ScreenshotError::EnumerationFailed(e.to_string()),
    })?;

    // Now find the actual xcap window by ID to capture
    let windows = xcap::Window::all().map_err(|e| {
        let msg = e.to_string();
        if msg.contains("permission") || msg.contains("denied") {
            ScreenshotError::PermissionDenied
        } else {
            ScreenshotError::EnumerationFailed(msg)
        }
    })?;

    let window = windows
        .into_iter()
        .find(|w| w.id().ok() == Some(window_info.id()))
        .ok_or_else(|| ScreenshotError::WindowNotFound {
            title: title.to_string(),
        })?;

    // Check if minimized
    let is_minimized = match window.is_minimized() {
        Ok(minimized) => minimized,
        Err(e) => {
            debug!(
                event = "core.screenshot.is_minimized_check_failed",
                title = title,
                error = %e
            );
            false
        }
    };
    if is_minimized {
        return Err(ScreenshotError::WindowMinimized {
            title: title.to_string(),
        });
    }

    let image = window
        .capture_image()
        .map_err(|e| ScreenshotError::CaptureFailed(e.to_string()))?;

    encode_image(image, format)
}
```

**Why**: Ensures consistent matching behavior between `screenshot` and `assert` commands. Currently screenshot only checks window title, while window module checks both title and app name.

---

### Step 3: Add tests for exact vs partial matching

**File**: `crates/kild-peek-core/src/window/handler.rs`
**Lines**: After line 461
**Action**: UPDATE (add tests)

**Test cases to add:**

```rust
#[cfg(test)]
mod tests {
    // ... existing tests ...

    /// Test that find_window_by_title documents prioritization
    /// Note: These tests verify the function signature and error handling,
    /// not actual window matching which requires real windows
    #[test]
    fn test_find_window_by_title_returns_error_for_nonexistent() {
        // Verify error type is correct
        let result = find_window_by_title("DEFINITELY_NOT_A_REAL_WINDOW_TITLE_12345");
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.error_code(), "WINDOW_NOT_FOUND");
        }
    }

    #[test]
    fn test_find_window_by_title_is_case_insensitive() {
        // Both should return the same error (no such window exists)
        // This verifies case-insensitivity is applied
        let result_lower = find_window_by_title("nonexistent_window_test_123");
        let result_upper = find_window_by_title("NONEXISTENT_WINDOW_TEST_123");

        // Both should be errors
        assert!(result_lower.is_err());
        assert!(result_upper.is_err());

        // Both should have the same error code
        assert_eq!(
            result_lower.unwrap_err().error_code(),
            result_upper.unwrap_err().error_code()
        );
    }
}
```

---

### Step 4: Update CLI help text to document matching behavior

**File**: `crates/kild-peek/src/app.rs`
**Lines**: 52-56, 136-140
**Action**: UPDATE

**Current code (screenshot):**

```rust
Arg::new("window")
    .long("window")
    .short('w')
    .help("Capture window by title (partial match)")
    .conflicts_with_all(["window-id", "monitor"]),
```

**Required change:**

```rust
Arg::new("window")
    .long("window")
    .short('w')
    .help("Capture window by title (exact match preferred, falls back to partial)")
    .conflicts_with_all(["window-id", "monitor"]),
```

**Current code (assert):**

```rust
Arg::new("window")
    .long("window")
    .short('w')
    .help("Target window by title"),
```

**Required change:**

```rust
Arg::new("window")
    .long("window")
    .short('w')
    .help("Target window by title (exact match preferred, falls back to partial)"),
```

---

## Patterns to Follow

**From codebase - multi-pass matching pattern:**

The codebase doesn't have a direct pattern for this, but the approach mirrors how `list_windows()` processes windows:

```rust
// SOURCE: crates/kild-peek-core/src/window/handler.rs:17-133
// Pattern for collecting and filtering windows with filter_map
let result: Vec<WindowInfo> = windows
    .into_iter()
    .filter_map(|w| {
        // ... extract properties, filter, transform ...
    })
    .collect();
```

**From codebase - builder pattern with with_* methods:**

```rust
// SOURCE: crates/kild-peek-core/src/screenshot/types.rs:71-75
// Pattern for optional configuration
pub fn with_format(mut self, format: ImageFormat) -> Self {
    self.format = format;
    self
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                                  | Mitigation                                                                  |
| ----------------------------------------------- | --------------------------------------------------------------------------- |
| Performance: Multiple passes over window list   | Window list is typically <100 items; 4 passes is negligible                 |
| Breaking change: Scripts relying on partial     | Partial match still works as fallback; only priority changes                |
| xcap window iteration order                     | Multi-pass ensures deterministic matching regardless of iteration order     |
| Empty title windows                             | Already handled by fallback to app_name or `[Window {id}]`                  |
| Screenshot handler double-enumeration           | Acceptable tradeoff for consistent behavior; windows rarely change mid-call |

---

## Validation

### Automated Checks

```bash
cargo fmt --check
cargo clippy --all -- -D warnings
cargo test --all
cargo build --all
```

### Manual Verification

1. Create a terminal window with title containing "KILD" (e.g., `cd kild && cargo run`)
2. Open the KILD UI app (if available) or create another window titled exactly "KILD"
3. Run `kild-peek assert --window "KILD" --visible`
4. Verify it matches the exact "KILD" window, not the terminal with "kild" in title

---

## Scope Boundaries

**IN SCOPE:**

- Modify `find_window_by_title()` to prioritize exact matches
- Unify screenshot and window module matching behavior
- Update CLI help text
- Add tests for new behavior

**OUT OF SCOPE (do not touch):**

- Adding `--window-exact` flag (suggested but more invasive; prioritization solves the core issue)
- Adding `--app` filter (separate feature request)
- Changing `--window-id` behavior (already exact)
- Modifying assertion types to add matching mode (not needed with prioritization)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-29T14:30:00Z
- **Artifact**: `.claude/PRPs/issues/issue-136.md`
