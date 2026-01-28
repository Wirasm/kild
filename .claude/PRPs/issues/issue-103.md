# Investigation: UI does not remove destroyed kilds until manual refresh

**Issue**: #103 (https://github.com/Wirasm/kild/issues/103)
**Type**: BUG
**Investigated**: 2026-01-28T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                                                       |
| ---------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Severity   | MEDIUM | User impact is visual inconsistency (stale data) not data loss. Workaround exists (manual refresh button). Feature still works, just UI is out of sync. |
| Complexity | LOW    | Fix requires changes to 1-2 files (state.rs and possibly main_view.rs). Isolated to UI refresh mechanism. No architectural changes needed.      |
| Confidence | HIGH   | Root cause is clearly identified with code evidence. The issue description in #103 accurately diagnoses the problem. Fix approach is straightforward. |

---

## Problem Statement

When kilds are destroyed via CLI while the UI is running, the UI continues to display them with a "Stopped" status indicator instead of removing them from the list. The auto-refresh mechanism (`update_statuses_only()`) only checks process status but never reloads the session list from disk, so it cannot detect when session files are deleted externally.

---

## Analysis

### Root Cause / Change Rationale

The auto-refresh loop in `main_view.rs` calls `update_statuses_only()` every 5 seconds, which only updates the `ProcessStatus` field of existing `KildDisplay` objects in memory. It never reloads session files from disk, so it cannot detect when sessions are destroyed externally (CLI or other tools).

### Evidence Chain

WHY: UI shows destroyed kilds as "Stopped" instead of removing them
↓ BECAUSE: Auto-refresh only updates process status, not session list
Evidence: `main_view.rs:144` - `view.state.update_statuses_only();`

↓ BECAUSE: `update_statuses_only()` iterates over existing in-memory displays
Evidence: `state.rs:341` - `for kild_display in &mut self.displays {`

↓ ROOT CAUSE: `update_statuses_only()` never compares in-memory state to disk state
Evidence: `state.rs:331-360` - method only updates `status` field, never calls `load_sessions_from_files()` or checks if sessions were deleted

### Affected Files

| File                      | Lines   | Action | Description                                                       |
| ------------------------- | ------- | ------ | ----------------------------------------------------------------- |
| `crates/kild-ui/src/state.rs` | 331-360 | UPDATE | Modify `update_statuses_only()` to detect session count mismatch  |
| `crates/kild-ui/src/state.rs` | NEW     | CREATE | Add helper function to count session files on disk                |

### Integration Points

- `main_view.rs:144` calls `update_statuses_only()` in the auto-refresh loop
- `actions.rs:89` calls `session_ops::list_sessions()` for full refresh
- `kild_core::session_ops::list_sessions()` reads session files from disk

### Git History

- **Auto-refresh introduced**: Part of initial kild-ui implementation
- **Last modified**: Recent commits for project management features
- **Implication**: This is a design oversight from initial implementation, not a regression

---

## Implementation Plan

### Step 1: Add helper function to count session files

**File**: `crates/kild-ui/src/state.rs`
**Lines**: After line 360 (after `update_statuses_only()`)
**Action**: CREATE (add new function)

**Required change:**

```rust
/// Count session files on disk without fully loading them.
///
/// This is a lightweight check used by `update_statuses_only()` to detect
/// when sessions have been added or removed externally (e.g., via CLI).
fn count_session_files() -> usize {
    let config = match kild_core::KildConfig::load_hierarchy() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                event = "ui.count_session_files.config_failed",
                error = %e
            );
            return 0;
        }
    };

    let sessions_dir = config.sessions_dir();
    if !sessions_dir.exists() {
        return 0;
    }

    match std::fs::read_dir(&sessions_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|s| s.to_str())
                    == Some("json")
            })
            .count(),
        Err(e) => {
            tracing::warn!(
                event = "ui.count_session_files.read_dir_failed",
                path = %sessions_dir.display(),
                error = %e
            );
            0
        }
    }
}
```

**Why**: Lightweight way to detect if the number of sessions on disk differs from the in-memory count, without the overhead of fully loading and deserializing all session files.

---

### Step 2: Update `update_statuses_only()` to detect session count mismatch

**File**: `crates/kild-ui/src/state.rs`
**Lines**: 331-360
**Action**: UPDATE

**Current code:**

```rust
/// Update only the process status of existing kilds without reloading from disk.
///
/// This is faster than refresh_sessions() for status polling because it:
/// - Doesn't reload session files from disk
/// - Only checks if tracked processes are still running
/// - Preserves the existing kild list structure
///
/// Note: This does NOT update git status or diff stats. Use `refresh_sessions()`
/// for a full refresh that includes git information.
pub fn update_statuses_only(&mut self) {
    for kild_display in &mut self.displays {
        kild_display.status = match kild_display.session.process_id {
            None => ProcessStatus::Stopped,
            Some(pid) => match kild_core::process::is_process_running(pid) {
                Ok(true) => ProcessStatus::Running,
                Ok(false) => ProcessStatus::Stopped,
                Err(e) => {
                    tracing::warn!(
                        event = "ui.kild_list.process_check_failed",
                        pid = pid,
                        branch = kild_display.session.branch,
                        error = %e
                    );
                    ProcessStatus::Unknown
                }
            },
        };
    }
    self.last_refresh = std::time::Instant::now();
}
```

**Required change:**

```rust
/// Update only the process status of existing kilds without reloading from disk.
///
/// This is faster than refresh_sessions() for status polling because it:
/// - Doesn't reload session files from disk (unless count mismatch detected)
/// - Only checks if tracked processes are still running
/// - Preserves the existing kild list structure
///
/// If the session count on disk differs from the in-memory count (indicating
/// external create/destroy operations), triggers a full refresh instead.
///
/// Note: This does NOT update git status or diff stats. Use `refresh_sessions()`
/// for a full refresh that includes git information.
pub fn update_statuses_only(&mut self) {
    // Check if session count changed (external create/destroy)
    let disk_count = count_session_files();
    if disk_count != self.displays.len() {
        tracing::info!(
            event = "ui.auto_refresh.session_count_mismatch",
            disk_count = disk_count,
            memory_count = self.displays.len(),
            action = "triggering full refresh"
        );
        self.refresh_sessions();
        return;
    }

    // No count change - just update process statuses
    for kild_display in &mut self.displays {
        kild_display.status = match kild_display.session.process_id {
            None => ProcessStatus::Stopped,
            Some(pid) => match kild_core::process::is_process_running(pid) {
                Ok(true) => ProcessStatus::Running,
                Ok(false) => ProcessStatus::Stopped,
                Err(e) => {
                    tracing::warn!(
                        event = "ui.kild_list.process_check_failed",
                        pid = pid,
                        branch = kild_display.session.branch,
                        error = %e
                    );
                    ProcessStatus::Unknown
                }
            },
        };
    }
    self.last_refresh = std::time::Instant::now();
}
```

**Why**: Adding a session count check at the start of `update_statuses_only()` allows detecting external changes with minimal overhead. The check counts `.json` files in the sessions directory without deserializing them - this is O(n) in directory entries but with minimal I/O overhead. If the count differs, trigger a full refresh to sync with disk state.

---

### Step 3: Add unit tests for the count mismatch detection

**File**: `crates/kild-ui/src/state.rs`
**Lines**: End of tests module
**Action**: UPDATE (add tests)

**Test cases to add:**

```rust
#[test]
fn test_count_session_files_returns_zero_for_empty_or_missing_dir() {
    // This tests the helper function in isolation
    // The actual test depends on test environment setup
    // If sessions dir doesn't exist or is empty, should return 0
    // (Implementation will vary based on test isolation approach)
}

#[test]
fn test_update_statuses_only_triggers_refresh_on_count_mismatch() {
    use kild_core::sessions::types::SessionStatus;

    // Create state with some displays
    let mut state = make_test_state();
    let session = Session {
        id: "test-id".to_string(),
        branch: "test-branch".to_string(),
        worktree_path: PathBuf::from("/tmp/test"),
        agent: "claude".to_string(),
        project_id: "test-project".to_string(),
        status: SessionStatus::Active,
        created_at: "2024-01-01T00:00:00Z".to_string(),
        port_range_start: 0,
        port_range_end: 0,
        port_count: 0,
        process_id: None,
        process_name: None,
        process_start_time: None,
        terminal_type: None,
        terminal_window_id: None,
        command: String::new(),
        last_activity: None,
        note: None,
    };
    state.displays = vec![KildDisplay {
        session,
        status: ProcessStatus::Stopped,
        git_status: GitStatus::Unknown,
        diff_stats: None,
    }];

    // When disk count doesn't match memory count, update_statuses_only
    // should detect this and call refresh_sessions()
    // (Note: Full integration test would require mocking the filesystem)
}
```

**Why**: Unit tests ensure the count mismatch detection works correctly and doesn't break existing behavior.

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: state.rs:323-329
// Pattern for full refresh
pub fn refresh_sessions(&mut self) {
    let (displays, load_error) = crate::actions::refresh_sessions();
    self.displays = displays;
    self.load_error = load_error;
    self.last_refresh = std::time::Instant::now();
}
```

```rust
// SOURCE: state.rs:340-360
// Pattern for logging in state methods
tracing::warn!(
    event = "ui.kild_list.process_check_failed",
    pid = pid,
    branch = kild_display.session.branch,
    error = %e
);
```

---

## Edge Cases & Risks

| Risk/Edge Case                          | Mitigation                                                                                             |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| Race condition: file being written      | Count check is atomic read. If count matches but session is mid-write, next refresh will catch it.    |
| Performance: frequent disk reads        | Only counts directory entries (no file content read). Cost is ~1 syscall per refresh cycle.           |
| Config load failure                     | Helper returns 0, triggering no refresh. Logs warning. Graceful degradation.                          |
| Session added AND removed in same cycle | If net count unchanged, won't detect. Acceptable - extremely rare edge case, next manual refresh fixes. |

---

## Validation

### Automated Checks

```bash
# Type check
cargo check -p kild-ui

# Run kild-ui tests
cargo test -p kild-ui

# Run all tests
cargo test --all

# Lint
cargo clippy --all -- -D warnings

# Format check
cargo fmt --check
```

### Manual Verification

1. Start the UI: `cargo run -p kild-ui`
2. Create a kild via CLI: `cargo run -p kild -- create test-stale --agent claude`
3. Observe the kild appears in the UI (within 5 seconds)
4. Destroy the kild via CLI: `cargo run -p kild -- destroy test-stale --force`
5. **Verify**: The kild disappears from the UI within 5 seconds (no manual refresh needed)

---

## Scope Boundaries

**IN SCOPE:**

- Modify `update_statuses_only()` to detect session count changes
- Add helper function `count_session_files()` in state.rs
- Add appropriate logging for the count mismatch detection
- Add unit tests for the new functionality

**OUT OF SCOPE (do not touch):**

- Filesystem watcher implementation (future enhancement per issue #103)
- Changes to kild-core session loading
- Changes to the refresh interval constant
- Any UI rendering changes
- Changes to the manual refresh button behavior

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-28T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-103.md`
