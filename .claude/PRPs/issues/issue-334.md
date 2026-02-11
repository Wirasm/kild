# Investigation: Health module has zero test coverage (529 lines)

**Issue**: #334 (https://github.com/Wirasm/kild/issues/334)
**Type**: CHORE
**Investigated**: 2026-02-11T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                   |
| ---------- | ------ | ----------------------------------------------------------------------------------------------------------- |
| Priority   | MEDIUM | Health monitoring is used but not blocking other work; missing tests increase regression risk during refactors |
| Complexity | MEDIUM | 3 files to add tests to, pure logic + filesystem I/O; no external service mocking needed                    |
| Confidence | HIGH   | All code is readable, functions are pure or have clear I/O boundaries, test patterns well-established in repo |

---

## Problem Statement

The health module (`crates/kild-core/src/health/`) has 537 lines of code across 3 implementation files with zero test coverage. This includes health status calculation logic, session enrichment, aggregation, and snapshot storage with filesystem persistence. Without tests, refactors to this module risk silent regressions.

---

## Analysis

### Change Rationale

The health module contains testable pure logic (`calculate_health_status`, `aggregate_health_stats`) and filesystem operations (`save_snapshot`, `load_history`, `cleanup_old_history`) that follow patterns already tested elsewhere in the codebase. Adding unit tests brings this module in line with the testing standards of other modules like `process/operations.rs` and `cleanup/operations.rs`.

### Affected Files

| File                                             | Lines   | Action | Description                                                  |
| ------------------------------------------------ | ------- | ------ | ------------------------------------------------------------ |
| `crates/kild-core/src/health/operations.rs`      | 121     | UPDATE | Add `#[cfg(test)] mod tests` with tests for all 4 functions  |
| `crates/kild-core/src/health/storage.rs`         | 231     | UPDATE | Add `#[cfg(test)] mod tests` with tests for storage functions |
| `crates/kild-core/src/sessions/types.rs`         | 463-483 | (ref)  | Uses existing `Session::new_for_test()` helper               |

### Integration Points

- `handler.rs` calls `sessions::handler::list_sessions()` and `process::is_process_running()` — these are integration-level dependencies, NOT unit-testable without mocking. Skip handler tests.
- `operations.rs` functions are pure logic (except the global `AtomicU64` threshold) — fully unit-testable.
- `storage.rs` functions do filesystem I/O — testable with `tempfile::TempDir`.

### Git History

- **Last modified**: `f1a4972` - "refactor: remove singular process-tracking fields from Session (#237)"
- **Implication**: Multi-agent support was added; health enrichment now iterates `session.agents()`. Tests should cover the current multi-agent-aware shapes.

---

## Implementation Plan

### Step 1: Add unit tests to `operations.rs`

**File**: `crates/kild-core/src/health/operations.rs`
**Lines**: Append at end of file
**Action**: UPDATE

**Tests to add:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::path::PathBuf;

    // --- calculate_health_status tests ---

    #[test]
    fn test_calculate_health_status_crashed_when_not_running() {
        let result = calculate_health_status(false, Some("2026-01-01T00:00:00Z"), false);
        assert_eq!(result, HealthStatus::Crashed);
    }

    #[test]
    fn test_calculate_health_status_unknown_when_no_activity() {
        let result = calculate_health_status(true, None, false);
        assert_eq!(result, HealthStatus::Unknown);
    }

    #[test]
    fn test_calculate_health_status_unknown_when_invalid_timestamp() {
        let result = calculate_health_status(true, Some("not-a-timestamp"), false);
        assert_eq!(result, HealthStatus::Unknown);
    }

    #[test]
    fn test_calculate_health_status_working_when_recent_activity() {
        let recent = Utc::now().to_rfc3339();
        let result = calculate_health_status(true, Some(&recent), false);
        assert_eq!(result, HealthStatus::Working);
    }

    #[test]
    fn test_calculate_health_status_idle_when_old_activity_from_agent() {
        // Set threshold to 10 minutes, use activity from 20 minutes ago
        set_idle_threshold_minutes(10);
        let old = (Utc::now() - chrono::Duration::minutes(20)).to_rfc3339();
        let result = calculate_health_status(true, Some(&old), false);
        assert_eq!(result, HealthStatus::Idle);
    }

    #[test]
    fn test_calculate_health_status_stuck_when_old_activity_from_user() {
        set_idle_threshold_minutes(10);
        let old = (Utc::now() - chrono::Duration::minutes(20)).to_rfc3339();
        let result = calculate_health_status(true, Some(&old), true);
        assert_eq!(result, HealthStatus::Stuck);
    }

    #[test]
    fn test_calculate_health_status_threshold_boundary() {
        // Activity exactly at threshold should NOT be Working
        set_idle_threshold_minutes(5);
        let boundary = (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
        let result = calculate_health_status(true, Some(&boundary), false);
        assert_eq!(result, HealthStatus::Idle);
    }

    #[test]
    fn test_calculate_health_status_respects_custom_threshold() {
        set_idle_threshold_minutes(60);
        let thirty_min_ago = (Utc::now() - chrono::Duration::minutes(30)).to_rfc3339();
        let result = calculate_health_status(true, Some(&thirty_min_ago), false);
        assert_eq!(result, HealthStatus::Working);
        // Reset
        set_idle_threshold_minutes(10);
    }

    #[test]
    fn test_calculate_health_status_crashed_takes_priority_over_activity() {
        // Even with recent activity, not running = Crashed
        let recent = Utc::now().to_rfc3339();
        let result = calculate_health_status(false, Some(&recent), false);
        assert_eq!(result, HealthStatus::Crashed);
    }

    // --- threshold getter/setter tests ---

    #[test]
    fn test_idle_threshold_default() {
        // Default is 10 minutes (set in static)
        // Note: other tests may have modified this, so just verify get/set round-trips
        set_idle_threshold_minutes(42);
        assert_eq!(get_idle_threshold_minutes(), 42);
        set_idle_threshold_minutes(10); // reset
    }

    // --- enrich_session_with_health tests ---

    #[test]
    fn test_enrich_session_running_with_metrics() {
        let session = Session::new_for_test("test-branch".to_string(), PathBuf::from("/tmp/test"));
        let metrics = ProcessMetrics {
            cpu_usage_percent: 25.0,
            memory_usage_bytes: 100 * 1024 * 1024, // 100 MB
        };

        let health = enrich_session_with_health(&session, Some(metrics), true);

        assert_eq!(health.branch, "test-branch");
        assert_eq!(health.metrics.process_status, "Running");
        assert_eq!(health.metrics.cpu_usage_percent, Some(25.0));
        assert_eq!(health.metrics.memory_usage_mb, Some(100));
    }

    #[test]
    fn test_enrich_session_stopped_no_metrics() {
        let session = Session::new_for_test("stopped".to_string(), PathBuf::from("/tmp/test"));

        let health = enrich_session_with_health(&session, None, false);

        assert_eq!(health.metrics.process_status, "Stopped");
        assert_eq!(health.metrics.status, HealthStatus::Crashed);
        assert_eq!(health.metrics.cpu_usage_percent, None);
        assert_eq!(health.metrics.memory_usage_mb, None);
        assert_eq!(health.metrics.status_icon, "\u{274c}"); // ❌
    }

    #[test]
    fn test_enrich_session_copies_session_fields() {
        let session = Session::new_for_test("my-branch".to_string(), PathBuf::from("/tmp/wt"));

        let health = enrich_session_with_health(&session, None, false);

        assert_eq!(health.session_id, session.id);
        assert_eq!(health.project_id, session.project_id);
        assert_eq!(health.branch, session.branch);
        assert_eq!(health.agent, session.agent);
        assert_eq!(health.worktree_path, "/tmp/wt");
        assert_eq!(health.created_at, session.created_at);
    }

    // --- aggregate_health_stats tests ---

    #[test]
    fn test_aggregate_empty() {
        let output = aggregate_health_stats(&[]);
        assert_eq!(output.total_count, 0);
        assert_eq!(output.working_count, 0);
        assert_eq!(output.idle_count, 0);
        assert_eq!(output.stuck_count, 0);
        assert_eq!(output.crashed_count, 0);
    }

    #[test]
    fn test_aggregate_mixed_statuses() {
        let session = Session::new_for_test("b".to_string(), PathBuf::from("/tmp"));

        // Create kilds with different statuses
        let mut working = enrich_session_with_health(&session, None, true);
        // Force recent activity to get Working status
        let recent = Utc::now().to_rfc3339();
        working.metrics.status = HealthStatus::Working;

        let mut idle = enrich_session_with_health(&session, None, true);
        idle.metrics.status = HealthStatus::Idle;

        let mut crashed = enrich_session_with_health(&session, None, false);
        crashed.metrics.status = HealthStatus::Crashed;

        let mut unknown = enrich_session_with_health(&session, None, true);
        unknown.metrics.status = HealthStatus::Unknown;

        let output = aggregate_health_stats(&[working, idle, crashed, unknown]);

        assert_eq!(output.total_count, 4);
        assert_eq!(output.working_count, 1);
        assert_eq!(output.idle_count, 1);
        assert_eq!(output.crashed_count, 1);
        // Unknown is NOT counted in any specific bucket
        assert_eq!(output.stuck_count, 0);
    }
}
```

**Why**: Tests pure logic functions that form the core of health status calculation. These are the highest-value tests since they validate the status state machine.

---

### Step 2: Add unit tests to `storage.rs`

**File**: `crates/kild-core/src/health/storage.rs`
**Lines**: Append at end of file
**Action**: UPDATE

The storage functions use `get_history_dir()` which hardcodes `~/.kild/health_history/`. To make storage functions testable with `tempfile::TempDir`, we need to add internal variants that accept a directory parameter. Add `save_snapshot_to`, `load_history_from`, and `cleanup_old_history_in` functions that take a `&Path` parameter, and have the public functions delegate to them.

**Required changes:**

1. Refactor `save_snapshot` to delegate to `save_snapshot_to(dir, snapshot)`
2. Refactor `load_history` to delegate to `load_history_from(dir, days)`
3. Refactor `cleanup_old_history` to delegate to `cleanup_old_history_in(dir, retention_days)`
4. Add `From<&HealthOutput>` conversion tests
5. Add filesystem round-trip tests using `tempfile::TempDir`

```rust
// Refactored public API (delegates to _to/_from/_in variants):

pub fn save_snapshot(snapshot: &HealthSnapshot) -> Result<(), std::io::Error> {
    let history_dir = get_history_dir()?;
    save_snapshot_to(&history_dir, snapshot)
}

pub fn save_snapshot_to(history_dir: &Path, snapshot: &HealthSnapshot) -> Result<(), std::io::Error> {
    fs::create_dir_all(history_dir)?;
    // ... existing logic using history_dir instead of get_history_dir()
}

pub fn load_history(days: u64) -> Result<Vec<HealthSnapshot>, std::io::Error> {
    let history_dir = get_history_dir()?;
    load_history_from(&history_dir, days)
}

pub fn load_history_from(history_dir: &Path, days: u64) -> Result<Vec<HealthSnapshot>, std::io::Error> {
    // ... existing logic using history_dir parameter
}

pub fn cleanup_old_history(retention_days: u64) -> Result<CleanupResult, std::io::Error> {
    let history_dir = get_history_dir()?;
    cleanup_old_history_in(&history_dir, retention_days)
}

pub fn cleanup_old_history_in(history_dir: &Path, retention_days: u64) -> Result<CleanupResult, std::io::Error> {
    // ... existing logic using history_dir parameter
}
```

**Tests to add:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::types::*;
    use chrono::{Duration, Utc};
    use tempfile::TempDir;

    fn make_test_snapshot(working: usize, idle: usize, crashed: usize) -> HealthSnapshot {
        HealthSnapshot {
            timestamp: Utc::now(),
            total_kilds: working + idle + crashed,
            working,
            idle,
            stuck: 0,
            crashed,
            avg_cpu_percent: Some(15.0),
            total_memory_mb: Some(512),
        }
    }

    fn make_test_health_output() -> HealthOutput {
        let metrics = HealthMetrics {
            cpu_usage_percent: Some(20.0),
            memory_usage_mb: Some(256),
            process_status: "Running".to_string(),
            last_activity: None,
            status: HealthStatus::Working,
            status_icon: "check".to_string(),
        };
        let kild = KildHealth {
            session_id: "s1".to_string(),
            project_id: "p1".to_string(),
            branch: "main".to_string(),
            agent: "claude".to_string(),
            worktree_path: "/tmp/wt".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            metrics,
        };
        HealthOutput {
            kilds: vec![kild],
            total_count: 1,
            working_count: 1,
            idle_count: 0,
            stuck_count: 0,
            crashed_count: 0,
        }
    }

    // --- HealthSnapshot From<&HealthOutput> tests ---

    #[test]
    fn test_snapshot_from_health_output_with_metrics() {
        let output = make_test_health_output();
        let snapshot = HealthSnapshot::from(&output);

        assert_eq!(snapshot.total_kilds, 1);
        assert_eq!(snapshot.working, 1);
        assert_eq!(snapshot.avg_cpu_percent, Some(20.0));
        assert_eq!(snapshot.total_memory_mb, Some(256));
    }

    #[test]
    fn test_snapshot_from_empty_output() {
        let output = HealthOutput {
            kilds: vec![],
            total_count: 0,
            working_count: 0,
            idle_count: 0,
            stuck_count: 0,
            crashed_count: 0,
        };
        let snapshot = HealthSnapshot::from(&output);

        assert_eq!(snapshot.total_kilds, 0);
        assert_eq!(snapshot.avg_cpu_percent, None);
        assert_eq!(snapshot.total_memory_mb, None);
    }

    #[test]
    fn test_snapshot_from_output_no_metrics() {
        let metrics = HealthMetrics {
            cpu_usage_percent: None,
            memory_usage_mb: None,
            process_status: "Stopped".to_string(),
            last_activity: None,
            status: HealthStatus::Crashed,
            status_icon: "x".to_string(),
        };
        let kild = KildHealth {
            session_id: "s1".to_string(),
            project_id: "p1".to_string(),
            branch: "b".to_string(),
            agent: "a".to_string(),
            worktree_path: "/tmp".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            metrics,
        };
        let output = HealthOutput {
            kilds: vec![kild],
            total_count: 1,
            working_count: 0,
            idle_count: 0,
            stuck_count: 0,
            crashed_count: 1,
        };
        let snapshot = HealthSnapshot::from(&output);

        assert_eq!(snapshot.avg_cpu_percent, None);
        assert_eq!(snapshot.total_memory_mb, None); // 0 mem → None
    }

    // --- save_snapshot / load round-trip tests ---

    #[test]
    fn test_save_and_load_snapshot_roundtrip() {
        let dir = TempDir::new().unwrap();
        let snapshot = make_test_snapshot(2, 1, 0);

        save_snapshot_to(dir.path(), &snapshot).unwrap();

        let loaded = load_history_from(dir.path(), 1).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].working, 2);
        assert_eq!(loaded[0].idle, 1);
    }

    #[test]
    fn test_save_multiple_snapshots_same_day() {
        let dir = TempDir::new().unwrap();
        let s1 = make_test_snapshot(1, 0, 0);
        let s2 = make_test_snapshot(0, 1, 0);

        save_snapshot_to(dir.path(), &s1).unwrap();
        save_snapshot_to(dir.path(), &s2).unwrap();

        let loaded = load_history_from(dir.path(), 1).unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[test]
    fn test_load_history_empty_directory() {
        let dir = TempDir::new().unwrap();
        let loaded = load_history_from(dir.path(), 7).unwrap();
        assert_eq!(loaded.len(), 0);
    }

    #[test]
    fn test_load_history_nonexistent_directory() {
        let dir = TempDir::new().unwrap();
        let nonexistent = dir.path().join("does_not_exist");
        // load_history_from should return Ok(empty) when dir doesn't exist
        let loaded = load_history_from(&nonexistent, 7).unwrap();
        assert_eq!(loaded.len(), 0);
    }

    #[test]
    fn test_load_history_skips_corrupted_files() {
        let dir = TempDir::new().unwrap();

        // Write a valid snapshot
        let snapshot = make_test_snapshot(1, 0, 0);
        save_snapshot_to(dir.path(), &snapshot).unwrap();

        // Write a corrupted file
        let corrupted_path = dir.path().join("2025-01-01.json");
        fs::write(&corrupted_path, "not valid json").unwrap();

        let loaded = load_history_from(dir.path(), 365).unwrap();
        // Should load the valid one and skip corrupted
        assert!(loaded.len() >= 1);
    }

    #[test]
    fn test_save_snapshot_overwrites_corrupted_daily_file() {
        let dir = TempDir::new().unwrap();

        // Pre-create a corrupted file for today's date
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let filepath = dir.path().join(format!("{}.json", today));
        fs::write(&filepath, "corrupted data").unwrap();

        // Save should succeed (start fresh)
        let snapshot = make_test_snapshot(1, 0, 0);
        save_snapshot_to(dir.path(), &snapshot).unwrap();

        // Verify the file now contains valid data
        let loaded = load_history_from(dir.path(), 1).unwrap();
        assert_eq!(loaded.len(), 1);
    }

    // --- cleanup tests ---

    #[test]
    fn test_cleanup_removes_old_files() {
        let dir = TempDir::new().unwrap();

        // Create an old file (30 days ago)
        let old_date = (Utc::now() - Duration::days(30)).format("%Y-%m-%d").to_string();
        let old_file = dir.path().join(format!("{}.json", old_date));
        fs::write(&old_file, "[]").unwrap();

        // Create a recent file (today)
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let today_file = dir.path().join(format!("{}.json", today));
        fs::write(&today_file, "[]").unwrap();

        let result = cleanup_old_history_in(dir.path(), 7).unwrap();

        assert_eq!(result.removed, 1);
        assert_eq!(result.failed, 0);
        assert!(!old_file.exists());
        assert!(today_file.exists());
    }

    #[test]
    fn test_cleanup_empty_directory() {
        let dir = TempDir::new().unwrap();
        let result = cleanup_old_history_in(dir.path(), 7).unwrap();
        assert_eq!(result.removed, 0);
        assert_eq!(result.failed, 0);
    }

    #[test]
    fn test_cleanup_ignores_non_json_files() {
        let dir = TempDir::new().unwrap();

        let old_date = (Utc::now() - Duration::days(30)).format("%Y-%m-%d").to_string();
        let txt_file = dir.path().join(format!("{}.txt", old_date));
        fs::write(&txt_file, "not json").unwrap();

        let result = cleanup_old_history_in(dir.path(), 7).unwrap();
        assert_eq!(result.removed, 0);
        assert!(txt_file.exists());
    }

    #[test]
    fn test_cleanup_nonexistent_directory() {
        let dir = TempDir::new().unwrap();
        let nonexistent = dir.path().join("nope");
        let result = cleanup_old_history_in(&nonexistent, 7).unwrap();
        assert_eq!(result.removed, 0);
        assert_eq!(result.failed, 0);
    }

    // --- load_history date filtering ---

    #[test]
    fn test_load_history_filters_by_days() {
        let dir = TempDir::new().unwrap();

        // Create file with old snapshot (outside window)
        let old_ts = Utc::now() - Duration::days(10);
        let old_snapshot = HealthSnapshot {
            timestamp: old_ts,
            total_kilds: 1,
            working: 1,
            idle: 0,
            stuck: 0,
            crashed: 0,
            avg_cpu_percent: None,
            total_memory_mb: None,
        };
        let old_date = old_ts.format("%Y-%m-%d").to_string();
        let old_path = dir.path().join(format!("{}.json", old_date));
        fs::write(&old_path, serde_json::to_string(&vec![old_snapshot]).unwrap()).unwrap();

        // Create file with recent snapshot (inside window)
        let recent_snapshot = make_test_snapshot(2, 0, 0);
        save_snapshot_to(dir.path(), &recent_snapshot).unwrap();

        // Load with 3-day window — should only get the recent one
        let loaded = load_history_from(dir.path(), 3).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].working, 2);
    }
}
```

**Why**: Tests the filesystem persistence layer with isolated temp directories. Covers round-trip serialization, corruption recovery, date filtering, and cleanup logic.

---

## Patterns to Follow

**From codebase — mirror these exactly:**

```rust
// SOURCE: crates/kild-core/src/cleanup/operations.rs:357-360
// Pattern for filesystem tests with TempDir
use tempfile::TempDir;

#[test]
fn test_detect_stale_sessions_empty_dir() {
    let temp_dir = TempDir::new().unwrap();
    let stale_sessions = detect_stale_sessions(temp_dir.path()).unwrap();
    assert_eq!(stale_sessions.len(), 0);
}
```

```rust
// SOURCE: crates/kild-core/src/sessions/types.rs:463-483
// Pattern for test-only session construction
#[cfg(test)]
pub fn new_for_test(branch: String, worktree_path: PathBuf) -> Self { ... }
```

```rust
// SOURCE: crates/kild-core/src/process/operations.rs:313-341
// Pattern for inline test modules
#[cfg(test)]
mod tests {
    use super::*;
    // tests...
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                        | Mitigation                                                                |
| ------------------------------------- | ------------------------------------------------------------------------- |
| Global `AtomicU64` shared across tests | Tests that modify threshold should reset it; use `set_idle_threshold_minutes(10)` after |
| Time-sensitive tests                  | Use recent timestamps relative to `Utc::now()`, not hardcoded dates       |
| `Session::new_for_test` is `#[cfg(test)]` only | Already available — used across crate in test context                   |
| Storage tests touching real `~/.kild/` | Refactor to accept `&Path` param; tests use `TempDir`                    |
| `load_history_from` with nonexistent dir | Currently warns + returns empty; test confirms this behavior            |

---

## Validation

### Automated Checks

```bash
cargo test -p kild-core -- health     # Run health module tests
cargo clippy --all -- -D warnings     # Lint
cargo fmt --check                     # Format check
cargo build --all                     # Clean build
```

### Manual Verification

1. `cargo test -p kild-core -- health --nocapture` — verify all new tests pass with readable output
2. `cargo test --all` — verify no regressions in other modules

---

## Scope Boundaries

**IN SCOPE:**

- Unit tests for `operations.rs`: `calculate_health_status`, `enrich_session_with_health`, `aggregate_health_stats`, threshold get/set
- Unit tests for `storage.rs`: `HealthSnapshot::from`, `save_snapshot_to`, `load_history_from`, `cleanup_old_history_in`
- Refactoring storage functions to accept `&Path` param for testability (internal `_to`/`_from`/`_in` variants)

**OUT OF SCOPE (do not touch):**

- `handler.rs` tests — requires mocking `sessions::handler` and `process` module; defer to integration tests
- `errors.rs` tests — trivial error type definitions, no logic to test
- `types.rs` tests — data structs with no logic
- Behavioral changes to any health module function
- Adding new health features

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-11T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-334.md`
