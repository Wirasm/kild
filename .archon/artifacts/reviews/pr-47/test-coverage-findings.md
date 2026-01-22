# Test Coverage Findings: PR #47

**Reviewer**: test-coverage-agent
**Date**: 2026-01-21T13:45:00Z
**Source Files**: 6
**Test Files**: 1

---

## Summary

This PR adds terminal window closing functionality during session destruction. Test coverage is mixed: the new `terminal_type` field in Session has excellent backward compatibility tests, but the core `close_terminal_window()` function and `close_terminal()` handler lack unit tests for error handling paths and edge cases. The critical integration point in `destroy_session()` has no test coverage.

**Verdict**: REQUEST_CHANGES

---

## Coverage Map

| Source File | Test File | New Code Tested | Modified Code Tested |
|-------------|-----------|-----------------|---------------------|
| `src/terminal/operations.rs` | `src/terminal/operations.rs` (inline) | PARTIAL | N/A |
| `src/terminal/handler.rs` | `src/terminal/handler.rs` (inline) | NONE | N/A |
| `src/sessions/handler.rs` | `src/sessions/handler.rs` (inline) | NONE | N/A |
| `src/sessions/types.rs` | `src/sessions/types.rs` (inline) | FULL | FULL |
| `src/terminal/errors.rs` | (no tests) | NONE | N/A |
| `src/terminal/types.rs` | (no tests) | N/A | FULL (serde derives) |

---

## Findings

### Finding 1: Missing Test for `close_terminal()` Handler Function

**Severity**: HIGH
**Category**: missing-test
**Location**: `src/terminal/handler.rs:183-200` (source) / `src/terminal/handler.rs` (test)
**Criticality Score**: 8

**Issue**:
The new `close_terminal()` handler function has no unit tests. This function is the public API called by `destroy_session()` and contains important error-swallowing logic that should be verified.

**Untested Code**:
```rust
// src/terminal/handler.rs:183-200
pub fn close_terminal(terminal_type: &TerminalType) -> Result<(), TerminalError> {
    info!(event = "terminal.close_started", terminal_type = %terminal_type);

    let result = operations::close_terminal_window(terminal_type);

    match &result {
        Ok(()) => info!(event = "terminal.close_completed", terminal_type = %terminal_type),
        Err(e) => warn!(
            event = "terminal.close_failed",
            terminal_type = %terminal_type,
            error = %e,
            message = "Continuing with destroy despite terminal close failure"
        ),
    }

    // Always return Ok - terminal close failure should not block destroy
    Ok(())
}
```

**Why This Matters**:
- If the error-swallowing behavior changes (returning `Err` instead of `Ok`), `destroy_session` would fail on terminal close errors
- The logging behavior for failures is critical for debugging but isn't tested
- Contract: "terminal close failure should not block destroy" is not verified by tests

---

#### Test Suggestions

| Option | Approach | Catches | Effort |
|--------|----------|---------|--------|
| A | Unit test with mock operations | Error swallowing, logging | MED |
| B | Integration test calling close_terminal directly | End-to-end behavior | LOW |

**Recommended**: Option B

**Reasoning**:
- Matches existing test patterns in handler.rs (simple direct calls)
- Verifies the most critical invariant: always returns Ok
- Low effort, high value

**Recommended Test**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_close_terminal_returns_ok_for_all_terminal_types() {
        // close_terminal should always return Ok, even if the underlying
        // operation fails (error swallowing is by design)
        let terminal_types = vec![
            TerminalType::ITerm,
            TerminalType::TerminalApp,
            TerminalType::Ghostty,
            TerminalType::Native,
        ];

        for terminal_type in terminal_types {
            let result = close_terminal(&terminal_type);
            assert!(result.is_ok(),
                "close_terminal should always return Ok for {:?}", terminal_type);
        }
    }
}
```

**Test Pattern Reference**:
```rust
// SOURCE: src/terminal/handler.rs:206-210
// This is how similar functionality is tested
#[test]
fn test_detect_available_terminal() {
    // This test depends on the system environment
    let _result = detect_available_terminal();
    // We can't assert specific results since it depends on what's installed
}
```

---

### Finding 2: No Test for `destroy_session()` Terminal Close Integration

**Severity**: CRITICAL
**Category**: missing-test
**Location**: `src/sessions/handler.rs:184-189` (source)
**Criticality Score**: 9

**Issue**:
The integration of terminal closing into `destroy_session()` is completely untested. This is the most critical code path added by this PR - where terminal close is called before killing the process.

**Untested Code**:
```rust
// src/sessions/handler.rs:184-189
// 2. Close terminal window first (before killing process)
if let Some(ref terminal_type) = session.terminal_type {
    info!(event = "session.destroy_close_terminal", terminal_type = %terminal_type);
    // Best-effort - don't fail destroy if terminal close fails
    let _ = terminal::handler::close_terminal(terminal_type);
}
```

**Why This Matters**:
- If the conditional check is inverted or removed, terminal close would be skipped
- If `let _` is changed to proper error handling that returns Err, destroy would fail
- The ordering (close terminal BEFORE kill process) is critical but not verified
- Session data flow (terminal_type from session to close_terminal) not verified

---

#### Test Suggestions

| Option | Approach | Catches | Effort |
|--------|----------|---------|--------|
| A | Unit test with mocked dependencies | All edge cases | HIGH |
| B | Integration test with real session | End-to-end flow | MED |
| C | Test that destroy succeeds when terminal_type is Some | Basic happy path | LOW |

**Recommended**: Option C (minimum) + Option A (ideal)

**Reasoning**:
- Existing `destroy_session` tests only check the "not found" case
- A test with a session that has `terminal_type: Some(...)` would verify the new code path is executed
- Mocking is complex in this codebase, so integration-style tests are more practical

**Recommended Test**:
```rust
#[test]
fn test_destroy_session_with_terminal_type() {
    // This test requires a full integration setup which is complex.
    // At minimum, verify that a session with terminal_type can be
    // created and the field is properly persisted/loaded.

    use std::fs;
    use crate::sessions::operations;
    use crate::terminal::types::TerminalType;

    let temp_dir = std::env::temp_dir().join("shards_test_destroy_terminal");
    let _ = fs::remove_dir_all(&temp_dir);
    let sessions_dir = temp_dir.join("sessions");
    fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");

    let worktree_path = temp_dir.join("worktree");
    fs::create_dir_all(&worktree_path).expect("Failed to create worktree");

    let session = Session {
        id: "test-project_test-branch".to_string(),
        project_id: "test-project".to_string(),
        branch: "test-branch".to_string(),
        worktree_path,
        agent: "test-agent".to_string(),
        status: SessionStatus::Active,
        created_at: chrono::Utc::now().to_rfc3339(),
        port_range_start: 3000,
        port_range_end: 3009,
        port_count: 10,
        process_id: None,
        process_name: None,
        process_start_time: None,
        terminal_type: Some(TerminalType::ITerm), // <-- Key field being tested
        command: "test-command".to_string(),
        last_activity: Some(chrono::Utc::now().to_rfc3339()),
    };

    operations::save_session_to_file(&session, &sessions_dir).expect("Failed to save");

    // Verify terminal_type is persisted correctly
    let loaded = operations::find_session_by_name(&sessions_dir, "test-branch")
        .expect("Failed to find")
        .expect("Session not found");

    assert_eq!(loaded.terminal_type, Some(TerminalType::ITerm));

    let _ = fs::remove_dir_all(&temp_dir);
}
```

---

### Finding 3: `close_terminal_window()` Error Handling Paths Untested

**Severity**: MEDIUM
**Category**: missing-edge-case
**Location**: `src/terminal/operations.rs:200-217` (source)
**Criticality Score**: 6

**Issue**:
The error handling logic in `close_terminal_window()` has multiple branches that aren't fully tested. The existing test only checks that calling the function with no windows returns Ok, but doesn't verify the specific error handling paths.

**Untested Code**:
```rust
// src/terminal/operations.rs:200-217
if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Don't fail if window was already closed - this is expected behavior
    if stderr.contains("window") || stderr.contains("count") {
        debug!(...);
        return Ok(());
    }
    warn!(...);
    // Non-fatal - don't block destroy on terminal close failure
    return Ok(());
}
```

**Why This Matters**:
- The string matching logic (`stderr.contains("window") || stderr.contains("count")`) is fragile
- If AppleScript error messages change, behavior could change unexpectedly
- The fact that ALL non-success statuses return Ok is critical but not explicitly tested

---

#### Test Suggestions

| Option | Approach | Catches | Effort |
|--------|----------|---------|--------|
| A | Test with mocked Command output | All error paths | HIGH |
| B | Document behavior in existing test | Clarifies intent | LOW |
| C | Test for each terminal type | Per-terminal behavior | MED |

**Recommended**: Option B + expand existing test

**Reasoning**:
- Mocking `std::process::Command` is complex
- The existing `test_close_terminal_window_graceful_fallback` test should be expanded with comments documenting expected behavior

**Recommended Test**:
```rust
#[cfg(target_os = "macos")]
#[test]
fn test_close_terminal_window_always_succeeds() {
    // close_terminal_window is designed to ALWAYS return Ok
    // regardless of whether:
    // 1. The terminal app is not running
    // 2. No windows are open
    // 3. AppleScript fails with any error
    //
    // This is critical for destroy_session to not fail on terminal issues

    for terminal_type in &[
        TerminalType::ITerm,
        TerminalType::TerminalApp,
        TerminalType::Ghostty,
    ] {
        let result = close_terminal_window(terminal_type);
        assert!(result.is_ok(),
            "close_terminal_window should never fail for {:?}", terminal_type);
    }
}
```

---

### Finding 4: `TerminalCloseFailed` Error Variant Never Used

**Severity**: LOW
**Category**: weak-test
**Location**: `src/terminal/errors.rs:26-27` (source)
**Criticality Score**: 3

**Issue**:
A new error variant `TerminalCloseFailed` was added but is never actually used in the codebase. The close operations always return `Ok(())` even on failure.

**Untested Code**:
```rust
// src/terminal/errors.rs:26-27
#[error("Failed to close terminal window for {terminal}: {message}")]
TerminalCloseFailed { terminal: String, message: String },
```

**Why This Matters**:
- Dead code adds maintenance burden
- The error code mapping exists but will never be hit
- Could confuse future developers about error handling expectations

---

#### Test Suggestions

| Option | Approach | Catches | Effort |
|--------|----------|---------|--------|
| A | Remove unused error variant | Cleaner code | LOW |
| B | Use the error variant where appropriate | Proper error handling | MED |
| C | Document why it's intentionally unused | Future-proofing | LOW |

**Recommended**: Option A or C

**Reasoning**:
- If terminal close is truly meant to be non-fatal forever, remove the variant
- If it might be used in future (e.g., force-close flag), add a comment explaining this

---

### Finding 5: Test Fixture Updates Are Correct But Minimal

**Severity**: LOW
**Category**: weak-test
**Location**: `src/sessions/operations.rs:438-952` (test fixtures)
**Criticality Score**: 2

**Issue**:
All existing test fixtures were updated to include `terminal_type: None`, which is correct for backward compatibility. However, none of the tests actually verify behavior WITH a terminal_type value.

**Why This Matters**:
- Tests pass but don't exercise the new functionality
- Only backward compatibility is tested, not forward compatibility
- Session round-trip with terminal_type is only tested in types.rs, not operations.rs

---

#### Test Suggestions

| Option | Approach | Catches | Effort |
|--------|----------|---------|--------|
| A | Add tests with terminal_type: Some(...) | Full coverage | MED |
| B | Rely on types.rs tests | Minimal duplication | LOW |

**Recommended**: Option B (acceptable given types.rs coverage)

---

## Test Quality Audit

| Test | Tests Behavior | Resilient | Meaningful Assertions | Verdict |
|------|---------------|-----------|----------------------|---------|
| `test_close_terminal_scripts_defined` | YES | YES | YES | GOOD |
| `test_close_terminal_window_graceful_fallback` | PARTIAL | NO (system-dependent) | YES | NEEDS_WORK |
| `test_session_with_terminal_type` | YES | YES | YES | GOOD |
| `test_session_backward_compatibility_terminal_type` | YES | YES | YES | GOOD |
| `test_create_list_destroy_integration_flow` | YES | YES | YES | GOOD |

---

## Statistics

| Severity | Count | Criticality 8-10 | Criticality 5-7 | Criticality 1-4 |
|----------|-------|------------------|-----------------|-----------------|
| CRITICAL | 1 | 1 | - | - |
| HIGH | 1 | 1 | - | - |
| MEDIUM | 1 | - | 1 | - |
| LOW | 2 | - | - | 2 |

---

## Risk Assessment

| Untested Area | Failure Mode | User Impact | Priority |
|---------------|--------------|-------------|----------|
| `destroy_session` integration | Terminal close skipped or blocks destroy | Windows don't close OR destroy fails | CRITICAL |
| `close_terminal` handler | Error returned instead of swallowed | Destroy fails when terminal app not running | HIGH |
| AppleScript error matching | Wrong error path taken | Unexpected warnings in logs | MED |

---

## Patterns Referenced

| Test File | Lines | Pattern |
|-----------|-------|---------|
| `src/terminal/handler.rs` | 206-210 | System-dependent tests that verify function doesn't panic |
| `src/sessions/types.rs` | 183-232 | Serialization round-trip tests for new fields |
| `src/sessions/handler.rs` | 396-468 | Integration tests using temp directories |

---

## Positive Observations

1. **Excellent backward compatibility tests**: The `test_session_backward_compatibility_terminal_type` test properly verifies that sessions without the new field can still be deserialized
2. **Good serialization round-trip test**: The `test_session_with_terminal_type` test verifies the new field survives JSON serialization
3. **Script definition test**: The `test_close_terminal_scripts_defined` test ensures the close script constants aren't accidentally removed
4. **Test fixture updates**: All 14 test fixtures in operations.rs were correctly updated with the new field
5. **Graceful fallback test**: The `test_close_terminal_window_graceful_fallback` test verifies the critical "no error on missing window" behavior

---

## Metadata

- **Agent**: test-coverage-agent
- **Timestamp**: 2026-01-21T13:45:00Z
- **Artifact**: `.archon/artifacts/reviews/pr-47/test-coverage-findings.md`
