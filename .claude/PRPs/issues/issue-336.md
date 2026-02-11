# Investigation: Daemon PID fallback to 0 gives misleading output

**Issue**: #336 (https://github.com/Wirasm/kild/issues/336)
**Type**: BUG
**Investigated**: 2026-02-11

### Assessment

| Metric     | Value  | Reasoning                                                                                          |
| ---------- | ------ | -------------------------------------------------------------------------------------------------- |
| Severity   | MEDIUM | Output is misleading (PID 0 is invalid) but daemon actually works; user just sees wrong PID number |
| Complexity | LOW    | Single file change, 3 call sites in `daemon.rs`, no integration changes needed                     |
| Confidence | HIGH   | Root cause is obvious — `.unwrap_or(0)` on 3 call sites, correct pattern exists on line 22         |

---

## Problem Statement

When `read_daemon_pid()` fails (PID file missing, unreadable, or corrupt), three call sites in `crates/kild/src/commands/daemon.rs` silently fall back to PID 0 via `.unwrap_or(0)`. This produces misleading output like "Daemon started (PID: 0)" — PID 0 is the kernel's idle process on Unix, never a valid user process. The same file already has a correct pattern on line 22 using `?` for proper error propagation.

---

## Analysis

### Root Cause

WHY: User sees "Daemon started (PID: 0)"
↓ BECAUSE: `read_daemon_pid().unwrap_or(0)` falls back to 0 on any error
Evidence: `crates/kild/src/commands/daemon.rs:71` — `let pid = read_daemon_pid().unwrap_or(0);`

↓ ROOT CAUSE: Three call sites use `.unwrap_or(0)` instead of handling the error explicitly
Evidence: Lines 71, 121, 134 all use the same misleading fallback pattern

### Correct Pattern Already Exists

`crates/kild/src/commands/daemon.rs:22`:
```rust
let pid = read_daemon_pid()?;
println!("Daemon already running (PID: {})", pid);
```

This correctly propagates the error. However, `?` isn't the right fix for lines 71/121/134 because we don't want PID read failure to fail the entire command (the daemon IS running, we just can't read its PID). A `match` is appropriate here.

### Affected Files

| File                                   | Lines      | Action | Description                                     |
| -------------------------------------- | ---------- | ------ | ----------------------------------------------- |
| `crates/kild/src/commands/daemon.rs`   | 71         | UPDATE | Background start: match instead of unwrap_or(0) |
| `crates/kild/src/commands/daemon.rs`   | 121        | UPDATE | JSON status: use null PID instead of 0          |
| `crates/kild/src/commands/daemon.rs`   | 134        | UPDATE | Human status: show "unknown" instead of 0       |

### Integration Points

- No callers are affected — these are terminal output statements
- JSON consumers (`kild status --json`) will see `"pid": null` instead of `"pid": 0` when PID is unknown — this is more correct for JSON consumers

### Git History

- **Introduced**: `6f1cfa7` - 2026-02-10 - "feat: add kild-daemon crate with PTY ownership, IPC server, and session persistence (#294)"
- **Implication**: Original bug from initial daemon implementation, likely a quick shortcut during feature development

---

## Implementation Plan

### Step 1: Fix background start PID output (line 71)

**File**: `crates/kild/src/commands/daemon.rs`
**Lines**: 71-73
**Action**: UPDATE

**Current code:**
```rust
let pid = read_daemon_pid().unwrap_or(0);
println!("Daemon started (PID: {})", pid);
info!(event = "cli.daemon.start_completed", pid = pid);
```

**Required change:**
```rust
match read_daemon_pid() {
    Ok(pid) => {
        println!("Daemon started (PID: {})", pid);
        info!(event = "cli.daemon.start_completed", pid = pid);
    }
    Err(_) => {
        println!("Daemon started (PID unknown)");
        info!(event = "cli.daemon.start_completed");
    }
}
```

**Why**: When PID is unknown, say so explicitly instead of printing an invalid PID.

### Step 2: Fix JSON status PID output (lines 120-126)

**File**: `crates/kild/src/commands/daemon.rs`
**Lines**: 120-126
**Action**: UPDATE

**Current code:**
```rust
let status = if running {
    let pid = read_daemon_pid().unwrap_or(0);
    serde_json::json!({
        "running": true,
        "pid": pid,
        "socket": kild_core::daemon::socket_path().display().to_string(),
    })
```

**Required change:**
```rust
let status = if running {
    let pid = read_daemon_pid().ok();
    serde_json::json!({
        "running": true,
        "pid": pid,
        "socket": kild_core::daemon::socket_path().display().to_string(),
    })
```

**Why**: `serde_json::json!` serializes `None` as `null` and `Some(u32)` as the number — this gives JSON consumers a correct nullable PID field instead of the misleading 0.

### Step 3: Fix human-readable status PID output (lines 133-136)

**File**: `crates/kild/src/commands/daemon.rs`
**Lines**: 133-136
**Action**: UPDATE

**Current code:**
```rust
} else if running {
    let pid = read_daemon_pid().unwrap_or(0);
    println!("Daemon: running (PID: {})", pid);
    println!("Socket: {}", kild_core::daemon::socket_path().display());
```

**Required change:**
```rust
} else if running {
    match read_daemon_pid() {
        Ok(pid) => println!("Daemon: running (PID: {})", pid),
        Err(_) => println!("Daemon: running (PID unknown)"),
    }
    println!("Socket: {}", kild_core::daemon::socket_path().display());
```

**Why**: Same rationale — show "unknown" instead of an invalid PID.

---

## Patterns to Follow

**From codebase — correct pattern already in same file:**

```rust
// SOURCE: crates/kild/src/commands/daemon.rs:22-24
// Pattern for proper PID error handling
if kild_core::daemon::client::ping_daemon().unwrap_or(false) {
    let pid = read_daemon_pid()?;
    println!("Daemon already running (PID: {})", pid);
    return Ok(());
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                          | Mitigation                                                    |
| --------------------------------------- | ------------------------------------------------------------- |
| JSON consumers relying on `pid: 0`      | `null` is more correct; 0 was never valid anyway              |
| Race condition: PID file not yet written | Daemon writes PID before socket bind, so this is unlikely     |
| PID file exists but parse fails          | `read_daemon_pid()` returns descriptive error, we print "unknown" |

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

1. `kild daemon start` — should show real PID
2. `kild daemon status` — should show real PID
3. `kild daemon status --json` — should show PID as number (not null under normal conditions)
4. Remove `~/.kild/daemon.pid` while daemon runs, then `kild daemon status` — should show "PID unknown"

---

## Scope Boundaries

**IN SCOPE:**
- Replace `.unwrap_or(0)` with explicit match/ok() at 3 call sites in `daemon.rs`

**OUT OF SCOPE (do not touch):**
- `read_daemon_pid()` function signature (it's fine as-is)
- Daemon auto-start flow in kild-core (doesn't read PID)
- PID file infrastructure in kild-daemon (separate system, already has proper logging)
- Tests (no existing daemon CLI tests; adding them is out of scope for this bug fix)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-11
- **Artifact**: `.claude/PRPs/issues/issue-336.md`
