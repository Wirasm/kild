# Investigation: Close terminal windows when killing processes

**Issue**: #43 (https://github.com/Wirasm/shards/issues/43)
**Type**: ENHANCEMENT
**Investigated**: 2026-01-21T12:34:00Z

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Priority | MEDIUM | Improves UX but doesn't affect core functionality; users have manual workaround (close windows manually) |
| Complexity | MEDIUM | Changes 4-5 files with new AppleScript close logic; integration at destroy point is straightforward |
| Confidence | HIGH | Clear path using existing AppleScript patterns; similar terminal-specific code already exists |

---

## Problem Statement

When `shards destroy` kills agent processes, terminal windows remain open and empty. Users must manually close these orphaned windows, creating desktop clutter and a poor user experience. The destroy operation should automatically close the terminal window that was opened during `shards create`.

---

## Analysis

### Change Rationale

The codebase already has terminal-specific AppleScript for launching terminals (iTerm, Terminal.app, Ghostty). The same pattern can be extended to close terminal windows. Since we track which terminal type was used during creation, we can invoke the appropriate close script during destruction.

### Evidence Chain

WHY: Terminal windows remain open after `shards destroy`
↓ BECAUSE: `destroy_session()` only kills the agent process, not the terminal window
  Evidence: `src/sessions/handler.rs:183-213`
  ```rust
  // 2. Kill process if PID is tracked
  if let Some(pid) = session.process_id {
      match crate::process::kill_process(pid, ...) { ... }
  }
  // NO terminal close logic after this
  ```

↓ BECAUSE: No terminal close functionality exists in the codebase
  Evidence: `src/terminal/operations.rs:6-28` - Only launch scripts, no close scripts
  ```rust
  const ITERM_SCRIPT: &str = r#"tell application "iTerm"
          create window with default profile
          ...
      end tell"#;
  // No ITERM_CLOSE_SCRIPT defined
  ```

↓ BECAUSE: Session struct doesn't store terminal type for use during destroy
  Evidence: `src/sessions/types.rs:11-61` - Session has `process_id` but no `terminal_type`
  ```rust
  pub struct Session {
      pub process_id: Option<u32>,
      pub process_name: Option<String>,
      // No terminal_type field
  }
  ```

↓ ROOT CAUSE: Missing terminal metadata in Session and no close terminal functionality
  Evidence: `src/terminal/types.rs:19-26` - SpawnResult has terminal_type but it's not persisted to Session

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `src/terminal/types.rs` | 4-9 | UPDATE | Add serde derives to TerminalType for serialization |
| `src/terminal/operations.rs` | 6-28 | UPDATE | Add AppleScript close templates and close_terminal_window function |
| `src/terminal/handler.rs` | NEW | UPDATE | Add close_terminal() handler function |
| `src/terminal/errors.rs` | 4-31 | UPDATE | Add TerminalCloseFailed error variant |
| `src/sessions/types.rs` | 11-61 | UPDATE | Add terminal_type field to Session struct |
| `src/sessions/handler.rs` | 161-242 | UPDATE | Call close_terminal() in destroy_session() before killing process |

### Integration Points

- `src/sessions/handler.rs:86-87` - spawn_terminal() returns SpawnResult with terminal_type
- `src/sessions/handler.rs:91-110` - Session creation copies SpawnResult fields to Session
- `src/sessions/handler.rs:183-213` - Process kill logic (close terminal should happen BEFORE this)
- `src/terminal/mod.rs` - Module exports (add close_terminal)

### Git History

- **Terminal support added**: `1d1b229` - Implements terminal type selection and cross-platform support
- **PID tracking added**: `ee1c14e` - Add PID tracking and process management
- **Last terminal fix**: `80788d3` - Fix extra empty terminal window on iTerm2 launch
- **Implication**: Infrastructure for terminal-specific handling exists; this extends it

---

## Implementation Plan

### Step 1: Add serde derives to TerminalType enum

**File**: `src/terminal/types.rs`
**Lines**: 3-9
**Action**: UPDATE

**Current code:**
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalType {
    ITerm,
    TerminalApp,
    Ghostty,
    Native, // System default
}
```

**Required change:**
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TerminalType {
    ITerm,
    TerminalApp,
    Ghostty,
    Native, // System default
}
```

**Why**: Enables TerminalType to be stored in Session JSON files for persistence across create/destroy.

---

### Step 2: Add terminal_type field to Session struct

**File**: `src/sessions/types.rs`
**Lines**: 11-61
**Action**: UPDATE

**Add import at top:**
```rust
use crate::terminal::types::TerminalType;
```

**Add field after process_start_time (around line 41):**
```rust
    pub process_start_time: Option<u64>,

    /// Terminal type used to launch this session (iTerm, Terminal.app, Ghostty)
    ///
    /// Used to close the terminal window during destroy.
    /// None for sessions created before this field was added.
    #[serde(default)]
    pub terminal_type: Option<TerminalType>,
```

**Why**: Stores which terminal opened this session so destroy knows which close script to use.

---

### Step 3: Add AppleScript close templates to operations.rs

**File**: `src/terminal/operations.rs`
**Lines**: After line 28 (after existing scripts)
**Action**: UPDATE

**Add close script constants:**
```rust
// AppleScript templates for terminal closing
const ITERM_CLOSE_SCRIPT: &str = r#"tell application "iTerm"
        if (count of windows) > 0 then
            close current window
        end if
    end tell"#;

const TERMINAL_CLOSE_SCRIPT: &str = r#"tell application "Terminal"
        if (count of windows) > 0 then
            close front window
        end if
    end tell"#;

const GHOSTTY_CLOSE_SCRIPT: &str = r#"tell application "Ghostty"
        if it is running then
            tell application "System Events"
                keystroke "w" using {command down}
            end tell
        end if
    end tell"#;
```

**Why**: Mirror pattern of launch scripts. Each terminal has its own AppleScript for closing windows.

---

### Step 4: Add close_terminal_window function to operations.rs

**File**: `src/terminal/operations.rs`
**Lines**: After build_spawn_command function (after line 99)
**Action**: UPDATE

**Add function:**
```rust
/// Close a terminal window by terminal type
///
/// Uses AppleScript (macOS) to close the frontmost/current window of the terminal.
/// This is a best-effort operation - it will not fail if the window is already closed.
#[cfg(target_os = "macos")]
pub fn close_terminal_window(terminal_type: &TerminalType) -> Result<(), TerminalError> {
    let script = match terminal_type {
        TerminalType::ITerm => ITERM_CLOSE_SCRIPT,
        TerminalType::TerminalApp => TERMINAL_CLOSE_SCRIPT,
        TerminalType::Ghostty => GHOSTTY_CLOSE_SCRIPT,
        TerminalType::Native => {
            // For Native, try to detect what terminal is running
            let detected = detect_terminal()?;
            return close_terminal_window(&detected);
        }
    };

    debug!(event = "terminal.close_started", terminal_type = %terminal_type);

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| TerminalError::AppleScriptExecution {
            message: format!("Failed to execute close script: {}", e),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Don't fail if window was already closed - this is expected behavior
        if stderr.contains("window") || stderr.contains("count") {
            debug!(
                event = "terminal.close_window_not_found",
                terminal_type = %terminal_type,
                message = "Window may have been closed manually"
            );
            return Ok(());
        }
        warn!(
            event = "terminal.close_failed",
            terminal_type = %terminal_type,
            stderr = %stderr
        );
        // Non-fatal - don't block destroy on terminal close failure
        return Ok(());
    }

    debug!(event = "terminal.close_completed", terminal_type = %terminal_type);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn close_terminal_window(_terminal_type: &TerminalType) -> Result<(), TerminalError> {
    // Terminal closing not yet implemented for non-macOS platforms
    debug!(event = "terminal.close_not_supported", platform = std::env::consts::OS);
    Ok(())
}
```

**Why**: Provides the core close functionality. Non-fatal errors ensure destroy completes even if window closing fails.

---

### Step 5: Add close_terminal handler function

**File**: `src/terminal/handler.rs`
**Lines**: After spawn_terminal function (after line 163)
**Action**: UPDATE

**Add function:**
```rust
/// Close a terminal window for a session
///
/// This is a best-effort operation used during session destruction.
/// It will not fail if the terminal window is already closed or the terminal
/// application is not running.
pub fn close_terminal(terminal_type: &TerminalType) -> Result<(), TerminalError> {
    info!(event = "terminal.close_started", terminal_type = %terminal_type);

    let result = operations::close_terminal_window(terminal_type);

    match &result {
        Ok(()) => info!(event = "terminal.close_completed", terminal_type = %terminal_type),
        Err(e) => warn!(
            event = "terminal.close_failed",
            terminal_type = %terminal_type,
            error = %e,
            message = "Continuing with destroy despite terminal close failure"
        ),
    }

    // Always return Ok - terminal close failure should not block destroy
    Ok(())
}
```

**Why**: Handler layer provides logging and ensures graceful fallback.

---

### Step 6: Add TerminalCloseFailed error variant

**File**: `src/terminal/errors.rs`
**Lines**: After line 24
**Action**: UPDATE

**Add variant:**
```rust
    #[error("Failed to close terminal window for {terminal}: {message}")]
    TerminalCloseFailed { terminal: String, message: String },
```

**And update error_code match:**
```rust
    TerminalError::TerminalCloseFailed { .. } => "TERMINAL_CLOSE_FAILED",
```

**Why**: Proper error type for close failures, even though they're non-fatal.

---

### Step 7: Update Session creation to store terminal_type

**File**: `src/sessions/handler.rs`
**Lines**: 91-110
**Action**: UPDATE

**Current code:**
```rust
    let session = Session {
        id: session_id.clone(),
        project_id: project.id,
        branch: validated.name.clone(),
        worktree_path: worktree.path,
        agent: validated.agent.clone(),
        status: SessionStatus::Active,
        created_at: now.clone(),
        last_activity: Some(now),
        port_range_start: port_start,
        port_range_end: port_end,
        port_count: config.default_port_count,
        process_id: spawn_result.process_id,
        process_name: spawn_result.process_name.clone(),
        process_start_time: spawn_result.process_start_time,
        command: spawn_result.command_executed.trim()
            .is_empty()
            .then(|| format!("{} (command not captured)", validated.agent))
            .unwrap_or_else(|| spawn_result.command_executed.clone()),
    };
```

**Required change (add terminal_type field):**
```rust
    let session = Session {
        id: session_id.clone(),
        project_id: project.id,
        branch: validated.name.clone(),
        worktree_path: worktree.path,
        agent: validated.agent.clone(),
        status: SessionStatus::Active,
        created_at: now.clone(),
        last_activity: Some(now),
        port_range_start: port_start,
        port_range_end: port_end,
        port_count: config.default_port_count,
        process_id: spawn_result.process_id,
        process_name: spawn_result.process_name.clone(),
        process_start_time: spawn_result.process_start_time,
        terminal_type: Some(spawn_result.terminal_type.clone()),
        command: spawn_result.command_executed.trim()
            .is_empty()
            .then(|| format!("{} (command not captured)", validated.agent))
            .unwrap_or_else(|| spawn_result.command_executed.clone()),
    };
```

**Why**: Captures terminal type from spawn result into session for later use during destroy.

---

### Step 8: Call close_terminal in destroy_session

**File**: `src/sessions/handler.rs`
**Lines**: 183-214 (before process kill)
**Action**: UPDATE

**Add close terminal call before killing process:**
```rust
    // 2. Close terminal window first (before killing process)
    if let Some(ref terminal_type) = session.terminal_type {
        info!(event = "session.destroy_close_terminal", terminal_type = %terminal_type);
        // Best-effort - don't fail destroy if terminal close fails
        let _ = terminal::handler::close_terminal(terminal_type);
    }

    // 3. Kill process if PID is tracked
    if let Some(pid) = session.process_id {
```

**Why**: Close terminal BEFORE killing process for cleaner UX. Terminal close is non-fatal.

---

### Step 9: Update restart_session to preserve terminal_type

**File**: `src/sessions/handler.rs`
**Lines**: 333-339
**Action**: UPDATE

**Add terminal_type to session update:**
```rust
    // 6. Update session with new process info
    session.agent = agent;
    session.process_id = spawn_result.process_id;
    session.process_name = process_name;
    session.process_start_time = process_start_time;
    session.terminal_type = Some(spawn_result.terminal_type.clone());
    session.status = SessionStatus::Active;
    session.last_activity = Some(chrono::Utc::now().to_rfc3339());
```

**Why**: Restart may use different terminal; update stored type.

---

### Step 10: Export close_terminal from terminal module

**File**: `src/terminal/mod.rs`
**Action**: UPDATE

**Ensure close_terminal is exported (if not auto-exported):**
```rust
pub use handler::{close_terminal, spawn_terminal, detect_available_terminal};
```

**Why**: Makes close_terminal available to sessions module.

---

### Step 11: Add/Update Tests

**File**: `src/terminal/operations.rs`
**Action**: UPDATE tests section

**Add test cases:**
```rust
    #[test]
    fn test_close_terminal_scripts_defined() {
        // Verify close scripts are non-empty
        assert!(!ITERM_CLOSE_SCRIPT.is_empty());
        assert!(!TERMINAL_CLOSE_SCRIPT.is_empty());
        assert!(!GHOSTTY_CLOSE_SCRIPT.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_close_terminal_window_graceful_fallback() {
        // Closing when no window exists should not error
        // This tests the graceful fallback behavior
        let result = close_terminal_window(&TerminalType::ITerm);
        // Should succeed even if no iTerm window exists
        assert!(result.is_ok());
    }
```

**File**: `src/sessions/types.rs`
**Action**: UPDATE tests section

**Add test for terminal_type serialization:**
```rust
    #[test]
    fn test_session_with_terminal_type() {
        use crate::terminal::types::TerminalType;

        let session = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 3000,
            port_range_end: 3009,
            port_count: 10,
            process_id: Some(12345),
            process_name: Some("claude-code".to_string()),
            process_start_time: Some(1234567890),
            terminal_type: Some(TerminalType::ITerm),
            command: "claude-code".to_string(),
            last_activity: Some("2024-01-01T00:00:00Z".to_string()),
        };

        // Test serialization round-trip
        let json = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.terminal_type, Some(TerminalType::ITerm));
    }

    #[test]
    fn test_session_backward_compatibility_terminal_type() {
        // Test that sessions without terminal_type field can be deserialized
        let json_without_terminal_type = r#"{
            "id": "test/branch",
            "project_id": "test",
            "branch": "branch",
            "worktree_path": "/tmp/test",
            "agent": "claude",
            "status": "Active",
            "created_at": "2024-01-01T00:00:00Z",
            "port_range_start": 3000,
            "port_range_end": 3009,
            "port_count": 10,
            "process_id": null,
            "process_name": null,
            "process_start_time": null,
            "command": "claude-code"
        }"#;

        let session: Session = serde_json::from_str(json_without_terminal_type).unwrap();
        assert_eq!(session.terminal_type, None);
    }
```

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: src/terminal/operations.rs:6-11
// Pattern for AppleScript terminal control
const ITERM_SCRIPT: &str = r#"tell application "iTerm"
        create window with default profile
        tell current session of current window
            write text "{}"
        end tell
    end tell"#;
```

```rust
// SOURCE: src/terminal/operations.rs:117-134
// Pattern for checking terminal app existence
fn app_exists_macos(app_name: &str) -> bool {
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!(r#"tell application "System Events" to exists application process "{}""#, app_name))
        .output()
        ...
}
```

```rust
// SOURCE: src/sessions/handler.rs:183-213
// Pattern for non-fatal operations during destroy (similar approach for terminal close)
match crate::process::kill_process(...) {
    Ok(()) => { info!(...); }
    Err(crate::process::ProcessError::NotFound { .. }) => { info!(...); }
    Err(e) => { return Err(...); }
}
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| Terminal window already closed manually | AppleScript checks window count before closing; gracefully handles |
| Terminal app not running | AppleScript won't error; returns silently |
| PID reuse after terminal close | Close terminal BEFORE kill_process to avoid race |
| Old sessions without terminal_type | Field is Option with serde default; None means skip close |
| Ghostty keystroke simulation | Uses Cmd+W pattern; may need testing across versions |
| Non-macOS platforms | close_terminal_window returns Ok immediately |
| Multiple windows from same terminal | Close only affects current/front window; may close wrong one |

---

## Validation

### Automated Checks

```bash
cargo check
cargo test
cargo clippy
```

### Manual Verification

1. Create shard: `shards create test --agent kiro`
2. Verify iTerm window opens with Kiro
3. Run: `shards destroy test`
4. Verify: Terminal window closes automatically
5. Verify: No error messages about terminal close
6. Test edge case: Close terminal manually, then run destroy (should not error)

---

## Scope Boundaries

**IN SCOPE:**
- macOS terminal closing (iTerm, Terminal.app, Ghostty)
- Session struct update for terminal_type persistence
- Non-fatal terminal close during destroy
- Backward compatibility with existing sessions

**OUT OF SCOPE (do not touch):**
- Linux terminal support (future Phase 2)
- Windows terminal support (future Phase 3)
- Window ID tracking (current approach uses "close front window")
- Process group management (separate approach)
- Force close option for destroy command

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-21T12:34:00Z
- **Artifact**: `.archon/artifacts/issues/issue-43.md`
