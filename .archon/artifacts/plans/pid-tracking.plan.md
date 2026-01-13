# Feature: PID Tracking and Process Management

## Summary

Implement comprehensive process tracking for spawned terminals to enable lifecycle management, prevent stale processes, and provide reliable cleanup. This adds PID storage to sessions, process health monitoring, and automatic cleanup when sessions are destroyed.

## User Story

As a developer using Shards
I want automatic tracking of terminal processes and their associated worktrees
So that I can avoid stale processes and have reliable cleanup when sessions end

## Problem Statement

Currently, Shards spawns terminals but has no visibility into their lifecycle. This leads to:
- Stale terminal processes when sessions are destroyed
- No way to detect if an agent is still running
- No automatic cleanup of orphaned worktrees
- Resource leaks from abandoned processes

## Solution Statement

Add comprehensive process tracking by extending the session model with PID storage, implementing cross-platform process management using the `sysinfo` crate, and providing cleanup commands that synchronize process state with worktree state.

## Metadata

| Field            | Value                                             |
| ---------------- | ------------------------------------------------- |
| Type             | NEW_CAPABILITY                                    |
| Complexity       | HIGH                                              |
| Systems Affected | sessions, terminal, git, CLI                      |
| Dependencies     | sysinfo ^0.37.2                                  |
| Estimated Tasks  | 8                                                 |

---

## UX Design

### Before State
```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              CURRENT STATE                                      │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐              │
│   │ shards      │ ──────► │ Terminal    │ ──────► │ Agent       │              │
│   │ create      │         │ Spawned     │         │ Running     │              │
│   └─────────────┘         └─────────────┘         └─────────────┘              │
│                                   │                       │                     │
│                                   ▼                       ▼                     │
│                          ┌─────────────┐         ┌─────────────┐              │
│                          │ PID Lost    │         │ No Tracking │              │
│                          │ (dropped)   │         │ Possible    │              │
│                          └─────────────┘         └─────────────┘              │
│                                                                                 │
│   USER_FLOW: Create shard → Terminal opens → No visibility into process        │
│   PAIN_POINT: Can't tell if agent is running, cleanup leaves stale processes   │
│   DATA_FLOW: Session created → PID discarded → No process management           │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────────┐
│                               AFTER STATE                                      │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐              │
│   │ shards      │ ──────► │ Terminal    │ ──────► │ Agent       │              │
│   │ create      │         │ Spawned     │         │ Running     │              │
│   └─────────────┘         └─────────────┘         └─────────────┘              │
│                                   │                       │                     │
│                                   ▼                       ▼                     │
│                          ┌─────────────┐         ┌─────────────┐              │
│                          │ PID Tracked │         │ Health      │              │
│                          │ in Session  │         │ Monitored   │              │
│                          └─────────────┘         └─────────────┘              │
│                                   │                       │                     │
│                                   ▼                       ▼                     │
│                          ┌─────────────┐         ┌─────────────┐              │
│                          │ shards list │         │ shards      │              │
│                          │ shows status│         │ destroy     │              │
│                          └─────────────┘         │ kills PID   │              │
│                                                  └─────────────┘              │
│                                                                                 │
│   USER_FLOW: Create shard → PID tracked → Status visible → Clean destroy       │
│   VALUE_ADD: Process visibility, automatic cleanup, resource management        │
│   DATA_FLOW: Session created → PID stored → Health checked → Cleanup managed   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Interaction Changes
| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| `shards create` | Terminal spawns, PID lost | Terminal spawns, PID tracked | Can monitor process health |
| `shards list` | Shows session metadata only | Shows process status (running/stopped) | Visibility into actual state |
| `shards destroy` | Removes worktree only | Kills process + removes worktree | Complete cleanup |
| `shards status <name>` | Command doesn't exist | Shows detailed process info | Debugging capability |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `src/sessions/types.rs` | 1-50 | Session struct to EXTEND with PID |
| P0 | `src/terminal/handler.rs` | 1-100 | Terminal spawning to MODIFY for PID capture |
| P1 | `src/sessions/operations.rs` | 1-200 | Session persistence patterns to MIRROR |
| P1 | `src/sessions/errors.rs` | 1-40 | Error patterns to EXTEND |
| P2 | `src/terminal/types.rs` | 1-30 | Types to EXTEND for PID tracking |

**External Documentation:**
| Source | Section | Why Needed |
|--------|---------|------------|
| [sysinfo docs v0.37.2](https://docs.rs/sysinfo/0.37.2/sysinfo/) | Process struct | Cross-platform process management |
| [std::process::Child](https://doc.rust-lang.org/std/process/struct.Child.html) | id() method | Getting PID from spawned process |

---

## Patterns to Mirror

**SESSION_STRUCTURE:**
```rust
// SOURCE: src/sessions/types.rs:10-25
// EXTEND THIS PATTERN:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub project_id: String,
    pub branch: String,
    pub worktree_path: PathBuf,
    pub agent: String,
    pub status: SessionStatus,
    pub created_at: String,
    // ADD: pub process_id: Option<u32>,
}
```

**ERROR_HANDLING:**
```rust
// SOURCE: src/sessions/errors.rs:5-20
// EXTEND THIS PATTERN:
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session '{name}' not found")]
    NotFound { name: String },
    // ADD: ProcessNotFound, ProcessKillFailed, etc.
}
```

**LOGGING_PATTERN:**
```rust
// SOURCE: src/sessions/handler.rs:25-35
// COPY THIS PATTERN:
info!(
    event = "session.create_completed",
    session_id = session_id,
    branch = validated.name,
    agent = session.agent
);
// ADD: process.spawn_completed, process.kill_started, etc.
```

**TERMINAL_SPAWNING:**
```rust
// SOURCE: src/terminal/handler.rs:spawn_terminal()
// MODIFY THIS PATTERN:
let _child = cmd.spawn().map_err(|e| TerminalError::SpawnFailed {
    message: format!("Failed to execute {}: {}", spawn_command[0], e),
})?;
// CHANGE TO: let child = cmd.spawn()...?; let pid = child.id();
```

**ATOMIC_FILE_OPERATIONS:**
```rust
// SOURCE: src/sessions/operations.rs:save_session_to_file()
// COPY THIS PATTERN:
let temp_file = session_file.with_extension("json.tmp");
fs::write(&temp_file, session_json)?;
fs::rename(&temp_file, &session_file)?; // Atomic
```

**VALIDATION_PATTERN:**
```rust
// SOURCE: src/sessions/operations.rs:validate_session_structure()
// MIRROR THIS PATTERN:
fn validate_session_structure(session: &Session) -> Result<(), SessionError> {
    if session.id.is_empty() {
        return Err(SessionError::InvalidStructure { 
            field: "id".to_string() 
        });
    }
    // ADD: PID validation logic
    Ok(())
}
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `Cargo.toml` | UPDATE | Add sysinfo dependency |
| `src/sessions/types.rs` | UPDATE | Add process_id field to Session |
| `src/sessions/errors.rs` | UPDATE | Add process-related errors |
| `src/terminal/handler.rs` | UPDATE | Capture and return PID from spawn |
| `src/terminal/types.rs` | UPDATE | Add PID to SpawnResult |
| `src/process/mod.rs` | CREATE | Process management module |
| `src/process/operations.rs` | CREATE | Process health checking logic |
| `src/process/errors.rs` | CREATE | Process-specific errors |
| `src/cli/commands.rs` | UPDATE | Add status command, extend destroy |
| `src/lib.rs` | UPDATE | Export process module |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **GUI process monitoring** - CLI only, no visual dashboard
- **Process resource monitoring** - Only track alive/dead, not CPU/memory usage
- **Multi-process agents** - Track only the main terminal process, not child processes
- **Process restart/recovery** - Only track and kill, no automatic restart
- **Historical process data** - No logging of process lifetime statistics

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `Cargo.toml` (add dependency)

- **ACTION**: ADD sysinfo dependency
- **IMPLEMENT**: Add `sysinfo = "0.37.2"` to dependencies
- **VALIDATE**: `cargo check` - dependency resolves correctly

### Task 2: UPDATE `src/sessions/types.rs` (extend Session)

- **ACTION**: ADD process_id field to Session struct
- **IMPLEMENT**: `pub process_id: Option<u32>,` field
- **MIRROR**: Existing Session struct pattern with Serialize/Deserialize
- **GOTCHA**: Use Option<u32> to handle cases where PID is unknown
- **VALIDATE**: `cargo check` - types compile correctly

### Task 3: UPDATE `src/sessions/errors.rs` (add process errors)

- **ACTION**: ADD process-related error variants
- **IMPLEMENT**: ProcessNotFound, ProcessKillFailed, ProcessAccessDenied
- **MIRROR**: `src/sessions/errors.rs:5-20` - existing error pattern
- **PATTERN**: Include context fields like PID, error message
- **VALIDATE**: `cargo check` - error types compile

### Task 4: CREATE `src/process/mod.rs` (process module)

- **ACTION**: CREATE process management module
- **IMPLEMENT**: Public exports for operations and errors
- **MIRROR**: `src/sessions/mod.rs:1-10` - module export pattern
- **IMPORTS**: Re-export key types and functions
- **VALIDATE**: `cargo check` - module structure correct

### Task 5: CREATE `src/process/operations.rs` (process logic)

- **ACTION**: CREATE pure process management logic
- **IMPLEMENT**: is_process_running(pid), kill_process(pid), get_process_info(pid)
- **DEPENDENCIES**: `use sysinfo::{System, Pid, ProcessesToUpdate};`
- **PATTERN**: Pure functions, no I/O, return Results
- **GOTCHA**: sysinfo requires System::refresh_processes() before checking
- **VALIDATE**: `cargo check && cargo test src/process/`

### Task 6: CREATE `src/process/errors.rs` (process errors)

- **ACTION**: CREATE process-specific error types
- **IMPLEMENT**: ProcessError enum with NotFound, KillFailed, AccessDenied
- **MIRROR**: `src/sessions/errors.rs:1-40` - error structure pattern
- **PATTERN**: Include PID in error context, implement ShardsError trait
- **VALIDATE**: `cargo check` - error types integrate correctly

### Task 7: UPDATE `src/terminal/types.rs` (extend SpawnResult)

- **ACTION**: ADD process_id field to SpawnResult
- **IMPLEMENT**: `pub process_id: Option<u32>,` in SpawnResult struct
- **MIRROR**: Existing SpawnResult pattern
- **GOTCHA**: Use Option to handle platform-specific spawn failures
- **VALIDATE**: `cargo check` - terminal types compile

### Task 8: UPDATE `src/terminal/handler.rs` (capture PID)

- **ACTION**: MODIFY spawn_terminal to capture and return PID
- **IMPLEMENT**: Store Child, get PID with child.id(), return in SpawnResult
- **MIRROR**: Existing spawn pattern but don't drop Child immediately
- **IMPORTS**: `use std::process::Child;`
- **GOTCHA**: Child.id() returns u32, store before dropping Child
- **VALIDATE**: `cargo check && cargo test src/terminal/`

### Task 9: UPDATE `src/sessions/handler.rs` (store PID)

- **ACTION**: MODIFY create_session to store PID from terminal spawn
- **IMPLEMENT**: Extract PID from SpawnResult, store in Session
- **MIRROR**: Existing session creation pattern
- **LOGGING**: Add process.spawn_completed event with PID
- **VALIDATE**: `cargo check && cargo test src/sessions/`

### Task 10: UPDATE `src/cli/commands.rs` (add status command)

- **ACTION**: ADD status command and extend destroy command
- **IMPLEMENT**: status_command() shows process health, destroy kills PID
- **MIRROR**: Existing command patterns in same file
- **DEPENDENCIES**: Import process operations for health checking
- **VALIDATE**: `cargo check && cargo run -- --help` shows new command

---

## Testing Strategy

### Unit Tests to Write

| Test File | Test Cases | Validates |
|-----------|------------|-----------|
| `src/process/tests/operations.test.rs` | is_process_running, kill_process | Process management logic |
| `src/sessions/tests/types.test.rs` | Session serialization with PID | Extended session structure |
| `src/terminal/tests/handler.test.rs` | PID capture from spawn | Terminal PID tracking |

### Edge Cases Checklist

- [ ] PID not available (spawn failed)
- [ ] Process already dead when checking
- [ ] Permission denied when killing process
- [ ] Invalid PID values (0, negative)
- [ ] Cross-platform PID differences
- [ ] Session file corruption with PID field

---

## Validation Commands

### Level 1: STATIC_ANALYSIS
```bash
cargo check && cargo clippy -- -D warnings
```
**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS
```bash
cargo test src/process/ && cargo test src/sessions/ && cargo test src/terminal/
```
**EXPECT**: All tests pass, new PID functionality covered

### Level 3: FULL_SUITE
```bash
cargo test && cargo build --release
```
**EXPECT**: All tests pass, release build succeeds

### Level 4: INTEGRATION_VALIDATION
```bash
# Test PID tracking end-to-end
cargo run -- create test-pid --agent "sleep 30"
cargo run -- list  # Should show process status
cargo run -- status test-pid  # Should show PID info
cargo run -- destroy test-pid  # Should kill process
```
**EXPECT**: PID tracked through full lifecycle

### Level 5: CROSS_PLATFORM_VALIDATION
Test on multiple platforms:
- [ ] macOS: Terminal.app spawning with PID capture
- [ ] Linux: gnome-terminal spawning with PID capture  
- [ ] Windows: cmd spawning with PID capture (if supported)

---

## Acceptance Criteria

- [ ] Sessions store PID when terminal is spawned successfully
- [ ] `shards list` shows process status (running/stopped) for each session
- [ ] `shards status <name>` command shows detailed process information
- [ ] `shards destroy <name>` kills the process before removing worktree
- [ ] Process health checking works cross-platform via sysinfo
- [ ] All existing functionality continues to work unchanged
- [ ] Level 1-4 validation commands pass with exit 0
- [ ] No memory leaks from retained Child processes

---

## Completion Checklist

- [ ] All tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] Level 1: `cargo check && cargo clippy` passes
- [ ] Level 2: `cargo test {modules}` passes
- [ ] Level 3: `cargo test && cargo build --release` succeeds
- [ ] Level 4: Integration test scenario passes
- [ ] Level 5: Cross-platform validation passes
- [ ] All acceptance criteria met
- [ ] No regressions in existing commands

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Cross-platform PID differences | HIGH | MEDIUM | Use sysinfo crate for abstraction, test on all platforms |
| Process already dead when tracking | MEDIUM | LOW | Handle ProcessNotFound gracefully, update session status |
| Permission denied killing process | MEDIUM | MEDIUM | Catch permission errors, provide clear user feedback |
| Child process drops PID too early | HIGH | HIGH | Store PID immediately after spawn, before any other operations |
| Session file corruption with new field | LOW | HIGH | Use atomic writes, validate structure on load |

---

## Notes

**Design Decision**: Using `Option<u32>` for PID storage allows handling cases where PID capture fails or process is already dead. This maintains backward compatibility with existing sessions.

**Performance Consideration**: sysinfo requires System::refresh_processes() which can be expensive. Cache System instance and refresh only when needed.

**Cross-Platform**: sysinfo handles platform differences, but terminal spawning mechanisms vary. Focus on getting PID from std::process::Child which is consistent.

**Future Enhancement**: This foundation enables future features like process resource monitoring, automatic restart, and process tree tracking.
