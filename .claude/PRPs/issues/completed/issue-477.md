# Investigation: perf: session persistence — pretty-print, full scan, linear search, full rewrite

**Issue**: #477 (https://github.com/Wirasm/kild/issues/477)
**Type**: PERFORMANCE
**Investigated**: 2026-02-18T00:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                         |
| ---------- | ------ | ------------------------------------------------------------------------------------------------- |
| Priority   | MEDIUM | P2 label confirmed; not blocking other work but directly degrades agent hook latency at scale     |
| Complexity | MEDIUM | ~5 files touched, one domain (sessions/persistence), no architectural changes                    |
| Confidence | HIGH   | Root cause is visible in source code, all callers identified, no unknowns                        |

---

## Problem Statement

The session persistence layer uses `serde_json::to_string_pretty` for all writes (adding unnecessary file size and serialize time), `find_session_by_name` triggers a full directory scan for every single-session CLI command, and `patch_session_json_field` does a read-parse-serialize-write cycle on `kild.json` even for the hot `agent-status` path that fires on every Claude Code hook event. With 15+ sessions, `kild agent-status --self` reads every session file twice per invocation.

---

## Analysis

### Root Cause / Change Rationale

Four independent inefficiencies compound at scale:

**Issue 1 — Pretty-print JSON:**
All `kild.json` writes use `to_string_pretty`, adding ~25–35% file size with no benefit (files are machine-read only). Sidecar files (`status`, `pr`) already use compact `to_string`. The inconsistency is accidental.

**Issue 2 — No branch→session_id index:**
`find_session_by_name` loads and deserializes every `kild.json` in `~/.kild/sessions/` to find one session by branch name. There is no way to go from branch → directory without a full scan because the directory name encodes `session_id.replace('/', "_")` (e.g., `myproject_feature-auth`), not the bare branch name.

**Issue 3 — `last_activity` patch on hot path:**
`update_agent_status` (called by `claude-status` hook on every Claude Code Stop/Notification/SubagentStop/TeammateIdle/TaskCompleted event) does:
1. `find_session_by_name` → full scan (Issue 2 above)
2. `write_agent_status` to sidecar → compact write (fast, correct)
3. `patch_session_json_field("last_activity", ...)` → read + parse + serialize (pretty) + write `kild.json`

Step 3 is redundant: the sidecar `status` already captures `updated_at` (the same timestamp). The health system at `health/operations.rs:61` reads `session.last_activity` but `enrich_session_with_health` already receives `agent_status_updated_at: Option<String>` and ignores it for health calculation — a pre-existing bug.

When `--self` is used, `find_session_by_worktree_path` at `agent_status.rs:91` calls `load_sessions_from_files` directly, then `update_agent_status` calls `find_session_by_name` which calls `load_sessions_from_files` again — **two full scans per invocation**.

**Issue 4 — Session list index (future work, not in this PR):**
`kild list` reads all N session files + N status sidecars + N PR sidecars. A lightweight index would allow fewer reads, but given the 3N sidecar reads and N git stat calls remain, this is lower priority and more complex.

### Evidence Chain

WHY: `kild agent-status` is slow with many sessions
↓ BECAUSE: it calls `find_session_by_name` which scans every session file
Evidence: `agent_status.rs:28` — `persistence::find_session_by_name(&config.sessions_dir(), name)`

↓ BECAUSE: `find_session_by_name` always calls `load_sessions_from_files` then linear-scans results
Evidence: `session_files.rs:289–303` — `let (sessions, _) = load_sessions_from_files(sessions_dir)?; for session in sessions { if &*session.branch == name { ... }`

↓ BECAUSE: no branch→session_id index exists; the directory name encodes `session_id` not `branch`
Evidence: `session_files.rs:10–12` — `let safe_id = session_id.replace('/', "_"); sessions_dir.join(safe_id)`

↓ ROOT CAUSE (find): Add `branch_index.json` in `~/.kild/sessions/` mapping `branch → session_id`, maintained on every `save_session_to_file` / `remove_session_file`

WHY: `kild.json` writes are larger/slower than needed
↓ BECAUSE: `serde_json::to_string_pretty` used in `save_session_to_file` and both patch functions
Evidence: `session_files.rs:141` — `serde_json::to_string_pretty(session)`, `patching.rs:44` — `serde_json::to_string_pretty(&json)`, `patching.rs:92` — same

↓ ROOT CAUSE (pretty): Replace with `serde_json::to_string`. Sidecar files already do this correctly.

WHY: `patch_session_json_field("last_activity", ...)` is called on every agent hook
↓ BECAUSE: health system reads `session.last_activity` from `kild.json`
Evidence: `health/operations.rs:61` — `session.last_activity.as_deref()`

↓ BUT: `enrich_session_with_health` already receives `agent_status_updated_at: Option<String>` and ignores it
Evidence: `health/operations.rs:57–63` — parameter received but `calculate_health_status` uses `session.last_activity.as_deref()` instead

↓ ROOT CAUSE (hot patch): Use sidecar `updated_at` in health calculation; remove `patch_session_json_field` from `update_agent_status`

### Affected Files

| File                                                              | Lines    | Action | Description                                          |
| ----------------------------------------------------------------- | -------- | ------ | ---------------------------------------------------- |
| `crates/kild-core/src/sessions/persistence/session_files.rs`     | 141      | UPDATE | `to_string_pretty` → `to_string`                     |
| `crates/kild-core/src/sessions/persistence/patching.rs`          | 44, 92   | UPDATE | `to_string_pretty` → `to_string` (both patch fns)    |
| `crates/kild-core/src/sessions/persistence/index.rs`             | NEW      | CREATE | Branch index: load/update/remove helpers             |
| `crates/kild-core/src/sessions/persistence/session_files.rs`     | 130–168  | UPDATE | Maintain index in `save_session_to_file`             |
| `crates/kild-core/src/sessions/persistence/session_files.rs`     | 289–303  | UPDATE | Use index in `find_session_by_name`, fallback to scan|
| `crates/kild-core/src/sessions/persistence/session_files.rs`     | 305–349  | UPDATE | Maintain index in `remove_session_file` (add branch param or purge by session_id) |
| `crates/kild-core/src/sessions/persistence/mod.rs`               | 1–19     | UPDATE | Add `mod index;`                                     |
| `crates/kild-core/src/sessions/agent_status.rs`                  | 42–50    | UPDATE | Remove `patch_session_json_field("last_activity")` call |
| `crates/kild-core/src/health/operations.rs`                      | 57–63    | UPDATE | Use `max(session.last_activity, agent_status_updated_at)` for health |
| `crates/kild-core/src/sessions/persistence/tests.rs`             | existing | UPDATE | Add tests for branch index round-trips               |

### Integration Points

- `open.rs:107`, `stop.rs:17`, `destroy.rs:137, 558`, `complete.rs:40`, `agent_status.rs:28` — all call `find_session_by_name` and benefit from Fix 2
- `agent_status.rs:91` — calls `load_sessions_from_files` directly for `--self` lookup; still benefits from Fix 2 via `find_session_by_name` being called afterwards
- `ports.rs:25` — calls `load_sessions_from_files` for port allocation during create; not addressed in this PR (infrequent path)
- `list.rs:102` — `sync_daemon_session_status` calls `patch_session_json_fields` for `status` + `last_activity`; the `last_activity` write here is kept (not hot path)
- `health/operations.rs` — `enrich_session_with_health` already receives `agent_status_updated_at` but ignores it

### Git History

- **Patching introduced**: dd0ec66 - "refactor: move session storage to per-session directories" (recent)
- **Sidecar files**: introduced in same refactor
- **Implication**: Pretty-print was likely the default and never revisited; sidecar files were added correctly but kild.json patching wasn't updated to match

---

## Implementation Plan

### Step 1: Switch to compact JSON (3 one-line changes)

**File**: `crates/kild-core/src/sessions/persistence/session_files.rs`
**Line**: 141
**Action**: UPDATE

**Current code:**
```rust
let session_json = serde_json::to_string_pretty(session).map_err(|e| {
```

**Required change:**
```rust
let session_json = serde_json::to_string(session).map_err(|e| {
```

---

**File**: `crates/kild-core/src/sessions/persistence/patching.rs`
**Line**: 44
**Action**: UPDATE

**Current code:**
```rust
let updated = serde_json::to_string_pretty(&json).map_err(|e| SessionError::IoError {
```

**Required change:**
```rust
let updated = serde_json::to_string(&json).map_err(|e| SessionError::IoError {
```

---

**File**: `crates/kild-core/src/sessions/persistence/patching.rs`
**Line**: 92
**Action**: UPDATE

**Current code:**
```rust
let updated = serde_json::to_string_pretty(&json).map_err(|e| SessionError::IoError {
```

**Required change:**
```rust
let updated = serde_json::to_string(&json).map_err(|e| SessionError::IoError {
```

**Why**: Session files are read by machines only. `to_string` produces the same valid JSON at ~25–35% smaller size and ~15% faster serialization. Sidecar files already use `to_string` — this makes the whole persistence layer consistent.

---

### Step 2: Add branch index (`persistence/index.rs`)

**File**: `crates/kild-core/src/sessions/persistence/index.rs`
**Action**: CREATE

```rust
//! Branch-to-session-id index for O(1) lookups by branch name.
//!
//! Stored as `branch_index.json` in the sessions directory.
//! Format: JSON object mapping branch names to session IDs.
//! Best-effort: on read errors, returns empty map (triggers full scan fallback).

use crate::sessions::errors::SessionError;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::session_files::cleanup_temp_file;

const INDEX_FILE: &str = "branch_index.json";
const INDEX_TMP_FILE: &str = "branch_index.json.tmp";

pub(super) fn load_branch_index(sessions_dir: &Path) -> HashMap<String, String> {
    let index_file = sessions_dir.join(INDEX_FILE);
    let content = match fs::read_to_string(&index_file) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return HashMap::new(),
        Err(e) => {
            tracing::warn!(
                event = "core.session.branch_index_read_failed",
                error = %e,
            );
            return HashMap::new();
        }
    };
    match serde_json::from_str(&content) {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!(
                event = "core.session.branch_index_parse_failed",
                error = %e,
            );
            HashMap::new()
        }
    }
}

/// Upsert branch → session_id in the index. Best-effort: warns on failure.
pub(super) fn update_branch_index(sessions_dir: &Path, branch: &str, session_id: &str) {
    let mut index = load_branch_index(sessions_dir);
    index.insert(branch.to_string(), session_id.to_string());
    write_branch_index(sessions_dir, &index);
}

/// Remove a branch entry from the index. Best-effort: warns on failure.
pub(super) fn remove_from_branch_index(sessions_dir: &Path, branch: &str) {
    let mut index = load_branch_index(sessions_dir);
    if index.remove(branch).is_none() {
        return; // nothing to remove
    }
    write_branch_index(sessions_dir, &index);
}

/// Lookup session_id for a branch name. Returns None if not in index.
pub(super) fn lookup_branch(sessions_dir: &Path, branch: &str) -> Option<String> {
    load_branch_index(sessions_dir).remove(branch)
}

fn write_branch_index(sessions_dir: &Path, index: &HashMap<String, String>) {
    let index_file = sessions_dir.join(INDEX_FILE);
    let tmp_file = sessions_dir.join(INDEX_TMP_FILE);
    let content = match serde_json::to_string(index) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                event = "core.session.branch_index_serialize_failed",
                error = %e,
            );
            return;
        }
    };
    if let Err(e) = fs::write(&tmp_file, &content) {
        cleanup_temp_file(&tmp_file, &e);
        tracing::warn!(
            event = "core.session.branch_index_write_failed",
            error = %e,
        );
        return;
    }
    if let Err(e) = fs::rename(&tmp_file, &index_file) {
        cleanup_temp_file(&tmp_file, &e);
        tracing::warn!(
            event = "core.session.branch_index_rename_failed",
            error = %e,
        );
    }
}
```

**Why**: All index operations are best-effort (warn on failure, never error). Index miss → full scan fallback in `find_session_by_name`. Atomic writes via temp + rename, consistent with the rest of the persistence layer. No `std::collections::HashMap` dependency is new (already used in the codebase).

---

### Step 3: Wire index into `session_files.rs`

**File**: `crates/kild-core/src/sessions/persistence/session_files.rs`

**3a. Add module import at top:**
```rust
use super::index;
```

**3b. In `save_session_to_file`, after successful rename (line 165):**
```rust
    // Maintain branch index for O(1) find_session_by_name lookups
    index::update_branch_index(sessions_dir, &session.branch, &session.id);

    Ok(())
```

**3c. In `remove_session_file`, after successful remove (line 332 / before `Ok(())`):**
The function signature needs a `branch` parameter (callers all have the `Session` available):
```rust
pub fn remove_session_file(
    sessions_dir: &Path,
    session_id: &str,
    branch: &str,
) -> Result<(), SessionError> {
    // ... existing body ...
    // After successful removal:
    index::remove_from_branch_index(sessions_dir, branch);
    Ok(())
}
```
Update all callers of `remove_session_file` to pass `&session.branch`. If any caller doesn't have the branch available, use `index::purge_session_from_index` (reverse scan by session_id) as a fallback.

**3d. Replace `find_session_by_name` body:**
```rust
pub fn find_session_by_name(
    sessions_dir: &Path,
    name: &str,
) -> Result<Option<Session>, SessionError> {
    // Fast path: try branch index first
    if let Some(session_id) = index::lookup_branch(sessions_dir, name) {
        let file = session_file(sessions_dir, &session_id);
        if file.exists() {
            let content = fs::read_to_string(&file)
                .map_err(|e| SessionError::IoError { source: e })?;
            if let Ok(session) = serde_json::from_str::<Session>(&content) {
                if &*session.branch == name {
                    return Ok(Some(session));
                }
                // Index stale (branch renamed or session replaced) — fall through to scan
            }
        }
        // Index entry exists but file is gone or stale — fall through to scan
    }

    // Slow path: full scan (index miss or stale)
    let (sessions, _) = load_sessions_from_files(sessions_dir)?;
    for session in sessions {
        if &*session.branch == name {
            // Opportunistically repair the index
            index::update_branch_index(sessions_dir, name, &session.id);
            return Ok(Some(session));
        }
    }

    Ok(None)
}
```

**Why**: Index hit avoids reading all N session files. Index miss (first run, or after manual `~/.kild/sessions/` edits) falls back to full scan and opportunistically repairs the index. Stale index (file moved/deleted) is handled gracefully.

---

### Step 4: Add `mod index` to `persistence/mod.rs`

**File**: `crates/kild-core/src/sessions/persistence/mod.rs`

**Current:**
```rust
mod patching;
mod session_files;
mod sidecar;
```

**Required change:**
```rust
mod index;
mod patching;
mod session_files;
mod sidecar;
```

---

### Step 5: Remove `last_activity` patch from `update_agent_status`

**File**: `crates/kild-core/src/sessions/agent_status.rs`
**Lines**: 42–50
**Action**: UPDATE

**Current code:**
```rust
    // Update last_activity on the session (heartbeat) via field patch to preserve unknown fields.
    // Using patch instead of full save prevents older binaries from dropping new fields
    // (e.g., installed kild binary dropping task_list_id added by a newer version).
    persistence::patch_session_json_field(
        &config.sessions_dir(),
        &session.id,
        "last_activity",
        serde_json::Value::String(now.clone()),
    )?;
```

**Required change:**
```rust
    // last_activity is tracked via the sidecar's updated_at (written above).
    // The health system reads agent_status_updated_at from the sidecar directly.
    // Only lifecycle events (open, stop, daemon sync) update last_activity in kild.json.
```

Remove the import of the `patch_session_json_field` call from this function; the sidecar write at line 40 already captures `updated_at` with the same timestamp.

**Why**: The sidecar `status` file written at line 40 already stores `updated_at`. Writing `last_activity` to `kild.json` on every agent hook invocation (every few seconds with an active Claude Code session) is the hottest write path. Eliminating it removes one full file read-parse-serialize-write cycle per `agent-status` invocation.

---

### Step 6: Fix health system to use sidecar `updated_at`

**File**: `crates/kild-core/src/health/operations.rs`
**Lines**: 57–63
**Action**: UPDATE

**Current code:**
```rust
pub fn enrich_session_with_health(
    session: &Session,
    process_metrics: Option<ProcessMetrics>,
    process_running: bool,
    agent_status: Option<AgentStatus>,
    agent_status_updated_at: Option<String>,
) -> KildHealth {
    let status = calculate_health_status(
        process_running,
        session.last_activity.as_deref(),  // <-- ignores agent_status_updated_at
        false,
    );
```

**Required change:**
```rust
pub fn enrich_session_with_health(
    session: &Session,
    process_metrics: Option<ProcessMetrics>,
    process_running: bool,
    agent_status: Option<AgentStatus>,
    agent_status_updated_at: Option<String>,
) -> KildHealth {
    // Use the most recent of kild.json last_activity and sidecar updated_at.
    // After step 5, agent hook updates only touch the sidecar; kild.json
    // last_activity reflects lifecycle events only.
    let effective_last_activity = most_recent_activity(
        session.last_activity.as_deref(),
        agent_status_updated_at.as_deref(),
    );
    let status = calculate_health_status(
        process_running,
        effective_last_activity.as_deref(),
        false,
    );
```

Add helper (private to this module):
```rust
fn most_recent_activity<'a>(a: Option<&'a str>, b: Option<&'a str>) -> Option<String> {
    match (a, b) {
        (None, x) | (x, None) => x.map(str::to_string),
        (Some(ta), Some(tb)) => {
            // Pick whichever RFC3339 timestamp is later; fall back to `a` on parse error
            let ta_dt = DateTime::parse_from_rfc3339(ta).ok();
            let tb_dt = DateTime::parse_from_rfc3339(tb).ok();
            match (ta_dt, tb_dt) {
                (Some(a), Some(b)) => Some(if a >= b { ta } else { tb }.to_string()),
                _ => Some(ta.to_string()),
            }
        }
    }
}
```

Also update `health/operations.rs:81` to use `effective_last_activity`:
```rust
    let metrics = HealthMetrics {
        // ...
        last_activity: effective_last_activity,
        // ...
    };
```

**Why**: Fixes the pre-existing bug where `agent_status_updated_at` was received but ignored. After removing `last_activity` patching from the hot path, the health system needs to consult the sidecar.

---

### Step 7: Add/Update Tests

**File**: `crates/kild-core/src/sessions/persistence/tests.rs`
**Action**: UPDATE

**Test cases to add:**
```rust
// Branch index round-trip
#[test]
fn test_branch_index_save_and_lookup() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_dir = tmp.path();
    let session = Session::new_for_test("feature-auth".to_string(), ...);
    save_session_to_file(&session, sessions_dir).unwrap();
    // Index should be populated
    let found = find_session_by_name(sessions_dir, "feature-auth").unwrap();
    assert!(found.is_some());
    assert_eq!(&*found.unwrap().branch, "feature-auth");
}

// Branch index removal
#[test]
fn test_branch_index_remove_on_session_remove() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_dir = tmp.path();
    let session = Session::new_for_test("feature-auth".to_string(), ...);
    save_session_to_file(&session, sessions_dir).unwrap();
    remove_session_file(sessions_dir, &session.id, &session.branch).unwrap();
    let found = find_session_by_name(sessions_dir, "feature-auth").unwrap();
    assert!(found.is_none());
}

// Index miss falls back to full scan
#[test]
fn test_find_session_by_name_fallback_without_index() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_dir = tmp.path();
    let session = Session::new_for_test("feature-auth".to_string(), ...);
    // Write session file directly without going through save_session_to_file
    // (no index entry created)
    let dir = sessions_dir.join("project_feature-auth");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kild.json"), serde_json::to_string(&session).unwrap()).unwrap();
    // Should still find it via full scan
    let found = find_session_by_name(sessions_dir, "feature-auth").unwrap();
    assert!(found.is_some());
}

// Compact JSON output (no pretty-print)
#[test]
fn test_save_session_compact_json() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_dir = tmp.path();
    let session = Session::new_for_test("feat".to_string(), ...);
    save_session_to_file(&session, sessions_dir).unwrap();
    let content = std::fs::read_to_string(
        sessions_dir.join("project_feat").join("kild.json")
    ).unwrap();
    // Compact JSON has no newlines (single line)
    assert_eq!(content.lines().count(), 1);
}
```

---

## Patterns to Follow

**Existing atomic write pattern (mirror this exactly):**
```rust
// SOURCE: crates/kild-core/src/sessions/persistence/sidecar.rs:30-38
let temp_file = dir.join("status.tmp");
if let Err(e) = fs::write(&temp_file, &content) {
    cleanup_temp_file(&temp_file, &e);
    return Err(SessionError::IoError { source: e });
}
if let Err(e) = fs::rename(&temp_file, &sidecar_file) {
    cleanup_temp_file(&temp_file, &e);
    return Err(SessionError::IoError { source: e });
}
```

**Best-effort pattern (mirror for index ops):**
```rust
// SOURCE: crates/kild-core/src/sessions/persistence/sidecar.rs:74-84
pub fn remove_agent_status_file(sessions_dir: &Path, session_id: &str) {
    let sidecar_file = session_dir(sessions_dir, session_id).join("status");
    if sidecar_file.exists()
        && let Err(e) = fs::remove_file(&sidecar_file)
    {
        tracing::warn!(
            event = "core.session.agent_status_file_remove_failed",
            ...
        );
    }
}
```

**Compact `to_string` (match sidecar pattern):**
```rust
// SOURCE: crates/kild-core/src/sessions/persistence/sidecar.rs:27
let content = serde_json::to_string(status_info).map_err(|e| SessionError::IoError {
    source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
})?;
```

---

## Edge Cases & Risks

| Risk/Edge Case                                              | Mitigation                                                                  |
| ----------------------------------------------------------- | --------------------------------------------------------------------------- |
| Two projects with same branch name (e.g., `feature-auth`)  | Index stores last-write-wins; behavior matches current (first match found). Full-scan fallback correct either way. |
| Index out of sync after manual fs edits                     | `find_session_by_name` reads file and validates `session.branch == name`; falls back to scan on mismatch; repairs index opportunistically. |
| `branch_index.json` corrupted / unparseable                 | `load_branch_index` returns empty HashMap; full-scan fallback kicks in.    |
| `branch_index.json` left behind after all sessions removed  | Harmless: contains an empty `{}`. Cleanup can handle it or leave it.       |
| Old binary writing `last_activity` after this change        | Existing forward-compat note in patch functions still applies; kild.json field preserved if written by older binary. |
| Health command shows stale `last_activity` after step 5     | Step 6 fixes this by using sidecar `updated_at` for health calculation.    |
| `remove_session_file` callers that don't have branch        | Add `purge_session_from_index(sessions_dir, session_id)` that does a reverse scan of the (small) index as fallback. |

---

## Validation

### Automated Checks

```bash
cargo fmt --check
cargo clippy --all -- -D warnings
cargo test --all
cargo build --all
```

### Specific Tests

```bash
cargo test -p kild-core sessions::persistence
cargo test -p kild-core sessions::agent_status
cargo test -p kild-core health::operations
```

### Manual Verification

1. Create 10+ sessions: `kild create test-{1..10}` and verify `kild list` output unchanged
2. Run `kild agent-status some-branch idle` and confirm it completes faster (no kild.json rewrite)
3. Check generated `kild.json` files are now single-line compact JSON (not indented)
4. Verify `~/.kild/sessions/branch_index.json` is created and contains correct branch→session_id mappings
5. Delete a session and verify its branch is removed from the index
6. Run `kild health` and verify Last Activity timestamps still update correctly via sidecar
7. Test `find_session_by_name` with a missing index (delete `branch_index.json`): should fall back to full scan and recreate index

---

## Scope Boundaries

**IN SCOPE:**
- Switch `to_string_pretty` → `to_string` in `session_files.rs` and `patching.rs`
- Add `branch_index.json` index with O(1) branch lookup, fallback to full scan
- Remove `patch_session_json_field("last_activity")` from `update_agent_status`
- Fix `enrich_session_with_health` to use sidecar `updated_at` for health calculation
- Tests for all four changes

**OUT OF SCOPE (do not touch):**
- `ports.rs:allocate_port_range` — still calls `load_sessions_from_files` (infrequent; called only on create)
- Comprehensive session list index (Issue 4 from original report) — requires larger architectural change, defer to separate issue
- `list.rs:sync_daemon_session_status` `last_activity` patch — kept, this is a lifecycle event not a hot path
- Any changes to sidecar format (`AgentStatusInfo`, `PrInfo`) — already correct
- kild-ui or kild-daemon changes — not affected

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-18T00:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-477.md`
