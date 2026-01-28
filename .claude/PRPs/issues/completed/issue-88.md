# Investigation: Flaky test - test_cleanup_workflow_integration uses shared temp directory

**Issue**: #88 (https://github.com/Wirasm/kild/issues/88)
**Type**: BUG
**Investigated**: 2026-01-28T15:45:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                  |
| ---------- | ------ | ---------------------------------------------------------------------------------------------------------- |
| Severity   | MEDIUM | CI flakiness disrupts workflow, but workaround exists (re-run tests) and no data loss                      |
| Complexity | LOW    | Single file, 8 test functions to update, isolated change with no integration points                        |
| Confidence | HIGH   | Root cause is clear from issue description and code review; `tempfile` pattern already proven in codebase  |

---

## Problem Statement

The test `test_cleanup_workflow_integration` in `crates/kild-core/src/cleanup/operations.rs:546` is flaky because it uses a hardcoded shared temp directory path (`/tmp/kild_cleanup_integration_test`) without cleaning existing files first. When previous test runs leave files behind (crash, interruption, parallel execution), assertions like `assert_eq!(stale_sessions.len(), 1)` fail because they find more files than expected.

---

## Analysis

### Root Cause

WHY: Test assertion `assert_eq!(stale_sessions.len(), 1)` fails with `left: 2, right: 1`
↓ BECAUSE: Directory contains extra `.json` files from previous test runs
Evidence: `operations.rs:565` - assertion expects exactly 1 stale session

↓ BECAUSE: Test directory is not cleaned before use
Evidence: `operations.rs:552` - `let _ = fs::create_dir_all(&temp_dir);` only creates, doesn't remove existing

↓ BECAUSE: Test uses hardcoded, shared temp directory path
Evidence: `operations.rs:551` - `let temp_dir = env::temp_dir().join("kild_cleanup_integration_test");`

↓ ROOT CAUSE: All 8 cleanup tests use hardcoded shared paths instead of unique temp directories
Evidence: Lines 382, 394, 403, 427, 453, 471, 551, 591 all use `env::temp_dir().join("kild_test_*")`

### Evidence Chain

All 8 tests in the test module use this anti-pattern:

| Test Name                                    | Line | Hardcoded Path                          |
| -------------------------------------------- | ---- | --------------------------------------- |
| `test_detect_stale_sessions_empty_dir`       | 382  | `"kild_test_empty_sessions"`            |
| `test_detect_stale_sessions_nonexistent_dir` | 394  | `"kild_test_nonexistent"`               |
| `test_detect_stale_sessions_with_valid_session` | 403  | `"kild_test_valid_session"`          |
| `test_detect_stale_sessions_with_stale_session` | 427  | `"kild_test_stale_session"`          |
| `test_detect_stale_sessions_with_invalid_json` | 453  | `"kild_test_invalid_json"`            |
| `test_detect_stale_sessions_mixed_files`     | 471  | `"kild_test_mixed_files"`               |
| **`test_cleanup_workflow_integration`**      | 551  | **`"kild_cleanup_integration_test"`**   |
| `test_cleanup_workflow_empty_directory`      | 591  | `"kild_cleanup_empty_test"`             |

### Affected Files

| File                                        | Lines   | Action | Description                                  |
| ------------------------------------------- | ------- | ------ | -------------------------------------------- |
| `crates/kild-core/src/cleanup/operations.rs` | 380-601 | UPDATE | Replace 8 tests with `tempfile::TempDir`     |

### Integration Points

- No external callers - these are unit tests
- `detect_stale_sessions()` function being tested is unaffected
- Only test setup/teardown changes

### Git History

- **Introduced**: 15841ab7 - 2026-01-19 - "Initial cleanup module implementation"
- **Last modified**: 160314d - 2026-01-27 - "Rebrand Shards to KILD (#110)" (renamed path constants)
- **Implication**: Original bug, not a regression

---

## Implementation Plan

### Step 1: Add tempfile import to test module

**File**: `crates/kild-core/src/cleanup/operations.rs`
**Lines**: After line 346 (inside `#[cfg(test)]` module)
**Action**: UPDATE

**Current code:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
```

**Required change:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
```

**Why**: Import `TempDir` at module level, remove unused `std::env` import

---

### Step 2: Fix test_detect_stale_sessions_empty_dir

**File**: `crates/kild-core/src/cleanup/operations.rs`
**Lines**: 381-390
**Action**: UPDATE

**Current code:**
```rust
#[test]
fn test_detect_stale_sessions_empty_dir() {
    let temp_dir = std::env::temp_dir().join("kild_test_empty_sessions");
    let _ = std::fs::create_dir_all(&temp_dir);

    let result = detect_stale_sessions(&temp_dir);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 0);

    let _ = std::fs::remove_dir_all(&temp_dir);
}
```

**Required change:**
```rust
#[test]
fn test_detect_stale_sessions_empty_dir() {
    let temp_dir = TempDir::new().unwrap();

    let result = detect_stale_sessions(temp_dir.path());
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 0);
}
```

**Why**: Use `TempDir` for unique, auto-cleaned directory

---

### Step 3: Fix test_detect_stale_sessions_nonexistent_dir

**File**: `crates/kild-core/src/cleanup/operations.rs`
**Lines**: 392-399
**Action**: UPDATE

**Current code:**
```rust
#[test]
fn test_detect_stale_sessions_nonexistent_dir() {
    let nonexistent_dir = std::env::temp_dir().join("kild_test_nonexistent");

    let result = detect_stale_sessions(&nonexistent_dir);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 0);
}
```

**Required change:**
```rust
#[test]
fn test_detect_stale_sessions_nonexistent_dir() {
    let temp_dir = TempDir::new().unwrap();
    let nonexistent_dir = temp_dir.path().join("nonexistent");

    let result = detect_stale_sessions(&nonexistent_dir);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 0);
}
```

**Why**: Create unique nonexistent path within temp directory

---

### Step 4: Fix test_detect_stale_sessions_with_valid_session

**File**: `crates/kild-core/src/cleanup/operations.rs`
**Lines**: 401-423
**Action**: UPDATE

**Current code:**
```rust
#[test]
fn test_detect_stale_sessions_with_valid_session() {
    let temp_dir = env::temp_dir().join("kild_test_valid_session");
    let _ = fs::create_dir_all(&temp_dir);

    // Create a valid session file with existing worktree path
    let session_content = serde_json::json!({
        "id": "test-session",
        "worktree_path": temp_dir.to_str().unwrap(), // Use temp_dir as worktree path (exists)
        "branch": "test-branch",
        "agent": "test-agent"
    });

    let session_file = temp_dir.join("test-session.json");
    fs::write(&session_file, session_content.to_string()).unwrap();

    let result = detect_stale_sessions(&temp_dir);
    assert!(result.is_ok());
    // Should not detect as stale since worktree path exists
    assert_eq!(result.unwrap().len(), 0);

    let _ = fs::remove_dir_all(&temp_dir);
}
```

**Required change:**
```rust
#[test]
fn test_detect_stale_sessions_with_valid_session() {
    let temp_dir = TempDir::new().unwrap();
    let test_path = temp_dir.path();

    // Create a valid session file with existing worktree path
    let session_content = serde_json::json!({
        "id": "test-session",
        "worktree_path": test_path.to_str().unwrap(), // Use temp_dir as worktree path (exists)
        "branch": "test-branch",
        "agent": "test-agent"
    });

    let session_file = test_path.join("test-session.json");
    fs::write(&session_file, session_content.to_string()).unwrap();

    let result = detect_stale_sessions(test_path);
    assert!(result.is_ok());
    // Should not detect as stale since worktree path exists
    assert_eq!(result.unwrap().len(), 0);
}
```

**Why**: Use `TempDir` with auto-cleanup

---

### Step 5: Fix test_detect_stale_sessions_with_stale_session

**File**: `crates/kild-core/src/cleanup/operations.rs`
**Lines**: 425-449
**Action**: UPDATE

**Current code:**
```rust
#[test]
fn test_detect_stale_sessions_with_stale_session() {
    let temp_dir = env::temp_dir().join("kild_test_stale_session");
    let _ = fs::create_dir_all(&temp_dir);

    // Create a stale session file with non-existent worktree path
    let nonexistent_path = temp_dir.join("nonexistent_worktree");
    let session_content = serde_json::json!({
        "id": "stale-session",
        "worktree_path": nonexistent_path.to_str().unwrap(),
        "branch": "stale-branch",
        "agent": "test-agent"
    });

    let session_file = temp_dir.join("stale-session.json");
    fs::write(&session_file, session_content.to_string()).unwrap();

    let result = detect_stale_sessions(&temp_dir);
    assert!(result.is_ok());
    let stale_sessions = result.unwrap();
    assert_eq!(stale_sessions.len(), 1);
    assert_eq!(stale_sessions[0], "stale-session");

    let _ = fs::remove_dir_all(&temp_dir);
}
```

**Required change:**
```rust
#[test]
fn test_detect_stale_sessions_with_stale_session() {
    let temp_dir = TempDir::new().unwrap();
    let test_path = temp_dir.path();

    // Create a stale session file with non-existent worktree path
    let nonexistent_path = test_path.join("nonexistent_worktree");
    let session_content = serde_json::json!({
        "id": "stale-session",
        "worktree_path": nonexistent_path.to_str().unwrap(),
        "branch": "stale-branch",
        "agent": "test-agent"
    });

    let session_file = test_path.join("stale-session.json");
    fs::write(&session_file, session_content.to_string()).unwrap();

    let result = detect_stale_sessions(test_path);
    assert!(result.is_ok());
    let stale_sessions = result.unwrap();
    assert_eq!(stale_sessions.len(), 1);
    assert_eq!(stale_sessions[0], "stale-session");
}
```

**Why**: Use `TempDir` with auto-cleanup

---

### Step 6: Fix test_detect_stale_sessions_with_invalid_json

**File**: `crates/kild-core/src/cleanup/operations.rs`
**Lines**: 451-467
**Action**: UPDATE

**Current code:**
```rust
#[test]
fn test_detect_stale_sessions_with_invalid_json() {
    let temp_dir = env::temp_dir().join("kild_test_invalid_json");
    let _ = fs::create_dir_all(&temp_dir);

    // Create an invalid JSON file
    let session_file = temp_dir.join("invalid-session.json");
    fs::write(&session_file, "invalid json content").unwrap();

    let result = detect_stale_sessions(&temp_dir);
    assert!(result.is_ok());
    let stale_sessions = result.unwrap();
    assert_eq!(stale_sessions.len(), 1);
    assert_eq!(stale_sessions[0], "invalid-session");

    let _ = fs::remove_dir_all(&temp_dir);
}
```

**Required change:**
```rust
#[test]
fn test_detect_stale_sessions_with_invalid_json() {
    let temp_dir = TempDir::new().unwrap();
    let test_path = temp_dir.path();

    // Create an invalid JSON file
    let session_file = test_path.join("invalid-session.json");
    fs::write(&session_file, "invalid json content").unwrap();

    let result = detect_stale_sessions(test_path);
    assert!(result.is_ok());
    let stale_sessions = result.unwrap();
    assert_eq!(stale_sessions.len(), 1);
    assert_eq!(stale_sessions[0], "invalid-session");
}
```

**Why**: Use `TempDir` with auto-cleanup

---

### Step 7: Fix test_detect_stale_sessions_mixed_files

**File**: `crates/kild-core/src/cleanup/operations.rs`
**Lines**: 469-510
**Action**: UPDATE

**Current code:**
```rust
#[test]
fn test_detect_stale_sessions_mixed_files() {
    let temp_dir = env::temp_dir().join("kild_test_mixed_files");
    let _ = fs::create_dir_all(&temp_dir);

    // Create a valid session
    let valid_session = serde_json::json!({
        "id": "valid-session",
        "worktree_path": temp_dir.to_str().unwrap(),
        "branch": "valid-branch",
        "agent": "test-agent"
    });
    fs::write(
        &temp_dir.join("valid-session.json"),
        valid_session.to_string(),
    )
    .unwrap();

    // Create a stale session
    let stale_session = serde_json::json!({
        "id": "stale-session",
        "worktree_path": temp_dir.join("nonexistent").to_str().unwrap(),
        "branch": "stale-branch",
        "agent": "test-agent"
    });
    fs::write(
        &temp_dir.join("stale-session.json"),
        stale_session.to_string(),
    )
    .unwrap();

    // Create a non-JSON file (should be ignored)
    fs::write(&temp_dir.join("not-a-session.txt"), "not json").unwrap();

    let result = detect_stale_sessions(&temp_dir);
    assert!(result.is_ok());
    let stale_sessions = result.unwrap();
    assert_eq!(stale_sessions.len(), 1);
    assert_eq!(stale_sessions[0], "stale-session");

    let _ = fs::remove_dir_all(&temp_dir);
}
```

**Required change:**
```rust
#[test]
fn test_detect_stale_sessions_mixed_files() {
    let temp_dir = TempDir::new().unwrap();
    let test_path = temp_dir.path();

    // Create a valid session
    let valid_session = serde_json::json!({
        "id": "valid-session",
        "worktree_path": test_path.to_str().unwrap(),
        "branch": "valid-branch",
        "agent": "test-agent"
    });
    fs::write(
        test_path.join("valid-session.json"),
        valid_session.to_string(),
    )
    .unwrap();

    // Create a stale session
    let stale_session = serde_json::json!({
        "id": "stale-session",
        "worktree_path": test_path.join("nonexistent").to_str().unwrap(),
        "branch": "stale-branch",
        "agent": "test-agent"
    });
    fs::write(
        test_path.join("stale-session.json"),
        stale_session.to_string(),
    )
    .unwrap();

    // Create a non-JSON file (should be ignored)
    fs::write(test_path.join("not-a-session.txt"), "not json").unwrap();

    let result = detect_stale_sessions(test_path);
    assert!(result.is_ok());
    let stale_sessions = result.unwrap();
    assert_eq!(stale_sessions.len(), 1);
    assert_eq!(stale_sessions[0], "stale-session");
}
```

**Why**: Use `TempDir` with auto-cleanup

---

### Step 8: Fix test_cleanup_workflow_integration (PRIMARY FLAKY TEST)

**File**: `crates/kild-core/src/cleanup/operations.rs`
**Lines**: 545-583
**Action**: UPDATE

**Current code:**
```rust
#[test]
fn test_cleanup_workflow_integration() {
    use std::env;
    use std::fs;

    // Create a temporary directory for testing
    let temp_dir = env::temp_dir().join("kild_cleanup_integration_test");
    let _ = fs::create_dir_all(&temp_dir);

    // Test that all detection functions work together
    let stale_result = detect_stale_sessions(&temp_dir);
    assert!(stale_result.is_ok());

    // Test with a malformed session file
    let malformed_content = "{ invalid json }";
    fs::write(&temp_dir.join("malformed.json"), malformed_content).unwrap();

    let stale_result = detect_stale_sessions(&temp_dir);
    assert!(stale_result.is_ok());
    let stale_sessions = stale_result.unwrap();
    assert_eq!(stale_sessions.len(), 1);
    assert_eq!(stale_sessions[0], "malformed");

    // Test with a valid session file pointing to non-existent worktree
    let valid_session = serde_json::json!({
        "id": "test-session",
        "worktree_path": "/non/existent/path",
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    fs::write(&temp_dir.join("valid.json"), valid_session.to_string()).unwrap();

    let stale_result = detect_stale_sessions(&temp_dir);
    assert!(stale_result.is_ok());
    let stale_sessions = stale_result.unwrap();
    assert_eq!(stale_sessions.len(), 2); // malformed + valid with missing worktree

    // Cleanup
    let _ = fs::remove_dir_all(&temp_dir);
}
```

**Required change:**
```rust
#[test]
fn test_cleanup_workflow_integration() {
    let temp_dir = TempDir::new().unwrap();
    let test_path = temp_dir.path();

    // Test that all detection functions work together
    let stale_result = detect_stale_sessions(test_path);
    assert!(stale_result.is_ok());

    // Test with a malformed session file
    let malformed_content = "{ invalid json }";
    fs::write(test_path.join("malformed.json"), malformed_content).unwrap();

    let stale_result = detect_stale_sessions(test_path);
    assert!(stale_result.is_ok());
    let stale_sessions = stale_result.unwrap();
    assert_eq!(stale_sessions.len(), 1);
    assert_eq!(stale_sessions[0], "malformed");

    // Test with a valid session file pointing to non-existent worktree
    let valid_session = serde_json::json!({
        "id": "test-session",
        "worktree_path": "/non/existent/path",
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    fs::write(test_path.join("valid.json"), valid_session.to_string()).unwrap();

    let stale_result = detect_stale_sessions(test_path);
    assert!(stale_result.is_ok());
    let stale_sessions = stale_result.unwrap();
    assert_eq!(stale_sessions.len(), 2); // malformed + valid with missing worktree
}
```

**Why**: This is the primary flaky test. Using `TempDir` ensures each run has a clean, isolated directory.

---

### Step 9: Fix test_cleanup_workflow_empty_directory

**File**: `crates/kild-core/src/cleanup/operations.rs`
**Lines**: 585-601
**Action**: UPDATE

**Current code:**
```rust
#[test]
fn test_cleanup_workflow_empty_directory() {
    use std::env;
    use std::fs;

    // Test cleanup workflow with empty directory
    let temp_dir = env::temp_dir().join("kild_cleanup_empty_test");
    let _ = fs::create_dir_all(&temp_dir);

    let stale_result = detect_stale_sessions(&temp_dir);
    assert!(stale_result.is_ok());
    let stale_sessions = stale_result.unwrap();
    assert_eq!(stale_sessions.len(), 0);

    // Cleanup
    let _ = fs::remove_dir_all(&temp_dir);
}
```

**Required change:**
```rust
#[test]
fn test_cleanup_workflow_empty_directory() {
    let temp_dir = TempDir::new().unwrap();

    let stale_result = detect_stale_sessions(temp_dir.path());
    assert!(stale_result.is_ok());
    let stale_sessions = stale_result.unwrap();
    assert_eq!(stale_sessions.len(), 0);
}
```

**Why**: Use `TempDir` with auto-cleanup

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: crates/kild-core/src/process/pid_file.rs:173-180
// Pattern for test temp directory setup
use tempfile::TempDir;

#[test]
fn test_ensure_pid_dir() {
    let temp_dir = TempDir::new().unwrap();
    let kild_dir = temp_dir.path();

    let pid_dir = ensure_pid_dir(kild_dir).unwrap();
    assert!(pid_dir.exists());
    // No cleanup needed - TempDir auto-cleans on drop
}
```

```rust
// SOURCE: crates/kild/tests/config_warning.rs:14-15
// Pattern for integration test temp directory
let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
let test_path = temp_dir.path();
```

---

## Edge Cases & Risks

| Risk/Edge Case                  | Mitigation                                                         |
| ------------------------------- | ------------------------------------------------------------------ |
| TempDir cleanup fails           | `TempDir::new().unwrap()` will panic clearly, not silently fail    |
| Parallel test interference      | Each `TempDir` creates unique directory with random suffix         |
| CI environment differences      | `tempfile` uses `TMPDIR`/`TMP` env vars, works across all platforms|

---

## Validation

### Automated Checks

```bash
# Run the specific tests to verify fix
cargo test -p kild-core test_detect_stale_sessions -- --test-threads=1
cargo test -p kild-core test_cleanup_workflow -- --test-threads=1

# Run with parallel threads to verify isolation
cargo test -p kild-core cleanup -- --test-threads=4

# Run full test suite
cargo test --all

# Type check and lint
cargo clippy --all -- -D warnings
cargo fmt --check
```

### Manual Verification

1. Run `test_cleanup_workflow_integration` 10 times in a loop to verify no flakiness:
   ```bash
   for i in {1..10}; do cargo test -p kild-core test_cleanup_workflow_integration; done
   ```
2. Verify no leftover temp directories in `/tmp` matching `kild_*`

---

## Scope Boundaries

**IN SCOPE:**
- Replace 8 hardcoded temp paths with `TempDir::new()`
- Add `tempfile::TempDir` import to test module
- Remove manual cleanup code (`let _ = fs::remove_dir_all`)
- Remove redundant `use std::env;` and local `use std::fs;` imports

**OUT OF SCOPE (do not touch):**
- The `detect_stale_sessions()` function implementation
- Other test modules in the codebase
- Any non-test code
- `test_detect_orphaned_branches_empty_repo` and `test_detect_orphaned_worktrees_error_handling` (these use `/tmp` differently, for a git repo test)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-28T15:45:00Z
- **Artifact**: `.claude/PRPs/issues/issue-88.md`
