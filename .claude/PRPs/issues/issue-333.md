# Investigation: mutex unwrap in get_process_metrics can panic in production

**Issue**: #333 (https://github.com/Wirasm/kild/issues/333)
**Type**: BUG
**Investigated**: 2026-02-11

### Assessment

| Metric     | Value  | Reasoning                                                                                                                |
| ---------- | ------ | ------------------------------------------------------------------------------------------------------------------------ |
| Severity   | MEDIUM | Mutex poisoning requires a prior panic in sysinfo (unlikely), and the caller already handles errors gracefully via Option |
| Complexity | LOW    | Single line change in one file, existing error variant `ProcessError::SystemError` can be reused                         |
| Confidence | HIGH   | Clear root cause at `operations.rs:136`, established fix pattern from `kild-ui/terminal/state.rs:332`                    |

---

## Problem Statement

`crates/kild-core/src/process/operations.rs:136` uses `.unwrap()` on `SYSTEM.lock()`, which is the only production `.unwrap()` on a fallible operation in the codebase. If the mutex is poisoned (another thread panicked while holding the lock), this panics instead of returning an error. The caller (`health/handler.rs:82`) already handles `Result` gracefully, so a proper error return would be handled without issue.

---

## Analysis

### Root Cause

WHY: `get_process_metrics()` panics if the SYSTEM mutex is poisoned
↓ BECAUSE: Line 136 uses `.unwrap()` instead of error handling
Evidence: `crates/kild-core/src/process/operations.rs:136` - `let mut system = SYSTEM.lock().unwrap();`

↓ BECAUSE: Original implementation (commit `568ed29`, PR #18) didn't convert mutex errors to `ProcessError`
Evidence: `git blame -L 136,136 crates/kild-core/src/process/operations.rs` - introduced 2026-01-19

↓ ROOT CAUSE: Missing `.map_err()` conversion from `PoisonError` to `ProcessError::SystemError`
Evidence: `ProcessError::SystemError` variant already exists at `crates/kild-core/src/process/errors.rs:13` but is unused for this case

### Evidence Chain

The function already returns `Result<ProcessMetrics, ProcessError>`:
```rust
// crates/kild-core/src/process/operations.rs:132-150
pub fn get_process_metrics(pid: u32) -> Result<ProcessMetrics, ProcessError> {
    let pid_obj = SysinfoPid::from_u32(pid);
    let mut system = SYSTEM.lock().unwrap(); // ← panics instead of Err
    // ...
}
```

The caller already handles errors gracefully:
```rust
// crates/kild-core/src/health/handler.rs:81-94
fn get_metrics_for_pid(pid: u32, branch: &str) -> Option<ProcessMetrics> {
    match process::get_process_metrics(pid) {
        Ok(metrics) => Some(metrics),
        Err(e) => {
            warn!(event = "core.health.process_metrics_failed", pid, error = %e);
            None
        }
    }
}
```

The codebase has an established pattern for mutex lock error handling:
```rust
// crates/kild-ui/src/terminal/state.rs:332-335
let mut writer = self.pty_writer.lock().map_err(|e| {
    tracing::error!(event = "ui.terminal.writer_lock_failed", error = %e);
    TerminalError::WriterLockPoisoned
})?;
```

### Affected Files

| File                                              | Lines   | Action | Description                                     |
| ------------------------------------------------- | ------- | ------ | ----------------------------------------------- |
| `crates/kild-core/src/process/operations.rs`      | 136     | UPDATE | Replace `.unwrap()` with `.map_err()` + `?`     |

### Integration Points

- `crates/kild-core/src/health/handler.rs:82` - sole caller via `get_metrics_for_pid()`
- `crates/kild-core/src/process/mod.rs:8` - re-exports `get_process_metrics`
- `crates/kild/src/commands/health.rs:82,102` - CLI entry points

### Git History

- **Introduced**: `568ed29` - 2026-01-19 - "Add shards health command with process monitoring" (PR #18)
- **Last modified**: `160314d` - Rebrand (no logic change)
- **Implication**: Original bug, present since function was introduced

---

## Implementation Plan

### Step 1: Replace `.unwrap()` with `.map_err()` on mutex lock

**File**: `crates/kild-core/src/process/operations.rs`
**Lines**: 136
**Action**: UPDATE

**Current code:**
```rust
// Line 135-136
    // Use shared system instance to prevent memory leaks
    let mut system = SYSTEM.lock().unwrap();
```

**Required change:**
```rust
    // Use shared system instance to prevent memory leaks
    let mut system = SYSTEM.lock().map_err(|e| {
        error!(event = "core.process.metrics_lock_failed", pid = pid, error = %e);
        ProcessError::SystemError {
            message: format!("Failed to acquire process metrics lock: {e}"),
        }
    })?;
```

**Why**: Converts panic into a proper `ProcessError::SystemError` that the caller already handles. Logs the error at `error!` level following the codebase pattern from `kild-ui/src/terminal/state.rs:332-335`.

**Note**: A `use tracing::error;` import is needed if not already present (check existing imports at top of file).

---

## Patterns to Follow

**From codebase - mirror this exactly:**

```rust
// SOURCE: crates/kild-ui/src/terminal/state.rs:332-335
// Pattern for mutex lock error handling with map_err
let mut writer = self.pty_writer.lock().map_err(|e| {
    tracing::error!(event = "ui.terminal.writer_lock_failed", error = %e);
    TerminalError::WriterLockPoisoned
})?;
```

**Error variant to use:**

```rust
// SOURCE: crates/kild-core/src/process/errors.rs:13-14
#[error("System error: {message}")]
SystemError { message: String },
```

---

## Edge Cases & Risks

| Risk/Edge Case                    | Mitigation                                                                                     |
| --------------------------------- | ---------------------------------------------------------------------------------------------- |
| Mutex poisoned permanently        | Each call returns `Err` instead of panicking; health monitoring degrades gracefully via `None`  |
| sysinfo panic while lock held     | Now caller gets `Err` on next call instead of cascading panic                                  |
| Missing `tracing::error` import   | Check imports at top of `operations.rs`; add if needed                                         |

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

1. `cargo test -p kild-core -- process` - process module tests pass
2. Confirm no other `.unwrap()` on fallible mutex operations in production code

---

## Scope Boundaries

**IN SCOPE:**
- Replace `.unwrap()` with `.map_err()` at `operations.rs:136`
- Add `tracing::error` import if not present

**OUT OF SCOPE (do not touch):**
- Other process functions that create local `System` instances (no mutex, no risk)
- Test code `.unwrap()` usage (acceptable in tests)
- Adding new tests for mutex poisoning (hard to reliably test, low value)
- Refactoring other parts of the process module

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-11
- **Artifact**: `.claude/PRPs/issues/issue-333.md`
