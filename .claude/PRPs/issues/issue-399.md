# Investigation: Stopped session shows stale agent activity status

**Issue**: #399 (https://github.com/Wirasm/kild/issues/399)
**Type**: BUG
**Investigated**: 2026-02-12T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                             |
| ---------- | ------ | ----------------------------------------------------------------------------------------------------- |
| Severity   | MEDIUM | Misleading UI output — session appears working when stopped. No data loss, workaround: destroy+recreate |
| Complexity | LOW    | Single file change (stop.rs), mirroring existing pattern from destroy.rs, plus one test               |
| Confidence | HIGH   | Root cause is unambiguous — stop.rs simply doesn't call remove_agent_status_file(), destroy.rs does   |

---

## Problem Statement

After running `kild agent-status <branch> working` and then `kild stop <branch>`, the stopped session still displays `Activity: working` in `kild status` and `kild list` output. This is because `stop_session()` does not remove the agent status sidecar file (`.status`), while `destroy_session()` does.

---

## Analysis

### Root Cause

WHY: Stopped session shows `Activity: working`
-> BECAUSE: `kild status` reads the `.status` sidecar file unconditionally
Evidence: `crates/kild/src/commands/status.rs:37` - `let status_info = session_ops::read_agent_status(&session.id);`

-> BECAUSE: The `.status` sidecar file still exists after stop
Evidence: `crates/kild-core/src/sessions/stop.rs:162-168` — no call to `remove_agent_status_file()`

-> ROOT CAUSE: `stop_session()` clears agents and sets status to Stopped, but never removes the `.status` sidecar file
Evidence: Compare `stop.rs:162-168` (no sidecar cleanup) with `destroy.rs:401-403` (has sidecar cleanup)

### Evidence Chain

**stop.rs:162-168** — what happens on stop:
```rust
// 5. Clear process info and set status to Stopped
session.clear_agents();
session.status = SessionStatus::Stopped;
session.last_activity = Some(chrono::Utc::now().to_rfc3339());

// 6. Save updated session (keep worktree, keep session file)
persistence::save_session_to_file(&session, &config.sessions_dir())?;
```

**destroy.rs:401-403** — what destroy does differently:
```rust
// 8. Remove sidecar files (best-effort)
persistence::remove_agent_status_file(&config.sessions_dir(), &session.id);
persistence::remove_pr_info_file(&config.sessions_dir(), &session.id);
```

### Affected Files

| File                                          | Lines   | Action | Description                                  |
| --------------------------------------------- | ------- | ------ | -------------------------------------------- |
| `crates/kild-core/src/sessions/stop.rs`       | 161-168 | UPDATE | Add agent status sidecar cleanup before save |
| `crates/kild-core/src/sessions/stop.rs`       | tests   | UPDATE | Add test verifying sidecar is removed on stop |

### Integration Points

- `crates/kild/src/commands/status.rs:37` reads sidecar via `session_ops::read_agent_status()`
- `crates/kild/src/commands/list.rs:71` reads sidecar via `session_ops::read_agent_status()`
- `crates/kild/src/table.rs:57-61,158-159` displays activity from sidecar
- `crates/kild-core/src/sessions/persistence.rs:303-314` provides `remove_agent_status_file()`

### Git History

- **Agent status introduced**: `9c53d24` - refactor: split sessions/handler.rs into focused modules
- **Last stop.rs change**: `131bd2e` - refactor: clarify runtime mode inference logic in stop
- **Implication**: The sidecar cleanup was never added to stop — oversight when agent-status feature was introduced

---

## Implementation Plan

### Step 1: Remove agent status sidecar file on stop

**File**: `crates/kild-core/src/sessions/stop.rs`
**Lines**: After line 161 (after runtime_mode inference, before clear_agents)
**Action**: UPDATE

**Current code (lines 162-168):**
```rust
    // 5. Clear process info and set status to Stopped
    session.clear_agents();
    session.status = SessionStatus::Stopped;
    session.last_activity = Some(chrono::Utc::now().to_rfc3339());

    // 6. Save updated session (keep worktree, keep session file)
    persistence::save_session_to_file(&session, &config.sessions_dir())?;
```

**Required change:**
```rust
    // 5. Remove agent status sidecar (best-effort, mirrors destroy.rs:402)
    persistence::remove_agent_status_file(&config.sessions_dir(), &session.id);

    // 6. Clear process info and set status to Stopped
    session.clear_agents();
    session.status = SessionStatus::Stopped;
    session.last_activity = Some(chrono::Utc::now().to_rfc3339());

    // 7. Save updated session (keep worktree, keep session file)
    persistence::save_session_to_file(&session, &config.sessions_dir())?;
```

**Why**: Mirrors the cleanup pattern already used in `destroy_session()`. The sidecar file is no longer meaningful once the session is stopped — the agent is not running.

### Step 2: Add test verifying sidecar cleanup on stop

**File**: `crates/kild-core/src/sessions/stop.rs`
**Action**: UPDATE (add test to existing `mod tests`)

**Test case to add:**
```rust
#[test]
fn test_stop_removes_agent_status_sidecar() {
    use crate::sessions::types::{AgentStatus, AgentStatusInfo};
    use std::fs;

    let unique_id = format!(
        "{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let temp_dir =
        std::env::temp_dir().join(format!("kild_test_stop_sidecar_{}", unique_id));
    let _ = fs::remove_dir_all(&temp_dir);
    let sessions_dir = temp_dir.join("sessions");
    let worktree_dir = temp_dir.join("worktree");
    fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
    fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

    // Create a session
    let session = Session::new(
        "test-project_sidecar-test".to_string(),
        "test-project".to_string(),
        "sidecar-test".to_string(),
        worktree_dir.clone(),
        "claude".to_string(),
        SessionStatus::Active,
        chrono::Utc::now().to_rfc3339(),
        3000,
        3009,
        10,
        None,
        None,
        vec![],
        None,
        None,
        None,
    );
    persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save");

    // Write agent status sidecar file
    let status_info = AgentStatusInfo {
        status: AgentStatus::Working,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    persistence::write_agent_status(&sessions_dir, &session.id, &status_info)
        .expect("Failed to write status");

    // Verify sidecar exists
    let sidecar_file = sessions_dir.join("test-project_sidecar-test.status");
    assert!(sidecar_file.exists(), "Sidecar should exist before stop");
    assert!(
        persistence::read_agent_status(&sessions_dir, &session.id).is_some(),
        "Should read agent status before stop"
    );

    // Simulate stop: remove sidecar + clear agents + set stopped
    persistence::remove_agent_status_file(&sessions_dir, &session.id);
    let mut stopped = session;
    stopped.clear_agents();
    stopped.status = SessionStatus::Stopped;
    persistence::save_session_to_file(&stopped, &sessions_dir).expect("Failed to save");

    // Verify sidecar is gone
    assert!(!sidecar_file.exists(), "Sidecar should be removed after stop");
    assert!(
        persistence::read_agent_status(&sessions_dir, &stopped.id).is_none(),
        "Should return None for agent status after stop"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
```

---

## Patterns to Follow

**From codebase — mirror destroy.rs:401-403 exactly:**

```rust
// SOURCE: crates/kild-core/src/sessions/destroy.rs:401-403
// Pattern for best-effort sidecar cleanup
persistence::remove_agent_status_file(&config.sessions_dir(), &session.id);
```

**From codebase — test pattern with temp dirs (stop.rs existing tests):**

```rust
// SOURCE: crates/kild-core/src/sessions/stop.rs:459-467
// Pattern for test isolation with unique temp dirs
let unique_id = format!(
    "{}_{}",
    std::process::id(),
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
);
let temp_dir = std::env::temp_dir().join(format!("kild_test_stop_sidecar_{}", unique_id));
```

---

## Edge Cases & Risks

| Risk/Edge Case                         | Mitigation                                                        |
| -------------------------------------- | ----------------------------------------------------------------- |
| Sidecar file doesn't exist on stop     | `remove_agent_status_file()` already handles missing files (no-op) |
| File removal fails (permissions)       | Function logs warn and continues (best-effort pattern)            |
| Session re-opened after stop           | `kild open` + `agent-status` will recreate the sidecar naturally  |

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

1. `kild create test --no-agent --no-daemon`
2. `kild agent-status test working`
3. `kild status test` — verify shows `Activity: working`
4. `kild stop test`
5. `kild status test` — verify activity is gone (no `Activity:` line)
6. `kild list` — verify activity column shows `-` for stopped session

---

## Scope Boundaries

**IN SCOPE:**
- Remove `.status` sidecar file in `stop_session()`
- Add test for this behavior

**OUT OF SCOPE (do not touch):**
- Display-side filtering (status.rs, list.rs, table.rs) — removing the file is the clean fix
- PR info sidecar (`.pr`) — PR info is still relevant for stopped sessions
- Any other stop.rs logic

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-12T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-399.md`
