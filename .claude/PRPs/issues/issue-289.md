# Investigation: kild focus/hide issues with Ghostty after Core Graphics migration

**Issue**: #289 (https://github.com/Wirasm/kild/issues/289)
**Type**: BUG
**Investigated**: 2026-02-11T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                   |
| ---------- | ------ | ----------------------------------------------------------------------------------------------------------- |
| Severity   | HIGH   | Focus after hide is broken — users cannot restore minimized Ghostty windows, breaking a core workflow        |
| Complexity | MEDIUM | 2 files affected (ghostty.rs, macos.rs), requires new AX action variant and integration, moderate risk      |
| Confidence | HIGH   | Clear root cause identified with code-level evidence; `is_minimized` tracked but unused in focus path        |

---

## Problem Statement

After migrating the Ghostty backend from AppleScript to Core Graphics + Accessibility API (PR #286), `kild focus` cannot restore a window that was previously minimized via `kild hide`. The `focus_window` implementation raises the window and activates the app but never un-minimizes it, so a hidden window stays in the Dock. Additionally, the AX API title matching in `focus_window` may fail independently of the CG title match, causing silent degradation to app-level activation.

---

## Analysis

### Root Cause / Change Rationale

**Bug 1 (PRIMARY): `focus_window` doesn't unminimize**

WHY: `kild focus` fails to restore a previously hidden Ghostty window
↓ BECAUSE: `native::focus_window()` only calls `AXRaised` + `activate_app()`
Evidence: `crates/kild-core/src/terminal/native/macos.rs:264-286`

```rust
// Try Accessibility API first
match ax_raise_window(pid, &window.title) { ... }
// Always activate the app to bring it to the foreground
activate_app(&window.app_name)?;
```

↓ BECAUSE: Neither `AXRaised` nor `activate_app` undo `kAXMinimizedAttribute = true`
Evidence: `crates/kild-core/src/terminal/native/macos.rs:444-482` — `ax_perform_raise` only sets `AXRaised`/`AXMain`, never touches `kAXMinimizedAttribute`

↓ ROOT CAUSE: The `is_minimized` field is tracked in `NativeWindowInfo` but never checked or acted upon in the focus path.
Evidence: `crates/kild-core/src/terminal/native/macos.rs:91` — `is_minimized` is captured, and `types.rs:20` defines the field, but `focus_window()` at line 251-289 never reads `window.is_minimized`.

Compare to iTerm which explicitly unminimizes before focusing:
```applescript
set miniaturized of window id {window_id} to false
select window id {window_id}
```
Evidence: `crates/kild-core/src/terminal/backends/iterm.rs:55-59`

**Bug 2 (SECONDARY): AX title match is independent of CG title match**

The CG window enumeration finds a window by title (in `find_ghostty_native_window`), but the AX API performs its _own_ title search (in `ax_find_and_act_on_window`). Since Ghostty uses GPU rendering, the title exposed via CG may differ from the title exposed via AX, causing the AX step to fail silently and fall through to the imprecise app-level activation.

Evidence: `crates/kild-core/src/terminal/native/macos.rs:417-435` — AX title search is independent
Evidence: `crates/kild-core/src/terminal/native/macos.rs:273-282` — AX failure silently degrades

### Evidence Chain

WHY: `kild focus branch` does nothing visible after `kild hide branch`
↓ BECAUSE: `focus_window()` at `macos.rs:251-289` never unminimizes
↓ BECAUSE: `ax_perform_raise()` only sets `AXRaised`/`AXMain` attributes (`macos.rs:444-482`)
↓ BECAUSE: `activate_app()` runs `tell application "Ghostty" to activate` (`macos.rs:539-540`) which brings the app forward but doesn't touch individual window minimize state
↓ ROOT CAUSE: Missing unminimize step — need to set `kAXMinimizedAttribute = false` before raising

### Affected Files

| File                                                        | Lines   | Action | Description                                                   |
| ----------------------------------------------------------- | ------- | ------ | ------------------------------------------------------------- |
| `crates/kild-core/src/terminal/native/macos.rs`            | 251-289 | UPDATE | Add unminimize step in `focus_window()` before raise          |
| `crates/kild-core/src/terminal/native/macos.rs`            | 381-441 | UPDATE | Add `UnminimizeAndRaise` variant to `WindowAction` enum       |
| `crates/kild-core/src/terminal/backends/ghostty.rs`        | 339-355 | NO CHANGE | Focus path delegates correctly, no changes needed          |
| `crates/kild-core/src/terminal/backends/ghostty.rs`        | 358-374 | NO CHANGE | Hide path delegates correctly, no changes needed           |

### Integration Points

- `crates/kild/src/commands/focus.rs:57` — CLI calls `terminal_ops::focus_terminal`
- `crates/kild/src/commands/hide.rs:40` — CLI calls `terminal_ops::hide_terminal`
- `crates/kild-core/src/terminal/handler.rs:363-370` — Core handler wraps `operations::focus_terminal_window`
- `crates/kild-core/src/terminal/operations.rs:165-176` — Operations delegate to backend via registry
- `crates/kild-ui/src/views/main_view.rs:713-741` — GUI focus button also uses this path

### Git History

- **Introduced**: `768e8bd` - 2026-02-09 - "fix: replace Ghostty AppleScript with Core Graphics + Accessibility API (#286)"
- **Previous approach**: AppleScript used `set miniaturized to false` + `set index to 1` for focus (removed in this commit)
- **Implication**: Regression introduced in the CG migration — the AppleScript approach handled unminimize but the AX replacement doesn't

---

## Implementation Plan

### Step 1: Add `UnminimizeAndRaise` action to `WindowAction` enum

**File**: `crates/kild-core/src/terminal/native/macos.rs`
**Lines**: 381-384
**Action**: UPDATE

**Current code:**
```rust
enum WindowAction {
    Raise,
    Minimize,
}
```

**Required change:**
```rust
enum WindowAction {
    Raise,
    UnminimizeAndRaise,
    Minimize,
}
```

**Why**: New action variant to handle the unminimize-then-raise sequence in a single AX window lookup.

### Step 2: Handle `UnminimizeAndRaise` in `ax_find_and_act_on_window`

**File**: `crates/kild-core/src/terminal/native/macos.rs`
**Lines**: 430-434
**Action**: UPDATE

**Current code:**
```rust
            return match action {
                WindowAction::Raise => ax_perform_raise(window_element),
                WindowAction::Minimize => ax_set_minimized(window_element, true),
            };
```

**Required change:**
```rust
            return match action {
                WindowAction::Raise => ax_perform_raise(window_element),
                WindowAction::UnminimizeAndRaise => {
                    // Unminimize first, then raise
                    ax_set_minimized(window_element, false)?;
                    ax_perform_raise(window_element)
                }
                WindowAction::Minimize => ax_set_minimized(window_element, true),
            };
```

**Why**: When a window is minimized, we must unset `kAXMinimizedAttribute` before raising, otherwise the raise has no visible effect.

### Step 3: Add `ax_unminimize_and_raise_window` function

**File**: `crates/kild-core/src/terminal/native/macos.rs`
**Lines**: After line 378 (after `ax_minimize_window`)
**Action**: ADD

**Required change:**
```rust
/// Unminimize and raise a window via the Accessibility API by matching its title.
fn ax_unminimize_and_raise_window(pid: i32, title: &str) -> Result<(), String> {
    // SAFETY: AXUIElementCreateApplication creates a +1 retained AXUIElementRef.
    let app_element = unsafe { AXUIElementCreateApplication(pid) };
    if app_element.is_null() {
        return Err(format!("Failed to create AX element for PID {}", pid));
    }

    // SAFETY: app_element is a valid AXUIElementRef we just created.
    unsafe {
        AXUIElementSetMessagingTimeout(app_element, AX_MESSAGING_TIMEOUT);
    }

    let result =
        ax_find_and_act_on_window(app_element, title, WindowAction::UnminimizeAndRaise);

    // SAFETY: Release the app element (Create Rule — we own it).
    unsafe {
        core_foundation::base::CFRelease(app_element as *mut c_void);
    }

    result
}
```

**Why**: Mirrors the existing `ax_raise_window` and `ax_minimize_window` functions for consistent API surface.

### Step 4: Update `focus_window` to unminimize before raising

**File**: `crates/kild-core/src/terminal/native/macos.rs`
**Lines**: 251-289
**Action**: UPDATE

**Current code:**
```rust
pub fn focus_window(window: &NativeWindowInfo) -> Result<(), TerminalError> {
    let pid = window.pid.ok_or_else(|| TerminalError::NativeWindowError {
        message: "Cannot focus window: no PID available".to_string(),
    })?;

    debug!(
        event = "core.terminal.native.focus_started",
        window_id = window.id,
        title = %window.title,
        pid = pid
    );

    // Try Accessibility API first
    match ax_raise_window(pid, &window.title) {
        Ok(()) => {
            debug!(
                event = "core.terminal.native.focus_ax_succeeded",
                window_id = window.id,
                pid = pid
            );
        }
        Err(e) => {
            // AX failed — fall back to AppleScript activation (activates entire app,
            // can't target specific window — may focus wrong window if multiple exist)
            warn!(
                event = "core.terminal.native.focus_ax_failed_fallback",
                window_id = window.id,
                pid = pid,
                error = %e,
                message = "Accessibility API failed, falling back to app activation (less precise — activates entire app, may focus wrong window if multiple exist)"
            );
        }
    }

    // Always activate the app to bring it to the foreground
    activate_app(&window.app_name)?;

    Ok(())
}
```

**Required change:**
```rust
pub fn focus_window(window: &NativeWindowInfo) -> Result<(), TerminalError> {
    let pid = window.pid.ok_or_else(|| TerminalError::NativeWindowError {
        message: "Cannot focus window: no PID available".to_string(),
    })?;

    debug!(
        event = "core.terminal.native.focus_started",
        window_id = window.id,
        title = %window.title,
        pid = pid,
        is_minimized = window.is_minimized
    );

    // Choose AX action based on whether the window is minimized
    let ax_result = if window.is_minimized {
        debug!(
            event = "core.terminal.native.focus_unminimizing",
            window_id = window.id,
            pid = pid
        );
        ax_unminimize_and_raise_window(pid, &window.title)
    } else {
        ax_raise_window(pid, &window.title)
    };

    match ax_result {
        Ok(()) => {
            debug!(
                event = "core.terminal.native.focus_ax_succeeded",
                window_id = window.id,
                pid = pid
            );
        }
        Err(e) => {
            // AX failed — fall back to AppleScript activation (activates entire app,
            // can't target specific window — may focus wrong window if multiple exist)
            warn!(
                event = "core.terminal.native.focus_ax_failed_fallback",
                window_id = window.id,
                pid = pid,
                error = %e,
                message = "Accessibility API failed, falling back to app activation (less precise — activates entire app, may focus wrong window if multiple exist)"
            );
        }
    }

    // Always activate the app to bring it to the foreground
    activate_app(&window.app_name)?;

    Ok(())
}
```

**Why**: When CG reports the window is minimized, we must unminimize via AX API before raising. This mirrors the iTerm behavior of `set miniaturized to false` before `select window`.

### Step 5: Add tests

**File**: `crates/kild-core/src/terminal/native/macos.rs`
**Action**: UPDATE (add to existing `#[cfg(test)] mod tests`)

**Test cases to add:**
```rust
#[test]
fn test_window_action_variants() {
    // Ensure all variants exist (compile-time check via exhaustive match)
    let actions = [
        WindowAction::Raise,
        WindowAction::UnminimizeAndRaise,
        WindowAction::Minimize,
    ];
    assert_eq!(actions.len(), 3);
}
```

**Why**: Compile-time verification that the new variant exists. Integration testing of AX API requires a running Ghostty process (documented limitation).

---

## Patterns to Follow

**From codebase — mirror these exactly:**

```rust
// SOURCE: crates/kild-core/src/terminal/native/macos.rs:336-356
// Pattern for AX window operation functions (ax_raise_window, ax_minimize_window)
fn ax_raise_window(pid: i32, title: &str) -> Result<(), String> {
    let app_element = unsafe { AXUIElementCreateApplication(pid) };
    if app_element.is_null() {
        return Err(format!("Failed to create AX element for PID {}", pid));
    }
    unsafe { AXUIElementSetMessagingTimeout(app_element, AX_MESSAGING_TIMEOUT); }
    let result = ax_find_and_act_on_window(app_element, title, WindowAction::Raise);
    unsafe { core_foundation::base::CFRelease(app_element as *mut c_void); }
    result
}
```

```rust
// SOURCE: crates/kild-core/src/terminal/backends/iterm.rs:55-59
// Pattern for focus including unminimize (what Ghostty should do)
// tell application "iTerm"
//     activate
//     set miniaturized of window id {window_id} to false
//     select window id {window_id}
// end tell
```

---

## Edge Cases & Risks

| Risk/Edge Case                              | Mitigation                                                                                              |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| AX API can't find minimized window by title | CG already found it — if AX can't match title, existing fallback to `activate_app` kicks in             |
| `is_minimized` from CG is stale             | CG enumeration happens immediately before focus — staleness window is milliseconds, acceptable           |
| Unminimize succeeds but raise fails         | Window will at least be visible (unminimized), user can click it; raise failure falls through to activate |
| Ghostty doesn't expose AX windows at all    | Existing fallback to `activate_app` — same behavior as before this fix                                  |

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

1. `kild create test-branch --agent claude` → window appears
2. `kild hide test-branch` → window minimizes to Dock
3. `kild focus test-branch` → window restores from Dock and comes to foreground
4. `kild hide --all` → all Ghostty kild windows minimize
5. `kild focus test-branch` → specific window restores (others stay hidden)
6. Test with multiple Ghostty windows open to verify window-specific targeting

---

## Scope Boundaries

**IN SCOPE:**
- Fix `focus_window` to unminimize before raising (Bug 1)
- Add `UnminimizeAndRaise` action variant and handler
- Add unit tests for the new code path

**OUT OF SCOPE (do not touch):**
- `hide_window` / `minimize_window` — these work correctly
- `hide --all` System Events fallback (hides all windows) — existing documented limitation
- AX/CG title mismatch issue (Bug 2) — requires deeper architectural change, acceptable degradation exists
- Other terminal backends (iTerm, Terminal.app, Alacritty) — not affected
- CLI layer (focus.rs, hide.rs) — delegates correctly, no changes needed

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-11T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-289.md`
