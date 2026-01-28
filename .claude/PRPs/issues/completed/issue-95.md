# Investigation: Ghostty focus command fails due to dynamic window titles

**Issue**: #95 (https://github.com/Wirasm/kild/issues/95)
**Type**: BUG
**Investigated**: 2026-01-28T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                                                     |
| ---------- | ------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Severity   | HIGH   | Core kild functionality (`kild focus`) is completely broken for Ghostty users, the default terminal. No workaround exists within kild.       |
| Complexity | MEDIUM | Single file change (ghostty.rs), but requires understanding process/window relationship on macOS and testing both PID-based and fallback paths. |
| Confidence | HIGH   | Root cause clearly identified with evidence. Proof of concept provided in the issue shows the solution works.                                 |

---

## Problem Statement

The `kild focus <branch>` command fails for Ghostty terminals because the window title set at spawn time (`kild-<session-id>`) is dynamically overwritten by the running command (e.g., Claude Code shows "⠂ UI Build & CLI E2E Testing"). The current implementation searches for windows by title, which no longer matches.

---

## Analysis

### Root Cause / Change Rationale

WHY: The focus command fails with "Ghostty window 'kild-xxx' not found"
↓ BECAUSE: AppleScript searches for `name of w contains "kild-xxx"` but no window has that title
Evidence: `ghostty.rs:227-232`
```rust
repeat with w in windows
    if name of w contains "{}" then  -- Never matches!
        perform action "AXRaise" of w
        return "focused"
    end if
end repeat
```

↓ BECAUSE: Ghostty dynamically overwrites window titles based on the running command
Evidence: User observed windows titled "⠂ UI Build & CLI E2E Testing" instead of "kild-xxx"

↓ ROOT CAUSE: The focus implementation relies on window title matching, which is unreliable for Ghostty
Evidence: `ghostty.rs:186-189`
```rust
// Ghostty uses window title for identification, not a numeric window ID like iTerm/Terminal.app.
// Unlike AppleScript-scriptable apps, Ghostty requires System Events to manipulate windows.
```

However, the session ID IS still in the process command line:
```bash
$ pgrep -fl "kild-af405012531586b7-cli-focus-command"
79229 /Applications/Ghostty.app/Contents/MacOS/ghostty -e sh -c printf '\\033]2;''kild-af405012531586b7-cli-focus-command''\\007' ...
```

### Evidence Chain

WHY: `kild focus` returns error "Ghostty window 'kild-xxx' not found"
↓ BECAUSE: Window title no longer contains "kild-xxx" (Ghostty overwrote it)
Evidence: AppleScript sees "⠂ UI Build & CLI E2E Testing", not the original title

↓ BECAUSE: Current implementation searches by window title
Evidence: `ghostty.rs:227-228` - `if name of w contains "{}" then`

↓ ROOT CAUSE: Need to use process ID (PID) instead of window title
Evidence: `close_window` successfully uses `pkill -f` to find processes by session ID in command line

### Affected Files

| File                                                         | Lines   | Action | Description                                  |
| ------------------------------------------------------------ | ------- | ------ | -------------------------------------------- |
| `crates/kild-core/src/terminal/backends/ghostty.rs`          | 177-289 | UPDATE | Rewrite focus_window to use PID-based lookup |

### Integration Points

- `crates/kild-core/src/terminal/handler.rs:338-355` calls `operations::focus_terminal_window`
- `crates/kild-core/src/terminal/operations.rs:244-255` delegates to backend via registry
- `crates/kild/src/commands.rs:736-742` invokes focus from CLI

### Git History

- **Introduced**: `1aa6d9d9` - 2026-01-26 - "Add focus command support for Ghostty"
- **Last modified**: `160314d` - Rebrand Shards to KILD
- **Implication**: Original implementation didn't account for dynamic title changes

---

## Implementation Plan

### Step 1: Add helper function to find Ghostty process by session ID

**File**: `crates/kild-core/src/terminal/backends/ghostty.rs`
**Lines**: After line 14 (after use statements)
**Action**: ADD

Add a helper function to find the Ghostty process PID by searching for the session ID in the process command line:

```rust
/// Find the Ghostty process PID that contains the given session identifier in its command line.
/// Returns None if no matching process is found.
#[cfg(target_os = "macos")]
fn find_ghostty_pid_by_session(session_id: &str) -> Option<u32> {
    use tracing::debug;

    // Use pgrep -f to find processes with session_id in their command line
    let pgrep_output = std::process::Command::new("pgrep")
        .arg("-f")
        .arg(session_id)
        .output()
        .ok()?;

    if !pgrep_output.status.success() {
        debug!(
            event = "core.terminal.ghostty_pgrep_no_match",
            session_id = %session_id
        );
        return None;
    }

    let pids: Vec<u32> = String::from_utf8_lossy(&pgrep_output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect();

    debug!(
        event = "core.terminal.ghostty_pgrep_candidates",
        session_id = %session_id,
        candidate_count = pids.len()
    );

    // Find the Ghostty process among candidates
    // The Ghostty process will have "ghostty" in its command name
    for pid in pids {
        if is_ghostty_process(pid) {
            debug!(
                event = "core.terminal.ghostty_pid_found",
                session_id = %session_id,
                pid = pid
            );
            return Some(pid);
        }
    }

    debug!(
        event = "core.terminal.ghostty_pid_not_found",
        session_id = %session_id
    );
    None
}

/// Check if a process is a Ghostty process by examining its command name.
#[cfg(target_os = "macos")]
fn is_ghostty_process(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
        .map(|output| {
            let comm = String::from_utf8_lossy(&output.stdout);
            comm.to_lowercase().contains("ghostty")
        })
        .unwrap_or(false)
}
```

**Why**: This mirrors the pattern used by `close_window` which successfully uses process command line matching via `pkill -f`. The session ID is embedded in the command line at spawn time and persists even when the window title changes.

---

### Step 2: Add PID-based focus function

**File**: `crates/kild-core/src/terminal/backends/ghostty.rs`
**Lines**: After the helper functions from Step 1
**Action**: ADD

```rust
/// Focus a Ghostty window by finding its process via PID and using System Events.
#[cfg(target_os = "macos")]
fn focus_by_pid(pid: u32) -> Result<(), TerminalError> {
    use tracing::{debug, info};

    debug!(
        event = "core.terminal.focus_ghostty_by_pid_started",
        pid = pid
    );

    // Use System Events with unix id to target the specific process
    let focus_script = format!(
        r#"tell application "System Events"
            set targetProc to first process whose unix id is {}
            set frontmost of targetProc to true
            tell targetProc
                if (count of windows) > 0 then
                    perform action "AXRaise" of window 1
                    return "focused"
                else
                    return "no windows"
                end if
            end tell
        end tell"#,
        pid
    );

    match std::process::Command::new("osascript")
        .arg("-e")
        .arg(&focus_script)
        .output()
    {
        Ok(output) if output.status.success() => {
            let result = String::from_utf8_lossy(&output.stdout);
            let result_trimmed = result.trim();
            if result_trimmed == "focused" {
                info!(
                    event = "core.terminal.focus_completed",
                    terminal = "Ghostty",
                    method = "pid",
                    pid = pid
                );
                Ok(())
            } else {
                debug!(
                    event = "core.terminal.focus_ghostty_by_pid_no_windows",
                    pid = pid,
                    result = %result_trimmed
                );
                Err(TerminalError::FocusFailed {
                    message: format!("Ghostty process {} has no windows", pid),
                })
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            debug!(
                event = "core.terminal.focus_ghostty_by_pid_failed",
                pid = pid,
                stderr = %stderr.trim()
            );
            Err(TerminalError::FocusFailed {
                message: format!("Failed to focus Ghostty by PID {}: {}", pid, stderr.trim()),
            })
        }
        Err(e) => {
            debug!(
                event = "core.terminal.focus_ghostty_by_pid_error",
                pid = pid,
                error = %e
            );
            Err(TerminalError::FocusFailed {
                message: format!("osascript error for PID {}: {}", pid, e),
            })
        }
    }
}
```

**Why**: Uses System Events `unix id` to target the exact process, bypassing the need to match window titles. This is the proven approach from the issue's proof of concept.

---

### Step 3: Rewrite focus_window to use PID-first strategy

**File**: `crates/kild-core/src/terminal/backends/ghostty.rs`
**Lines**: 177-289
**Action**: REPLACE

Replace the current `focus_window` implementation with a PID-first approach that falls back to title-based search:

```rust
#[cfg(target_os = "macos")]
fn focus_window(&self, window_id: &str) -> Result<(), TerminalError> {
    use tracing::{debug, error, info, warn};

    debug!(
        event = "core.terminal.focus_ghostty_started",
        window_id = %window_id
    );

    // Step 1: Activate Ghostty app to bring it to the foreground
    let activate_script = r#"tell application "Ghostty" to activate"#;
    if let Err(e) = std::process::Command::new("osascript")
        .arg("-e")
        .arg(activate_script)
        .output()
    {
        warn!(
            event = "core.terminal.focus_ghostty_activate_failed",
            window_id = %window_id,
            error = %e,
            message = "Failed to activate Ghostty - continuing with focus attempt"
        );
    }

    // Step 2: Try PID-based focus first (handles dynamic title changes)
    // The session ID is embedded in the process command line and persists
    // even when the window title is overwritten by running commands.
    if let Some(pid) = find_ghostty_pid_by_session(window_id) {
        debug!(
            event = "core.terminal.focus_ghostty_trying_pid",
            window_id = %window_id,
            pid = pid
        );
        match focus_by_pid(pid) {
            Ok(()) => return Ok(()),
            Err(e) => {
                debug!(
                    event = "core.terminal.focus_ghostty_pid_failed_fallback",
                    window_id = %window_id,
                    pid = pid,
                    error = %e,
                    message = "PID-based focus failed, falling back to title search"
                );
            }
        }
    } else {
        debug!(
            event = "core.terminal.focus_ghostty_no_pid_fallback",
            window_id = %window_id,
            message = "No matching Ghostty process found, falling back to title search"
        );
    }

    // Step 3: Fallback to title-based search (for edge cases)
    // This handles scenarios where the process might not be found via pgrep
    // but the window title still matches (e.g., no command running yet).
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

    match std::process::Command::new("osascript")
        .arg("-e")
        .arg(&focus_script)
        .output()
    {
        Ok(output) if output.status.success() => {
            let result = String::from_utf8_lossy(&output.stdout);
            if result.trim() == "focused" {
                info!(
                    event = "core.terminal.focus_completed",
                    terminal = "Ghostty",
                    method = "title",
                    window_id = %window_id
                );
                Ok(())
            } else {
                warn!(
                    event = "core.terminal.focus_failed",
                    terminal = "Ghostty",
                    window_id = %window_id,
                    message = "Window not found by PID or title"
                );
                Err(TerminalError::FocusFailed {
                    message: format!(
                        "Ghostty window '{}' not found (terminal may have been closed)",
                        window_id
                    ),
                })
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                event = "core.terminal.focus_failed",
                terminal = "Ghostty",
                window_id = %window_id,
                stderr = %stderr.trim()
            );
            Err(TerminalError::FocusFailed {
                message: stderr.trim().to_string(),
            })
        }
        Err(e) => {
            error!(
                event = "core.terminal.focus_failed",
                terminal = "Ghostty",
                window_id = %window_id,
                error = %e
            );
            Err(TerminalError::FocusFailed {
                message: e.to_string(),
            })
        }
    }
}
```

**Why**:
- PID-first ensures focus works even when window titles change
- Title fallback provides backwards compatibility
- Clear logging shows which method succeeded
- Improved error message clarifies the terminal may have been closed

---

### Step 4: Add tests for the new implementation

**File**: `crates/kild-core/src/terminal/backends/ghostty.rs`
**Lines**: After existing tests (line 355+)
**Action**: ADD

```rust
#[test]
fn test_is_ghostty_process_helper() {
    // Just verify the function doesn't panic with invalid PID
    // Can't test actual behavior without a running Ghostty process
    let result = is_ghostty_process(99999999);
    assert!(!result, "Non-existent PID should not be a Ghostty process");
}

#[test]
fn test_find_ghostty_pid_no_match() {
    // Search for a session ID that definitely doesn't exist
    let result = find_ghostty_pid_by_session("nonexistent-session-12345-xyz");
    assert!(
        result.is_none(),
        "Should return None for non-existent session"
    );
}
```

**Why**: These tests verify the helper functions don't panic and handle edge cases. Full integration tests would require a running Ghostty process.

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: ghostty.rs:132-138 (close_window uses pkill -f pattern)
// Pattern for process identification via command line
let escaped_id = escape_regex(id);
let result = std::process::Command::new("pkill")
    .arg("-f")
    .arg(format!("Ghostty.*{}", escaped_id));
```

```rust
// SOURCE: ghostty.rs:140-166 (close_window logging pattern)
// Pattern for handling success/no-match/error cases
match result {
    Ok(output) => {
        if output.status.success() {
            debug!(event = "..._completed", ...);
        } else {
            warn!(event = "..._no_match", ...);
        }
    }
    Err(e) => {
        warn!(event = "..._failed", ...);
    }
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                         | Mitigation                                                                                     |
| -------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Multiple Ghostty processes with same ID | `find_ghostty_pid_by_session` returns first match; unlikely in practice since IDs are unique  |
| pgrep not available                    | Falls back to title-based search; pgrep is standard on macOS                                   |
| Process found but no windows           | Return appropriate error "process has no windows"                                              |
| Terminal was closed                    | Both methods fail gracefully with "not found" error message                                    |
| Race condition (process exits between find and focus) | osascript handles gracefully with error; fallback to title search              |

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

1. Create a kild with Ghostty: `cargo run -p kild -- create test-focus --agent claude`
2. Wait for Claude Code to start and change the window title
3. Run: `cargo run -p kild -- focus test-focus`
4. Verify the window comes to foreground
5. Close the terminal manually, then run focus again to verify graceful error

---

## Scope Boundaries

**IN SCOPE:**
- Rewriting `focus_window` in `ghostty.rs` to use PID-based lookup
- Adding helper functions for PID detection
- Adding unit tests for new helpers
- Maintaining fallback to title-based search

**OUT OF SCOPE (do not touch):**
- Other terminal backends (iTerm, Terminal.app) - they use numeric window IDs that work correctly
- The `execute_spawn` function - the ANSI title setting is still useful for process identification
- The `close_window` function - it already uses the correct pattern
- Changes to the TerminalBackend trait

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-28T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-95.md`
