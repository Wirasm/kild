# Investigation: kild open on active session spawns duplicate agent, overwrites agent_session_id

**Issue**: #599 (https://github.com/Wirasm/kild/issues/599)
**Type**: BUG
**Investigated**: 2026-02-26T12:00:00Z

### Assessment

| Metric     | Value    | Reasoning                                                                                                                                           |
| ---------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Severity   | HIGH     | Breaks resume permanently, spawns competing agents in same worktree, requires manual JSON patching to recover — no workaround in the normal workflow |
| Complexity | LOW      | 2 files need changes (open.rs + errors.rs), isolated guard check with clear precedent in create.rs, no architectural changes required                |
| Confidence | HIGH     | Root cause is obvious from code (no status check in open.rs), confirmed by user with reproduction steps and manual recovery                         |

---

## Problem Statement

`kild open` on an already-active session unconditionally spawns a second agent process and overwrites `agent_session_id` with a new UUID. This creates competing agents in the same worktree and permanently breaks `--resume` for the original conversation. The only recovery is manual JSON patching.

---

## Analysis

### Root Cause

WHY: `kild open foo` on an active session spawns a duplicate agent
↓ BECAUSE: `open_session()` has no guard checking session status before spawning
Evidence: `crates/kild-core/src/sessions/open.rs:74-93` — after finding the session and checking worktree existence, execution flows directly to agent spawning with no status check

↓ BECAUSE: `add_agent()` is a bare `Vec::push` with no deduplication
Evidence: `crates/kild-core/src/sessions/types/session.rs:196-198`:
```rust
pub fn add_agent(&mut self, agent: AgentProcess) {
    self.agents.push(agent);
}
```

↓ BECAUSE: `rotate_agent_session_id()` unconditionally overwrites the live session ID
Evidence: `crates/kild-core/src/sessions/types/session.rs:214-225`:
```rust
pub fn rotate_agent_session_id(&mut self, new_id: String) -> bool {
    let rotated = if let Some(prev) = self.agent_session_id.take()
        && prev != new_id
    {
        self.agent_session_id_history.push(prev);
        true
    } else {
        false
    };
    self.agent_session_id = Some(new_id);
    rotated
}
```

↓ ROOT CAUSE: `open_session()` lacks the same kind of guard that `create_session()` has
Evidence: `crates/kild-core/src/sessions/create.rs:142-153` has `AlreadyExists` check; `open.rs` has no equivalent `AlreadyActive` check

### Evidence Chain

The `--all` CLI path correctly filters for stopped sessions only (`commands/open.rs:94-97`):
```rust
let stopped: Vec<_> = sessions
    .into_iter()
    .filter(|s| s.status == SessionStatus::Stopped)
    .collect();
```

But the single-branch path at `commands/open.rs:35` calls `open_session()` directly with no pre-check.

The daemon liveness check exists in `list.rs:58-100` (`sync_daemon_session_status()`) and can distinguish truly-running sessions from stale-active ones, but it is never called by `open_session()`.

### Affected Files

| File                                                    | Lines   | Action | Description                                                              |
| ------------------------------------------------------- | ------- | ------ | ------------------------------------------------------------------------ |
| `crates/kild-core/src/sessions/errors.rs`               | 4-8     | UPDATE | Add `AlreadyActive` error variant with actionable message                |
| `crates/kild-core/src/sessions/open.rs`                 | 74-93   | UPDATE | Add active-session guard after finding session, before spawn             |
| `crates/kild-core/src/sessions/open.rs`                 | 342+    | UPDATE | Add tests for the new guard                                             |

### Integration Points

- `crates/kild/src/commands/open.rs:35` — CLI single-branch path calls `open_session()` directly; will surface the new error naturally
- `crates/kild/src/commands/open.rs:85` — CLI `--all` path already filters for `Stopped`; unaffected
- `crates/kild-core/src/sessions/handler.rs:10` — re-exports `open_session`; no change needed
- `crates/kild-core/src/sessions/list.rs:58` — `sync_daemon_session_status()` can be reused for liveness check
- `crates/kild-ui/src/actions.rs` — UI dispatches `Command::Open`; will surface the error via `DispatchError`

### Git History

- **Last modified**: `8278c8a` - "fix(session): preserve agent_session_id on fresh open, clear idle gate on initial-prompt (#575)" — added the `rotate_agent_session_id` mechanism but did not add the active guard
- **Implication**: The rotation mechanism was added as a safety net but the root cause (no guard on active sessions) was not addressed

---

## Implementation Plan

### Step 1: Add `AlreadyActive` error variant

**File**: `crates/kild-core/src/sessions/errors.rs`
**Lines**: 8-9 (after `AlreadyExists`)
**Action**: UPDATE

**Current code:**
```rust
#[error(
    "Kild '{name}' already exists.\n  Resume: kild open {name}\n  Remove: kild destroy {name}"
)]
AlreadyExists { name: String },
```

**Required change:** Add new variant after `AlreadyExists`:

```rust
#[error(
    "Kild '{name}' already has a running agent.\n  To view it:             kild attach {name}\n  To send a message:      kild inject {name} \"...\"\n  To stop and reopen:     kild stop {name} && kild open {name}"
)]
AlreadyActive { name: String },
```

Also add the error code mapping in `error_code()`:
```rust
SessionError::AlreadyActive { .. } => "SESSION_ALREADY_ACTIVE",
```

And add it to the `is_user_error()` match:
```rust
| SessionError::AlreadyActive { .. }
```

**Why**: The error message provides three actionable alternatives, matching the pattern from the issue comments. This is critical for fleet automation where Honryū reads error messages to decide next actions.

---

### Step 2: Add active-session guard in `open_session()`

**File**: `crates/kild-core/src/sessions/open.rs`
**Lines**: 88-93 (after worktree existence check, before agent resolution)
**Action**: UPDATE

**Current code:**
```rust
// 2. Verify worktree still exists
if !session.worktree_path.exists() {
    return Err(SessionError::WorktreeNotFound {
        path: session.worktree_path.clone(),
    });
}

// 3. Determine agent and command based on OpenMode
```

**Required change:** Insert active-session guard between steps 2 and 3:

```rust
// 2. Verify worktree still exists
if !session.worktree_path.exists() {
    return Err(SessionError::WorktreeNotFound {
        path: session.worktree_path.clone(),
    });
}

// 2b. Guard: refuse to spawn if session already has a running agent.
// Sync with daemon first to avoid blocking on stale-active sessions
// whose PTY has exited without a `kild stop`.
if session.status == SessionStatus::Active && session.has_agents() {
    // For daemon sessions, verify the agent is truly running before refusing.
    // A stale-active session (daemon PTY died) should be allowed to reopen.
    let truly_active = if session
        .latest_agent()
        .and_then(|a| a.daemon_session_id())
        .is_some()
    {
        !super::list::sync_daemon_session_status(&mut session)
    } else {
        // Non-daemon (terminal) sessions: trust the stored status.
        // Terminal sessions have no reliable liveness check.
        true
    };

    if truly_active {
        warn!(
            event = "core.session.open_rejected_already_active",
            branch = name,
            agent_count = session.agent_count(),
            "Session already has running agents — refusing duplicate spawn"
        );
        return Err(SessionError::AlreadyActive {
            name: name.to_string(),
        });
    }

    // Session was stale-active — daemon sync marked it Stopped.
    // Persist the corrected status before proceeding with the open.
    info!(
        event = "core.session.open_stale_active_synced",
        branch = name,
        "Stale-active session synced to Stopped, proceeding with open"
    );
    persistence::save_session_to_file(&session, &config.sessions_dir())?;
}

// 3. Determine agent and command based on OpenMode
```

**Why**: This guard mirrors the `AlreadyExists` guard in `create_session()`. The daemon liveness check via `sync_daemon_session_status()` prevents false positives where the session JSON says `Active` but the daemon PTY has already exited. Non-daemon (terminal) sessions trust the stored status since there's no reliable PID-based liveness check.

---

### Step 3: Add tests for the new guard

**File**: `crates/kild-core/src/sessions/errors.rs`
**Lines**: end of `mod tests`
**Action**: UPDATE

**Test cases to add:**

```rust
#[test]
fn test_already_active_error() {
    let error = SessionError::AlreadyActive {
        name: "feature-auth".to_string(),
    };
    assert!(error.to_string().contains("already has a running agent"));
    assert!(error.to_string().contains("kild attach feature-auth"));
    assert!(error.to_string().contains("kild inject feature-auth"));
    assert_eq!(error.error_code(), "SESSION_ALREADY_ACTIVE");
    assert!(error.is_user_error());
}
```

**File**: `crates/kild-core/src/sessions/open.rs`
**Lines**: end of `mod tests`
**Action**: UPDATE

**Test case: active guard rejects duplicate spawn**

```rust
/// Verify the active-session guard logic: Active + has_agents = reject.
#[test]
fn test_active_session_guard_logic() {
    // Simulate the guard condition
    let mut session = Session::new_for_test(
        "guard-test",
        std::env::temp_dir().join("kild_test_guard_worktree"),
    );
    session.status = SessionStatus::Active;

    // No agents → guard should not trigger
    assert!(
        !(session.status == SessionStatus::Active && session.has_agents()),
        "Active session without agents should not be blocked"
    );

    // Add an agent → guard should trigger
    let agent = AgentProcess::new(
        "claude".to_string(),
        "test_guard-test_0".to_string(),
        None,
        None,
        None,
        None,
        None,
        "claude --session-id abc".to_string(),
        chrono::Utc::now().to_rfc3339(),
        Some("test_guard-test_0".to_string()),
    )
    .unwrap();
    session.add_agent(agent);

    assert!(
        session.status == SessionStatus::Active && session.has_agents(),
        "Active session with agents should be blocked"
    );
}

/// Verify stopped sessions are not blocked by the guard.
#[test]
fn test_stopped_session_not_blocked() {
    let mut session = Session::new_for_test(
        "stopped-test",
        std::env::temp_dir().join("kild_test_stopped_worktree"),
    );
    session.status = SessionStatus::Stopped;
    // Even if agents vec is somehow non-empty, Stopped status should allow open
    assert!(
        !(session.status == SessionStatus::Active && session.has_agents()),
        "Stopped session should never be blocked"
    );
}
```

---

## Patterns to Follow

**From codebase — mirror the `AlreadyExists` guard in `create_session()`:**

```rust
// SOURCE: crates/kild-core/src/sessions/create.rs:142-153
// Pattern for "this operation is blocked by existing state" guard
if persistence::find_session_by_name(&config.sessions_dir(), &validated.name)?.is_some() {
    warn!(
        event = "core.session.create_failed",
        branch = %validated.name,
        reason = "already_exists",
    );
    return Err(SessionError::AlreadyExists {
        name: validated.name.into_inner(),
    });
}
```

**From codebase — mirror the `sync_daemon_session_status()` liveness pattern:**

```rust
// SOURCE: crates/kild-core/src/sessions/list.rs:58-100
// Pattern for daemon liveness check
pub fn sync_daemon_session_status(session: &mut Session) -> bool {
    if session.status != SessionStatus::Active {
        return false;
    }
    let daemon_sid = match session.latest_agent().and_then(|a| a.daemon_session_id()) {
        Some(id) => id.to_string(),
        None => return false,
    };
    // ... queries daemon, returns true if status changed to Stopped
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                              | Mitigation                                                                                                                     |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Stale-active session (daemon PTY exited)    | `sync_daemon_session_status()` detects this and syncs to Stopped before the guard fires, allowing reopen                       |
| Terminal session with stale Active status    | Trust stored status — no reliable liveness check for terminal PIDs. User can `kild stop` then `kild open`.                     |
| `kild open --all` double-fire               | `--all` already filters for `Stopped` only (`commands/open.rs:94-97`); unaffected by this change                               |
| Fleet brain (`honryu`) reopening workers    | Brain uses `kild open --no-attach --resume`. If worker is active, gets `AlreadyActive` error — brain can read it and `inject`  |
| Race condition: two `kild open` at once     | File-based persistence has no locking. Extremely unlikely for single-developer tool. Acceptable risk.                          |

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

1. `kild create foo --daemon` → session is Active
2. `kild open foo` → should get `AlreadyActive` error with actionable message
3. `kild open foo --resume` → should also get `AlreadyActive` error (agent is already running; use `kild attach` instead)
4. `kild stop foo && kild open foo` → should work normally
5. Kill the daemon PTY externally, then `kild open foo` → should sync to Stopped and reopen (stale-active recovery)

---

## Scope Boundaries

**IN SCOPE:**

- Add `AlreadyActive` error variant in `errors.rs`
- Add active-session guard in `open_session()` with daemon liveness check
- Add unit tests for the guard logic and error variant

**OUT OF SCOPE (do not touch):**

- `--new-agent` flag for intentional multi-agent spawning (issue mentions this as future work)
- Per-agent tracking for `kild stop --agent N` / `kild attach --agent N` (separate issue)
- Multi-agent addressability in `kild list --json` (separate issue)
- Changes to `rotate_agent_session_id()` itself — the guard prevents it from being called on active sessions
- Changes to `add_agent()` — the guard prevents duplicate pushes
- UI-side handling of the new error — `DispatchError` already surfaces `SessionError` variants

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-26T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-599.md`
