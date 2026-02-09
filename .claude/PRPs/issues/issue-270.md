# Investigation: iTerm focus_window fails with AppleEvent handler error (-10000)

**Issue**: #270 (https://github.com/Wirasm/kild/issues/270)
**Type**: BUG
**Investigated**: 2026-02-09

### Assessment

| Metric     | Value    | Reasoning                                                                                          |
| ---------- | -------- | -------------------------------------------------------------------------------------------------- |
| Severity   | MEDIUM   | `kild focus` is broken for iTerm users, but `kild hide` works and users can manually switch windows |
| Complexity | LOW      | Single constant change in one file, no logic changes needed                                        |
| Confidence | HIGH     | Root cause is confirmed (invalid AppleScript property), fix is user-verified                        |

---

## Problem Statement

`kild focus <branch>` always fails on iTerm2 with AppleEvent handler error (-10000). The `ITERM_FOCUS_SCRIPT` uses `set frontmost of window id X to true`, but `frontmost` is not a valid window property in iTerm2's AppleScript dictionary. The fix is to use `set miniaturized ... to false` + `select window id X`, which has been manually verified.

---

## Analysis

### Root Cause

WHY: `kild focus` fails with "AppleEvent handler failed. (-10000)"
-> BECAUSE: The AppleScript sent to iTerm uses an invalid property
-> Evidence: `crates/kild-core/src/terminal/backends/iterm.rs:38` - `set frontmost of window id {window_id} to true`

WHY: Why is `set frontmost` invalid?
-> BECAUSE: iTerm2's AppleScript dictionary does not expose a `frontmost` property on `window` objects. The `frontmost` property exists on `application` (read-only), not on individual windows.

ROOT CAUSE: The `ITERM_FOCUS_SCRIPT` constant uses `set frontmost of window id X to true` which is not part of iTerm's scripting interface.
Evidence: `crates/kild-core/src/terminal/backends/iterm.rs:36-39`

### Evidence Chain

The working `hide_window` implementation at line 43-45 uses `set miniaturized of window id X to true` - a valid iTerm window property. The fix mirrors this pattern: use `set miniaturized ... to false` to unminiaturize, then `select window id X` to bring it to front.

### Affected Files

| File                                                       | Lines | Action | Description                                              |
| ---------------------------------------------------------- | ----- | ------ | -------------------------------------------------------- |
| `crates/kild-core/src/terminal/backends/iterm.rs`          | 32-39 | UPDATE | Fix ITERM_FOCUS_SCRIPT constant and its doc comment       |

### Integration Points

- `crates/kild-core/src/terminal/common/applescript.rs:110-168` - `focus_applescript_window` executes the script (no changes needed)
- `crates/kild-core/src/terminal/operations.rs:175` - calls `backend.focus_window(window_id)` (no changes needed)
- `crates/kild/src/commands/focus.rs:57` - CLI entry point (no changes needed)

### Git History

- **Introduced**: `1aa6d9d9` - 2026-01-26 - Original implementation in shards-core
- **Last modified**: `6ccfb49` - refactor: deduplicate terminal backend common patterns (#273)
- **Implication**: Original bug from initial implementation, never worked on iTerm

---

## Implementation Plan

### Step 1: Fix ITERM_FOCUS_SCRIPT constant and doc comment

**File**: `crates/kild-core/src/terminal/backends/iterm.rs`
**Lines**: 32-39
**Action**: UPDATE

**Current code:**
```rust
/// AppleScript template for iTerm window focusing.
/// - `activate` brings iTerm to the foreground (above other apps)
/// - `set frontmost` ensures the specific window is in front of other iTerm windows
#[cfg(target_os = "macos")]
const ITERM_FOCUS_SCRIPT: &str = r#"tell application "iTerm"
        activate
        set frontmost of window id {window_id} to true
    end tell"#;
```

**Required change:**
```rust
/// AppleScript template for iTerm window focusing.
/// - `activate` brings iTerm to the foreground (above other apps)
/// - `set miniaturized to false` restores minimized windows
/// - `select` brings the specific window in front of other iTerm windows
#[cfg(target_os = "macos")]
const ITERM_FOCUS_SCRIPT: &str = r#"tell application "iTerm"
        activate
        set miniaturized of window id {window_id} to false
        select window id {window_id}
    end tell"#;
```

**Why**: `set frontmost` is not a valid iTerm window property. `set miniaturized to false` + `select` is the correct iTerm AppleScript approach: unminiaturize handles hidden windows, select brings the window to front among other iTerm windows.

---

## Patterns to Follow

**From codebase - mirror the working hide script:**
```rust
// SOURCE: crates/kild-core/src/terminal/backends/iterm.rs:43-45
// Pattern: valid iTerm window property usage
const ITERM_HIDE_SCRIPT: &str = r#"tell application "iTerm"
        set miniaturized of window id {window_id} to true
    end tell"#;
```

---

## Edge Cases & Risks

| Risk/Edge Case                          | Mitigation                                                          |
| --------------------------------------- | ------------------------------------------------------------------- |
| Window doesn't exist                    | AppleScript will error, existing error handling surfaces it properly |
| Window is already focused               | `select` and `set miniaturized to false` are idempotent             |
| Window is minimized                     | `set miniaturized to false` restores it before `select`             |
| Multiple iTerm windows                  | `select window id X` targets the specific window by ID              |

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

1. Create a kild with iTerm backend: `kild create test-focus --terminal iterm`
2. Run `kild focus test-focus` - should bring window to foreground without error
3. Minimize the iTerm window, run `kild focus test-focus` - should restore and focus
4. Verify `kild hide test-focus` still works (no regression)

---

## Scope Boundaries

**IN SCOPE:**
- Fix `ITERM_FOCUS_SCRIPT` constant in `iterm.rs`
- Update doc comment to match new implementation

**OUT OF SCOPE (do not touch):**
- Terminal.app backend (may have same issue with `set frontmost` - separate issue)
- Ghostty backend (uses System Events, different approach)
- `focus_applescript_window` helper (works correctly, just executes the script)
- Any other AppleScript constants

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-09
- **Artifact**: `.claude/PRPs/issues/issue-270.md`
