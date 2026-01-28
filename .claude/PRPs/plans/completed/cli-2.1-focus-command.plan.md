# Implementation Plan: CLI Phase 2.1 - Focus Terminal (`shards focus`)

**Source PRD**: `.claude/PRPs/prds/cli-core-features.prd.md`
**Phase**: 2.1
**Status**: READY FOR IMPLEMENTATION

---

## Summary

Add a new `shards focus <branch>` command that brings a shard's terminal window to the foreground. This feature enables quick context switching between multiple active shards without using the mouse or hunting through windows:
- Looks up session by branch name
- Uses stored `terminal_type` and `terminal_window_id` from the session
- Executes AppleScript (macOS) to activate and raise the specific terminal window
- Handles iTerm2, Terminal.app, and Ghostty with terminal-specific focus logic

## User Story

As a power user managing multiple parallel AI agents, I want to quickly bring a shard's terminal window to the foreground so that I can check on its progress without hunting through open windows.

## Problem Statement

When running multiple shards simultaneously, users accumulate many terminal windows. Finding the right window requires:
1. Mouse navigation through window stacks
2. Using Mission Control or Cmd+Tab cycling
3. Remembering which terminal contains which shard

This breaks flow and slows down context switching between parallel workstreams.

## Solution Statement

Add `shards focus <branch>` command that:
1. Looks up the session by branch name using existing `get_session()`
2. Retrieves the stored `terminal_type` and `terminal_window_id`
3. Delegates to a new `focus_window()` method on `TerminalBackend` trait
4. Each backend implements focus using appropriate AppleScript or system commands
5. Returns success/failure with clear error messages

---

## Metadata

| Field | Value |
|-------|-------|
| Type | NEW FEATURE |
| Complexity | MEDIUM |
| Systems Affected | shards (CLI), shards-core (terminal module) |
| Dependencies | Existing terminal backend infrastructure |
| Estimated Tasks | 6 |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/shards/src/app.rs` | 1-240 | CLI command definition patterns |
| P0 | `crates/shards/src/commands.rs` | 52-72, 355-418 | Handler patterns (run_command router, handle_code_command) |
| P0 | `crates/shards-core/src/terminal/traits.rs` | 1-47 | TerminalBackend trait definition |
| P0 | `crates/shards-core/src/terminal/backends/iterm.rs` | 1-95 | iTerm backend AppleScript patterns |
| P0 | `crates/shards-core/src/terminal/backends/ghostty.rs` | 1-177 | Ghostty backend with pkill pattern |
| P0 | `crates/shards-core/src/terminal/backends/terminal_app.rs` | 1-93 | Terminal.app backend AppleScript patterns |
| P1 | `crates/shards-core/src/terminal/common/applescript.rs` | 1-99 | execute_spawn_script and close_applescript_window helpers |
| P1 | `crates/shards-core/src/terminal/operations.rs` | 142-230 | execute_spawn_script and close_terminal_window delegation |
| P1 | `crates/shards-core/src/terminal/registry.rs` | 1-84 | get_backend and detect_terminal functions |
| P1 | `crates/shards-core/src/sessions/handler.rs` | 208-221 | get_session() function |
| P2 | `crates/shards-core/src/sessions/types.rs` | 48-62 | Session struct terminal_type and terminal_window_id fields |
| P2 | `crates/shards-core/src/terminal/errors.rs` | 1-58 | TerminalError enum for new error variants |

---

## Patterns to Mirror

**CLI_COMMAND_DEFINITION (branch argument only):**
```rust
// SOURCE: crates/shards/src/app.rs:69-78 (cd command)
.subcommand(
    Command::new("cd")
        .about("Print worktree path for shell integration")
        .arg(
            Arg::new("branch")
                .help("Branch name of the shard")
                .required(true)
                .index(1)
        )
)
```

**COMMAND_HANDLER_PATTERN:**
```rust
// SOURCE: crates/shards/src/commands.rs:355-418 (handle_code_command)
fn handle_code_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    info!(event = "cli.code_started", branch = branch, ...);

    // 1. Look up the session
    let session = match session_handler::get_session(branch) {
        Ok(session) => session,
        Err(e) => {
            eprintln!("Failed to find shard '{}': {}", branch, e);
            error!(event = "cli.code_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    // ... rest of logic ...
}
```

**APPLESCRIPT_EXECUTION_PATTERN (close_applescript_window for focus):**
```rust
// SOURCE: crates/shards-core/src/terminal/common/applescript.rs:48-88
#[cfg(target_os = "macos")]
pub fn close_applescript_window(script: &str, terminal_name: &str, window_id: &str) {
    debug!(event = "core.terminal.close_started", terminal = terminal_name, window_id = %window_id);

    match std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
    {
        Ok(output) if output.status.success() => {
            debug!(event = "core.terminal.close_completed", ...);
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(event = "core.terminal.close_failed", ...);
        }
        Err(e) => {
            warn!(event = "core.terminal.close_failed", ...);
        }
    }
}
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/shards/src/app.rs` | UPDATE | Add `focus` subcommand definition |
| `crates/shards/src/commands.rs` | UPDATE | Add `handle_focus_command()` and wire into `run_command()` |
| `crates/shards-core/src/terminal/traits.rs` | UPDATE | Add `focus_window()` method to TerminalBackend trait |
| `crates/shards-core/src/terminal/backends/iterm.rs` | UPDATE | Implement `focus_window()` with AppleScript |
| `crates/shards-core/src/terminal/backends/terminal_app.rs` | UPDATE | Implement `focus_window()` with AppleScript |
| `crates/shards-core/src/terminal/backends/ghostty.rs` | UPDATE | Implement `focus_window()` using AppleScript System Events |
| `crates/shards-core/src/terminal/common/applescript.rs` | UPDATE | Add `focus_applescript_window()` helper |
| `crates/shards-core/src/terminal/operations.rs` | UPDATE | Add `focus_terminal_window()` function |
| `crates/shards-core/src/terminal/errors.rs` | UPDATE | Add `FocusFailed` error variant |
| `crates/shards-core/src/terminal/handler.rs` | UPDATE | Add public `focus_terminal()` function |

---

## NOT Building (Scope Limits)

- **No cross-platform support** - Focus is macOS-only for now (Linux/Windows would need different APIs)
- **No fallback behavior** - If window_id is missing, fail clearly rather than focusing "any" window
- **No multi-window tracking** - Shards tracks one window_id per session; `open` overwrites it
- **No agent state verification** - Focus the window regardless of whether the agent is running
- **No window restoration** - If the window was closed, don't try to reopen it

---

## Step-by-Step Tasks

### Task 1: ADD `FocusFailed` error variant to TerminalError

- **ACTION**: Add new error variant for focus failures
- **FILE**: `crates/shards-core/src/terminal/errors.rs`
- **LOCATION**: After `AppleScriptFailed` variant (around line 23)
- **IMPLEMENT**:
```rust
#[error("Failed to focus terminal window: {message}")]
FocusFailed { message: String },
```
- **ALSO UPDATE** the `ShardsError` impl's `error_code()` and `is_user_error()` match arms
- **MIRROR**: Lines 8-9 (`TerminalNotFound` variant)
- **VALIDATE**: `cargo check -p shards-core`

### Task 2: ADD `focus_window()` method to TerminalBackend trait

- **ACTION**: Extend the trait with a new focus method
- **FILE**: `crates/shards-core/src/terminal/traits.rs`
- **LOCATION**: After `close_window()` method (around line 45)
- **IMPLEMENT**:
```rust
/// Focus a terminal window (bring to foreground).
///
/// # Arguments
/// * `window_id` - The window ID (for iTerm/Terminal.app) or title (for Ghostty)
///
/// # Returns
/// * `Ok(())` - Window was focused successfully
/// * `Err(TerminalError)` - Focus failed (window not found, permission denied, etc.)
fn focus_window(&self, window_id: &str) -> Result<(), TerminalError>;
```
- **MIRROR**: Lines 35-45 (`close_window` documentation style)
- **GOTCHA**: This is NOT `Option<&str>` like close_window - window_id is required
- **VALIDATE**: `cargo check -p shards-core` (will fail until backends implement it)

### Task 3: ADD `focus_applescript_window()` helper function

- **ACTION**: Add helper for executing focus AppleScripts
- **FILE**: `crates/shards-core/src/terminal/common/applescript.rs`
- **LOCATION**: After `close_applescript_window()` function (around line 88)
- **IMPLEMENT**:
```rust
/// Focus a window via AppleScript (returns result for user feedback).
#[cfg(target_os = "macos")]
pub fn focus_applescript_window(
    script: &str,
    terminal_name: &str,
    window_id: &str,
) -> Result<(), crate::terminal::errors::TerminalError> {
    use crate::terminal::errors::TerminalError;

    debug!(event = "core.terminal.focus_started", terminal = terminal_name, window_id = %window_id);

    match std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
    {
        Ok(output) if output.status.success() => {
            info!(event = "core.terminal.focus_completed", terminal = terminal_name, window_id = %window_id);
            Ok(())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(event = "core.terminal.focus_failed", terminal = terminal_name, window_id = %window_id, stderr = %stderr.trim());
            Err(TerminalError::FocusFailed {
                message: format!("{} focus failed for window {}: {}", terminal_name, window_id, stderr.trim()),
            })
        }
        Err(e) => {
            error!(event = "core.terminal.focus_failed", terminal = terminal_name, window_id = %window_id, error = %e);
            Err(TerminalError::FocusFailed {
                message: format!("Failed to execute osascript for {} focus: {}", terminal_name, e),
            })
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn focus_applescript_window(
    _script: &str,
    _terminal_name: &str,
    _window_id: &str,
) -> Result<(), crate::terminal::errors::TerminalError> {
    Err(crate::terminal::errors::TerminalError::FocusFailed {
        message: "Focus not supported on this platform".to_string(),
    })
}
```
- **MIRROR**: Lines 48-88 (`close_applescript_window` function)
- **VALIDATE**: `cargo check -p shards-core`

### Task 4: IMPLEMENT `focus_window()` in all terminal backends

**4a. iTerm Backend**
- **FILE**: `crates/shards-core/src/terminal/backends/iterm.rs`
- **ADD SCRIPT CONSTANT**:
```rust
const ITERM_FOCUS_SCRIPT: &str = r#"tell application "iTerm"
        activate
        set frontmost of window id {window_id} to true
    end tell"#;
```
- **IMPLEMENT**:
```rust
#[cfg(target_os = "macos")]
fn focus_window(&self, window_id: &str) -> Result<(), TerminalError> {
    let script = ITERM_FOCUS_SCRIPT.replace("{window_id}", window_id);
    crate::terminal::common::applescript::focus_applescript_window(&script, self.display_name(), window_id)
}

#[cfg(not(target_os = "macos"))]
fn focus_window(&self, _window_id: &str) -> Result<(), TerminalError> {
    Err(TerminalError::FocusFailed { message: "Focus not supported on this platform".to_string() })
}
```

**4b. Terminal.app Backend**
- **FILE**: `crates/shards-core/src/terminal/backends/terminal_app.rs`
- **ADD SCRIPT CONSTANT**:
```rust
const TERMINAL_FOCUS_SCRIPT: &str = r#"tell application "Terminal"
        activate
        set frontmost of window id {window_id} to true
    end tell"#;
```
- **IMPLEMENT**: Same pattern as iTerm

**4c. Ghostty Backend**
- **FILE**: `crates/shards-core/src/terminal/backends/ghostty.rs`
- **NOTE**: Original implementation used window title matching, but this was unreliable due to dynamic title changes. Updated in issue #95 to use PID-based lookup with title fallback.
- **IMPLEMENT** (Original approach - see issue #95 for PID-based implementation):
```rust
#[cfg(target_os = "macos")]
fn focus_window(&self, window_id: &str) -> Result<(), TerminalError> {
    // Original approach used window title matching
    let activate_script = r#"tell application "Ghostty" to activate"#;
    let _ = std::process::Command::new("osascript").arg("-e").arg(activate_script).output();

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

    match std::process::Command::new("osascript").arg("-e").arg(&focus_script).output() {
        Ok(output) if output.status.success() => {
            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if result == "focused" { Ok(()) }
            else { Err(TerminalError::FocusFailed { message: format!("Ghostty window '{}' not found", window_id) }) }
        }
        Ok(output) => Err(TerminalError::FocusFailed { message: String::from_utf8_lossy(&output.stderr).to_string() }),
        Err(e) => Err(TerminalError::FocusFailed { message: e.to_string() }),
    }
}
```
- **VALIDATE**: `cargo check -p shards-core`

### Task 5: ADD `focus_terminal_window()` to operations and handler

**5a. Operations layer** (`crates/shards-core/src/terminal/operations.rs`):
```rust
#[cfg(target_os = "macos")]
pub fn focus_terminal_window(terminal_type: &TerminalType, window_id: &str) -> Result<(), TerminalError> {
    let resolved_type = match terminal_type {
        TerminalType::Native => registry::detect_terminal()?,
        t => t.clone(),
    };
    let backend = registry::get_backend(&resolved_type).ok_or(TerminalError::NoTerminalFound)?;
    backend.focus_window(window_id)
}

#[cfg(not(target_os = "macos"))]
pub fn focus_terminal_window(_terminal_type: &TerminalType, _window_id: &str) -> Result<(), TerminalError> {
    Err(TerminalError::FocusFailed { message: "Focus not supported on this platform".to_string() })
}
```

**5b. Handler layer** (`crates/shards-core/src/terminal/handler.rs`):
```rust
pub fn focus_terminal(terminal_type: &TerminalType, window_id: &str) -> Result<(), TerminalError> {
    info!(event = "core.terminal.focus_requested", terminal_type = %terminal_type, window_id = %window_id);
    operations::focus_terminal_window(terminal_type, window_id)
}
```
- **VALIDATE**: `cargo check -p shards-core`

### Task 6: ADD `focus` CLI command

**6a. CLI definition** (`crates/shards/src/app.rs`):
```rust
.subcommand(
    Command::new("focus")
        .about("Bring a shard's terminal window to the foreground")
        .arg(Arg::new("branch").help("Branch name of the shard to focus").required(true).index(1))
)
```

**6b. Command handler** (`crates/shards/src/commands.rs`):
- Add match arm: `Some(("focus", sub_matches)) => handle_focus_command(sub_matches),`
- Add handler:
```rust
fn handle_focus_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").ok_or("Branch argument is required")?;
    info!(event = "cli.focus_started", branch = branch);

    let session = match session_handler::get_session(branch) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to find shard '{}': {}", branch, e);
            error!(event = "cli.focus_failed", branch = branch, error = %e);
            return Err(e.into());
        }
    };

    let terminal_type = session.terminal_type.as_ref().ok_or("No terminal type recorded")?;
    let window_id = session.terminal_window_id.as_ref().ok_or("No window ID recorded")?;

    match shards_core::terminal_ops::focus_terminal(terminal_type, window_id) {
        Ok(()) => {
            println!("Focused shard '{}' terminal window", branch);
            info!(event = "cli.focus_completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to focus terminal for '{}': {}", branch, e);
            error!(event = "cli.focus_failed", branch = branch, error = %e);
            Err(e.into())
        }
    }
}
```

**6c. CLI tests** (`crates/shards/src/app.rs`):
```rust
#[test]
fn test_cli_focus_command() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["shards", "focus", "test-branch"]);
    assert!(matches.is_ok());
}

#[test]
fn test_cli_focus_requires_branch() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["shards", "focus"]);
    assert!(matches.is_err());
}
```
- **VALIDATE**: `cargo test -p shards test_cli_focus`

---

## Validation Commands

### Level 1: STATIC_ANALYSIS
```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

### Level 2: TYPE_CHECK
```bash
cargo check --all
```

### Level 3: BUILD
```bash
cargo build --all
```

### Level 4: TESTS
```bash
cargo test --all
```

### Level 5: MANUAL_VALIDATION
```bash
shards create test-focus --agent claude
# Switch away from terminal
shards focus test-focus
# Verify terminal comes to foreground
shards destroy --force test-focus
```

---

## Acceptance Criteria

- [ ] `shards focus <branch>` brings the terminal window to foreground (iTerm2)
- [ ] `shards focus <branch>` brings the terminal window to foreground (Terminal.app)
- [ ] `shards focus <branch>` brings the terminal window to foreground (Ghostty)
- [ ] Clear error message when shard not found
- [ ] Clear error message when no terminal_type recorded
- [ ] Clear error message when no window_id recorded
- [ ] Clear error message when window no longer exists
- [ ] All validation commands pass with exit 0

---

## Completion Checklist

- [ ] Task 1: `FocusFailed` error variant added
- [ ] Task 2: `focus_window()` method added to trait
- [ ] Task 3: `focus_applescript_window()` helper added
- [ ] Task 4: All backends implement `focus_window()`
- [ ] Task 5: Operations and handler functions added
- [ ] Task 6: CLI command and tests added
- [ ] Level 1-4 validation commands pass
- [ ] Manual testing completed
