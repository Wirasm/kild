# Investigation: CRITICAL: Wrong process tracking - tracking osascript instead of agent process

**Issue**: #13 (https://github.com/Wirasm/shards/issues/13)
**Type**: BUG
**Investigated**: 2026-01-15T14:42:17+02:00

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Severity | CRITICAL | Core functionality broken - cannot determine if agents are actually running, process management features don't work, and destroy may not kill actual agent processes |
| Complexity | MEDIUM | Affects 2-3 files (terminal/handler.rs, terminal/operations.rs, possibly new platform-specific module), requires platform-specific solution but isolated to terminal spawning logic |
| Confidence | HIGH | Clear root cause identified in terminal/handler.rs:60 where child.id() captures osascript PID instead of the actual agent process running inside the terminal |

---

## Problem Statement

Shards tracks the osascript launcher process PID instead of the actual agent process (Kiro CLI, Claude, etc.), causing all sessions to show as "Stopped" immediately after creation. This breaks process management features including status checking, process killing, and session lifecycle management.

---

## Analysis

### Root Cause / Change Rationale

The terminal spawning logic captures the PID of the launcher process (osascript on macOS) rather than the actual agent process running inside the spawned terminal window.

### Evidence Chain

WHY: Sessions show as "Stopped" immediately after creation
↓ BECAUSE: `is_process_running(pid)` returns false for the tracked PID
  Evidence: `src/cli/commands.rs:102-105` - Shows "Stop(pid)" when process check fails

↓ BECAUSE: The tracked PID belongs to osascript, which exits immediately after launching the terminal
  Evidence: Issue description shows `"process_name": "osascript"` in session JSON

↓ ROOT CAUSE: Terminal handler captures the launcher process PID, not the agent process PID
  Evidence: `src/terminal/handler.rs:60` - `let process_id = child.id();` captures osascript PID

```rust
// Line 60 in src/terminal/handler.rs
let process_id = child.id();  // This is osascript's PID, not the agent's PID
```

The flow is:
1. `spawn_terminal()` builds osascript command in `operations.rs:32-52`
2. Executes osascript via `Command::new(&spawn_command[0]).spawn()` in `handler.rs:54-58`
3. Captures `child.id()` which is osascript's PID (handler.rs:60)
4. osascript exits immediately after telling Terminal/iTerm to run the command
5. The actual agent process (kiro-cli, claude, etc.) runs inside the terminal but is never tracked

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `src/terminal/handler.rs` | 60-75 | UPDATE | Change PID capture logic to find agent process instead of launcher |
| `src/terminal/operations.rs` | 14-52 | UPDATE | Add helper to extract agent command name for process search |
| `src/process/operations.rs` | NEW | CREATE | Add function to find process by name and parent relationship |

### Integration Points

- `src/sessions/handler.rs:91-93` stores the PID from SpawnResult into Session
- `src/cli/commands.rs:102-105` uses the PID to check if process is running
- `src/sessions/handler.rs:164-175` uses the PID to kill process on destroy
- `src/process/operations.rs:7-11` provides `is_process_running()` used by list command

### Git History

- **Introduced**: e9b1c8a - 2026-01-13 - "feat: Add PID tracking and process management"
- **Last modified**: 05479fd - 2026-01-13 - "fix: address code review issues - PID reuse protection"
- **Implication**: Recent feature (2 days old), introduced the bug when PID tracking was first implemented

---

## Implementation Plan

### Step 1: Add process search by name function

**File**: `src/process/operations.rs`
**Lines**: After line 75 (after get_process_info function)
**Action**: CREATE

**Required change:**
```rust
/// Find a process by name, optionally filtering by command line pattern
pub fn find_process_by_name(
    name_pattern: &str,
    command_pattern: Option<&str>,
) -> Result<Option<ProcessInfo>, ProcessError> {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    for (pid, process) in system.processes() {
        let process_name = process.name().to_string_lossy().to_string();
        
        // Check if name matches
        if !process_name.contains(name_pattern) {
            continue;
        }
        
        // If command pattern specified, check command line
        if let Some(cmd_pattern) = command_pattern {
            let cmd_line = process.cmd().join(" ");
            if !cmd_line.contains(cmd_pattern) {
                continue;
            }
        }
        
        return Ok(Some(ProcessInfo {
            pid: Pid::from_raw(pid.as_u32()),
            name: process_name,
            status: ProcessStatus::from(process.status()),
            start_time: process.start_time(),
        }));
    }
    
    Ok(None)
}
```

**Why**: Need ability to search for agent process by name after terminal spawns

---

### Step 2: Extract agent command name helper

**File**: `src/terminal/operations.rs`
**Lines**: After line 95 (after applescript_escape function)
**Action**: CREATE

**Required change:**
```rust
/// Extract the executable name from a command string
/// Examples: "kiro-cli chat" -> "kiro-cli", "claude-code" -> "claude-code"
pub fn extract_command_name(command: &str) -> String {
    command
        .trim()
        .split_whitespace()
        .next()
        .unwrap_or(command)
        .to_string()
}
```

**Why**: Need to extract agent executable name from full command for process search

---

### Step 3: Update terminal handler to find agent process

**File**: `src/terminal/handler.rs`
**Lines**: 60-75
**Action**: UPDATE

**Current code:**
```rust
let child = cmd.spawn().map_err(|e| TerminalError::SpawnFailed {
    message: format!("Failed to execute {}: {}", spawn_command[0], e),
})?;

let process_id = child.id();

// Capture process metadata immediately for PID reuse protection
let (process_name, process_start_time) = if let Ok(info) = crate::process::get_process_info(process_id) {
    (Some(info.name), Some(info.start_time))
} else {
    (None, None)
};
```

**Required change:**
```rust
let _child = cmd.spawn().map_err(|e| TerminalError::SpawnFailed {
    message: format!("Failed to execute {}: {}", spawn_command[0], e),
})?;

// Wait briefly for terminal to spawn the agent process
std::thread::sleep(std::time::Duration::from_millis(500));

// Extract agent command name for process search
let agent_name = operations::extract_command_name(command);

// Try to find the actual agent process
let (process_id, process_name, process_start_time) = 
    if let Ok(Some(info)) = crate::process::find_process_by_name(&agent_name, Some(command)) {
        (Some(info.pid.as_u32()), Some(info.name), Some(info.start_time))
    } else {
        debug!(
            event = "terminal.agent_process_not_found",
            agent_name = agent_name,
            command = command
        );
        (None, None, None)
    };
```

**Why**: Instead of tracking osascript, wait for terminal to spawn agent and find it by name

---

### Step 4: Add test for process finding

**File**: `src/process/operations.rs`
**Lines**: After line 119 (in tests module)
**Action**: CREATE

**Test cases to add:**
```rust
#[test]
fn test_find_process_by_name() {
    // Spawn a test process
    let mut child = Command::new("sleep")
        .arg("10")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn test process");

    // Give it a moment to start
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Should find it by name
    let result = find_process_by_name("sleep", None);
    assert!(result.is_ok());
    assert!(result.unwrap().is_some());

    // Clean up
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_find_process_by_name_not_found() {
    let result = find_process_by_name("nonexistent-process-xyz", None);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}
```

---

### Step 5: Add test for command name extraction

**File**: `src/terminal/operations.rs`
**Lines**: After line 195 (in tests module)
**Action**: CREATE

**Test cases to add:**
```rust
#[test]
fn test_extract_command_name() {
    assert_eq!(extract_command_name("kiro-cli chat"), "kiro-cli");
    assert_eq!(extract_command_name("claude-code"), "claude-code");
    assert_eq!(extract_command_name("  cc  "), "cc");
    assert_eq!(extract_command_name("echo hello world"), "echo");
}
```

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: src/process/operations.rs:67-75
// Pattern for getting process info with sysinfo
let mut system = System::new();
let pid_obj = SysinfoPid::from_u32(pid);
system.refresh_processes(ProcessesToUpdate::Some(&[pid_obj]), true);

match system.process(pid_obj) {
    Some(process) => Ok(ProcessInfo {
        pid: Pid::from_raw(pid),
        name: process.name().to_string_lossy().to_string(),
        status: ProcessStatus::from(process.status()),
        start_time: process.start_time(),
    }),
    None => Err(ProcessError::NotFound { pid }),
}
```

```rust
// SOURCE: src/terminal/handler.rs:14-17
// Pattern for structured logging
info!(
    event = "terminal.spawn_started",
    working_directory = %working_directory.display(),
    command = command
);
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| Agent process not started within 500ms | Log warning and store None for PID - session still works, just no process tracking |
| Multiple processes with same name | Use command pattern matching to find the right one |
| Agent command name extraction fails | Fall back to None PID - graceful degradation |
| Process search is expensive | Only search once at spawn time, cache result in session |
| Race condition - process exits before we find it | Acceptable - same as current behavior, just log it |

---

## Validation

### Automated Checks

```bash
cargo test
cargo clippy
cargo fmt --check
```

### Manual Verification

1. Create a shard: `shards create test-branch --agent kiro`
2. Immediately run: `shards list`
3. Verify process shows as "Run(PID)" not "Stop(PID)"
4. Check session JSON file contains correct process_name (e.g., "kiro-cli" not "osascript")
5. Verify `shards destroy` kills the actual agent process
6. Test with different agents (claude, gemini) to ensure name extraction works

---

## Scope Boundaries

**IN SCOPE:**
- Fix PID tracking to capture agent process instead of launcher
- Add process search by name functionality
- Add command name extraction helper
- Update terminal handler to use new approach
- Add tests for new functions

**OUT OF SCOPE (do not touch):**
- Linux/Windows terminal spawning (focus on macOS osascript issue)
- PTY integration or output parsing
- Session persistence format changes
- Process monitoring/heartbeat system
- Alternative terminal detection methods

---

## Metadata

- **Investigated by**: Claude (Kiro CLI)
- **Timestamp**: 2026-01-15T14:42:17+02:00
- **Artifact**: `.archon/artifacts/issues/issue-13.md`
