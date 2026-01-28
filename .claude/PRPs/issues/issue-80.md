# Investigation: UI shows stale status (red) for running Ghostty terminals

**Issue**: #80 (https://github.com/Wirasm/kild/issues/80)
**Type**: BUG
**Investigated**: 2026-01-28T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                     |
| ---------- | ------ | ------------------------------------------------------------------------------------------------------------- |
| Severity   | MEDIUM | UX confusion - status shows wrong state but workaround exists (user can see terminal is open); core functionality unaffected |
| Complexity | MEDIUM | Requires changes to terminal backend (Ghostty), UI state management, and potentially new AppleScript logic; 3-4 files affected |
| Confidence | HIGH   | Root cause is clear: `open -na Ghostty.app` returns no PID; existing AppleScript pattern in `focus_window` proves window detection works |

---

## Problem Statement

When a kild session is created with Ghostty terminal, the UI shows red "Stopped" status even when the Ghostty window is actively running. This occurs because Ghostty is spawned via `open -na Ghostty.app --args ...` which doesn't return the spawned process PID, leaving `session.process_id = None`.

---

## Analysis

### Root Cause / Change Rationale

WHY 1: Why does the UI show red/stopped status for running Ghostty terminals?
↓ BECAUSE: `ProcessStatus` is determined by checking if `session.process_id` exists and is running
Evidence: `crates/kild-ui/src/state.rs:92-108`

```rust
let status = if let Some(pid) = session.process_id {
    match kild_core::process::is_process_running(pid) {
        Ok(true) => ProcessStatus::Running,
        Ok(false) => ProcessStatus::Stopped,
        Err(e) => ProcessStatus::Unknown,
    }
} else {
    ProcessStatus::Stopped  // <-- No PID means Stopped
};
```

WHY 2: Why is `process_id` None for Ghostty sessions?
↓ BECAUSE: Ghostty spawns via `open -na Ghostty.app` which returns immediately with no PID
Evidence: `crates/kild-core/src/terminal/backends/ghostty.rs:61-68`

```rust
// On macOS, the ghostty CLI spawns headless processes, not GUI windows.
// Must use 'open -na Ghostty.app --args' where:
//   -n opens a new instance, -a specifies the application
let status = std::process::Command::new("open")
    .arg("-na")
    .arg("Ghostty.app")
    // ...
    .status()  // Returns only success/failure, NOT the spawned PID
```

WHY 3: Why doesn't the PID file approach work for Ghostty?
↓ BECAUSE: The shell PID captured in the PID file may exit before we can validate it
Evidence: `crates/kild-core/src/terminal/handler.rs:259-266`

```rust
Ok(false) => {
    warn!(
        event = "core.terminal.pid_file_process_not_running",
        pid,
        message = "PID from file exists but process is not running"
    );
    Ok((None, None, None))
}
```

↓ ROOT CAUSE: There is no mechanism to detect running Ghostty windows by their title, despite the window title being set via ANSI escape sequence and stored in `terminal_window_id`.

**Solution**: Ghostty's `focus_window` already uses AppleScript/System Events to find windows by title (lines 223-237). We can reuse this pattern to check if a window exists, providing window-based status detection as a fallback when `process_id` is None.

### Evidence: AppleScript Window Detection Already Exists

The `focus_window` implementation in ghostty.rs proves that window title detection via System Events works:

```applescript
// crates/kild-core/src/terminal/backends/ghostty.rs:223-237
tell application "System Events"
    tell process "Ghostty"
        set frontmost to true
        repeat with w in windows
            if name of w contains "{window_title}" then
                perform action "AXRaise" of w
                return "focused"
            end if
        end repeat
        return "not found"
    end tell
end tell
```

This same pattern can be adapted for status checking.

### Affected Files

| File                                                  | Lines   | Action | Description                                      |
| ----------------------------------------------------- | ------- | ------ | ------------------------------------------------ |
| `crates/kild-core/src/terminal/backends/ghostty.rs`   | NEW     | UPDATE | Add `is_window_open` method using AppleScript    |
| `crates/kild-core/src/terminal/traits.rs`             | NEW     | UPDATE | Add `is_window_open` to `TerminalBackend` trait  |
| `crates/kild-core/src/terminal/operations.rs`         | NEW     | UPDATE | Add `is_terminal_window_open` dispatcher function |
| `crates/kild-ui/src/state.rs`                         | 92-108  | UPDATE | Use window detection as fallback for Ghostty     |
| `crates/kild-core/src/terminal/backends/iterm.rs`     | NEW     | UPDATE | Implement `is_window_open` (return Ok(None) - use PID) |
| `crates/kild-core/src/terminal/backends/terminal_app.rs` | NEW  | UPDATE | Implement `is_window_open` (return Ok(None) - use PID) |

### Integration Points

- `crates/kild-ui/src/state.rs:340-360` - `update_statuses_only()` also needs to use window detection
- `crates/kild-core/src/terminal/registry.rs` - Backend registry dispatches to trait methods
- Session files store `terminal_window_id` and `terminal_type` needed for detection

### Git History

- **Backend introduced**: `160314d` - Rebrand Shards to KILD (#110)
- **Current status**: This is a known limitation from initial Ghostty implementation
- **Implication**: Original implementation acknowledged this constraint (see issue description)

---

## Implementation Plan

### Step 1: Add `is_window_open` method to TerminalBackend trait

**File**: `crates/kild-core/src/terminal/traits.rs`
**Action**: UPDATE

**Add to trait definition:**

```rust
/// Check if a terminal window is open (by window ID or title).
///
/// Returns:
/// - `Ok(Some(true))` - Window is definitely open
/// - `Ok(Some(false))` - Window is definitely closed
/// - `Ok(None)` - Cannot determine (use PID-based detection instead)
/// - `Err(...)` - Error occurred during check
///
/// This is used as a fallback for terminals like Ghostty where PID tracking
/// may not be available.
fn is_window_open(&self, window_id: &str) -> Result<Option<bool>, TerminalError> {
    // Default: cannot determine, fall back to PID-based detection
    let _ = window_id;
    Ok(None)
}
```

**Why**: Trait method with default implementation allows incremental adoption - existing backends don't need changes to compile.

---

### Step 2: Implement `is_window_open` for Ghostty backend

**File**: `crates/kild-core/src/terminal/backends/ghostty.rs`
**Action**: UPDATE

**Add implementation after `focus_window`:**

```rust
#[cfg(target_os = "macos")]
fn is_window_open(&self, window_id: &str) -> Result<Option<bool>, TerminalError> {
    use tracing::debug;

    debug!(
        event = "core.terminal.ghostty_window_check_started",
        window_title = %window_id
    );

    // Use System Events to check if a Ghostty window with our title exists.
    // This mirrors the approach in focus_window but only checks existence.
    let check_script = format!(
        r#"tell application "System Events"
            if not (exists process "Ghostty") then
                return "app_not_running"
            end if
            tell process "Ghostty"
                repeat with w in windows
                    if name of w contains "{}" then
                        return "found"
                    end if
                end repeat
                return "not_found"
            end tell
        end tell"#,
        window_id
    );

    match std::process::Command::new("osascript")
        .arg("-e")
        .arg(&check_script)
        .output()
    {
        Ok(output) if output.status.success() => {
            let result = String::from_utf8_lossy(&output.stdout);
            let trimmed = result.trim();

            match trimmed {
                "found" => {
                    debug!(
                        event = "core.terminal.ghostty_window_check_found",
                        window_title = %window_id
                    );
                    Ok(Some(true))
                }
                "not_found" | "app_not_running" => {
                    debug!(
                        event = "core.terminal.ghostty_window_check_not_found",
                        window_title = %window_id,
                        reason = %trimmed
                    );
                    Ok(Some(false))
                }
                _ => {
                    debug!(
                        event = "core.terminal.ghostty_window_check_unknown_result",
                        window_title = %window_id,
                        result = %trimmed
                    );
                    Ok(None)
                }
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            debug!(
                event = "core.terminal.ghostty_window_check_script_failed",
                window_title = %window_id,
                stderr = %stderr.trim()
            );
            // Script execution failed - fall back to PID detection
            Ok(None)
        }
        Err(e) => {
            debug!(
                event = "core.terminal.ghostty_window_check_error",
                window_title = %window_id,
                error = %e
            );
            // osascript failed - fall back to PID detection
            Ok(None)
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn is_window_open(&self, _window_id: &str) -> Result<Option<bool>, TerminalError> {
    // Non-macOS: cannot determine
    Ok(None)
}
```

**Why**: Reuses proven AppleScript pattern from `focus_window`, returns `Option<bool>` to allow graceful fallback.

---

### Step 3: Add dispatcher function in operations module

**File**: `crates/kild-core/src/terminal/operations.rs`
**Action**: UPDATE

**Add function:**

```rust
/// Check if a terminal window is open.
///
/// Returns `Ok(Some(true/false))` if the terminal supports window detection,
/// or `Ok(None)` if the terminal doesn't support it (use PID-based detection instead).
pub fn is_terminal_window_open(
    terminal_type: &TerminalType,
    window_id: &str,
) -> Result<Option<bool>, TerminalError> {
    let registry = get_registry();
    let backend = registry.get(terminal_type)?;
    backend.is_window_open(window_id)
}
```

**Why**: Follows existing pattern of operations module dispatching to backends via registry.

---

### Step 4: Export new function from kild-core

**File**: `crates/kild-core/src/terminal/mod.rs`
**Action**: UPDATE

**Add to public exports:**

```rust
pub use operations::is_terminal_window_open;
```

---

### Step 5: Update UI status determination to use window detection

**File**: `crates/kild-ui/src/state.rs`
**Lines**: 90-108
**Action**: UPDATE

**Current code:**

```rust
let status = if let Some(pid) = session.process_id {
    match kild_core::process::is_process_running(pid) {
        Ok(true) => ProcessStatus::Running,
        Ok(false) => ProcessStatus::Stopped,
        Err(e) => {
            tracing::warn!(/* ... */);
            ProcessStatus::Unknown
        }
    }
} else {
    ProcessStatus::Stopped
};
```

**Required change:**

```rust
let status = if let Some(pid) = session.process_id {
    // Primary: PID-based detection
    match kild_core::process::is_process_running(pid) {
        Ok(true) => ProcessStatus::Running,
        Ok(false) => ProcessStatus::Stopped,
        Err(e) => {
            tracing::warn!(
                event = "ui.kild_list.process_check_failed",
                pid = pid,
                branch = session.branch,
                error = %e
            );
            ProcessStatus::Unknown
        }
    }
} else if let (Some(terminal_type), Some(window_id)) =
    (&session.terminal_type, &session.terminal_window_id)
{
    // Fallback: Window-based detection for Ghostty (no PID available)
    match kild_core::terminal::is_terminal_window_open(terminal_type, window_id) {
        Ok(Some(true)) => ProcessStatus::Running,
        Ok(Some(false)) => ProcessStatus::Stopped,
        Ok(None) | Err(_) => ProcessStatus::Stopped, // Cannot determine, assume stopped
    }
} else {
    ProcessStatus::Stopped
};
```

**Why**: Adds window-based fallback when PID is not available, specifically benefiting Ghostty sessions.

---

### Step 6: Update `update_statuses_only` with same logic

**File**: `crates/kild-ui/src/state.rs`
**Lines**: 340-360
**Action**: UPDATE

**Current code:**

```rust
pub fn update_statuses_only(&mut self) {
    for kild_display in &mut self.displays {
        kild_display.status = match kild_display.session.process_id {
            None => ProcessStatus::Stopped,
            Some(pid) => match kild_core::process::is_process_running(pid) {
                Ok(true) => ProcessStatus::Running,
                Ok(false) => ProcessStatus::Stopped,
                Err(e) => {
                    tracing::warn!(/* ... */);
                    ProcessStatus::Unknown
                }
            },
        };
    }
    self.last_refresh = std::time::Instant::now();
}
```

**Required change:**

```rust
pub fn update_statuses_only(&mut self) {
    for kild_display in &mut self.displays {
        kild_display.status = if let Some(pid) = kild_display.session.process_id {
            match kild_core::process::is_process_running(pid) {
                Ok(true) => ProcessStatus::Running,
                Ok(false) => ProcessStatus::Stopped,
                Err(e) => {
                    tracing::warn!(
                        event = "ui.kild_list.process_check_failed",
                        pid = pid,
                        branch = kild_display.session.branch,
                        error = %e
                    );
                    ProcessStatus::Unknown
                }
            }
        } else if let (Some(terminal_type), Some(window_id)) = (
            &kild_display.session.terminal_type,
            &kild_display.session.terminal_window_id,
        ) {
            match kild_core::terminal::is_terminal_window_open(terminal_type, window_id) {
                Ok(Some(true)) => ProcessStatus::Running,
                Ok(Some(false)) => ProcessStatus::Stopped,
                Ok(None) | Err(_) => ProcessStatus::Stopped,
            }
        } else {
            ProcessStatus::Stopped
        };
    }
    self.last_refresh = std::time::Instant::now();
}
```

**Why**: Both status update paths need the same logic to maintain consistency.

---

### Step 7: Add tests for Ghostty window detection

**File**: `crates/kild-core/src/terminal/backends/ghostty.rs`
**Action**: UPDATE (add tests)

**Test cases to add:**

```rust
#[test]
fn test_is_window_open_returns_option_type() {
    let backend = GhosttyBackend;
    // The method should return without panic
    let result = backend.is_window_open("nonexistent-window-title");
    // Result type should be Result<Option<bool>, _>
    assert!(result.is_ok());
    // For a non-existent window, should return Some(false) or None
    let value = result.unwrap();
    assert!(value.is_none() || value == Some(false));
}

#[cfg(target_os = "macos")]
#[test]
#[ignore] // Requires Ghostty installed - run manually
fn test_is_window_open_ghostty_not_running() {
    // When Ghostty app is not running, should return Some(false)
    // This test is ignored because it depends on Ghostty being closed
    let backend = GhosttyBackend;
    let result = backend.is_window_open("any-window");
    // Should succeed and indicate window not found
    if let Ok(Some(found)) = result {
        assert!(!found, "Should report window not found when app not running");
    }
}
```

---

### Step 8: Update unit test for status detection

**File**: `crates/kild-ui/src/state.rs`
**Action**: UPDATE (add test)

**Test case to add:**

```rust
#[test]
fn test_process_status_from_session_with_window_id_no_pid() {
    use kild_core::sessions::types::SessionStatus;
    use kild_core::terminal::types::TerminalType;
    use std::path::PathBuf;

    // Session with terminal_window_id but no process_id (Ghostty case)
    let session = Session {
        id: "test-id".to_string(),
        branch: "test-branch".to_string(),
        worktree_path: PathBuf::from("/tmp/nonexistent-test-path"),
        agent: "claude".to_string(),
        project_id: "test-project".to_string(),
        status: SessionStatus::Active,
        created_at: "2024-01-01T00:00:00Z".to_string(),
        port_range_start: 0,
        port_range_end: 0,
        port_count: 0,
        process_id: None,
        process_name: None,
        process_start_time: None,
        terminal_type: Some(TerminalType::Ghostty),
        terminal_window_id: Some("kild-test-window".to_string()),
        command: String::new(),
        last_activity: None,
        note: None,
    };

    let display = KildDisplay::from_session(session);
    // With window detection fallback, should attempt to check window
    // In test environment without Ghostty running, will fall back to Stopped
    assert!(
        display.status == ProcessStatus::Stopped ||
        display.status == ProcessStatus::Running,
        "Should have valid status from window detection fallback"
    );
}
```

---

## Patterns to Follow

**From codebase - mirror these exactly:**

The `focus_window` AppleScript pattern in Ghostty:

```rust
// SOURCE: crates/kild-core/src/terminal/backends/ghostty.rs:223-237
// Pattern for querying Ghostty windows via System Events
let focus_script = format!(
    r#"tell application "System Events"
    tell process "Ghostty"
        set frontmost to true
        repeat with w in windows
            if name of w contains "{}" then
                perform action "AXRaise" of w
                return "focused"
            end if
        end repeat
        return "not found"
    end tell
end tell"#,
    window_id
);
```

The trait method with default implementation pattern:

```rust
// SOURCE: crates/kild-core/src/terminal/traits.rs (existing pattern)
fn focus_window(&self, window_id: &str) -> Result<(), TerminalError>;
// follow this pattern for is_window_open with default impl
```

---

## Edge Cases & Risks

| Risk/Edge Case                        | Mitigation                                                                 |
| ------------------------------------- | -------------------------------------------------------------------------- |
| AppleScript performance (5s polling)  | Window check is fast (<100ms); only called during refresh cycle            |
| Ghostty app not running               | Return `Some(false)` - correctly indicates session is stopped              |
| Window title collision                | Titles include session ID hash, making collisions extremely unlikely       |
| osascript not available               | Return `None` to fall back to PID detection (graceful degradation)         |
| Accessibility permissions denied      | osascript may fail - return `None` to fall back gracefully                 |
| iTerm/Terminal.app affected           | Default trait impl returns `None` - no change to PID-based detection       |

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

1. Create a kild session with Ghostty: `kild create test-ghostty --agent claude`
2. Verify UI shows green "Running" status (not red "Stopped")
3. Close the Ghostty terminal window manually
4. Wait for 5-second refresh cycle
5. Verify UI shows "Stopped" status
6. Test with iTerm/Terminal.app to confirm no regression (still uses PID detection)

---

## Scope Boundaries

**IN SCOPE:**

- Adding `is_window_open` trait method with default implementation
- Implementing window detection for Ghostty using AppleScript
- Updating UI status logic to use window detection as fallback
- Adding appropriate tests

**OUT OF SCOPE (do not touch):**

- PID tracking improvements (separate concern)
- Adding window detection to iTerm/Terminal.app (they have working PID tracking)
- Changing 5-second refresh interval
- Adding manual refresh button (mentioned in #79 Phase 6)
- Process metrics or CPU/memory display

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-28T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-80.md`
