# Investigation: Session file unexpectedly disappears during open --all operation

**Issue**: #102 (https://github.com/Wirasm/kild/issues/102)
**Type**: BUG
**Investigated**: 2026-01-28T14:30:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                                           |
| ---------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------- |
| Severity   | HIGH   | Major feature broken: sessions "disappear" from list even though file exists on disk, causing user confusion and potential data loss perception |
| Complexity | MEDIUM | 2-3 files affected (validation.rs, persistence.rs), clear root cause, moderate testing needed                                       |
| Confidence | HIGH   | Clear root cause identified in code path, evidence chain complete, reproducible scenario understood                                 |

---

## Problem Statement

During `open --all` operation, one or more session files appear to "disappear" from `kild list` output. Investigation reveals the **session file still exists on disk** but is silently skipped during loading due to worktree validation failure. The issue title is misleading - files don't disappear, they become invisible because their worktree path no longer exists.

---

## Analysis

### Root Cause / Change Rationale

The session loading mechanism includes overly strict worktree path validation that causes sessions to be silently skipped (not loaded) when their worktree doesn't exist. This creates a perception of "disappeared" sessions.

### Evidence Chain

WHY: Session disappears from `kild list` after `open --all`
↓ BECAUSE: `load_sessions_from_files()` doesn't include the session in returned list
Evidence: `crates/kild-core/src/sessions/persistence.rs:107-117`
```rust
if let Err(validation_error) = super::validation::validate_session_structure(&session) {
    skipped_count += 1;
    tracing::warn!(
        event = "core.session.load_invalid_structure",
        ...
    );
    continue;  // Session is skipped!
}
```

↓ BECAUSE: `validate_session_structure()` fails when worktree path doesn't exist
Evidence: `crates/kild-core/src/sessions/validation.rs:64-71`
```rust
if !session.worktree_path.exists() {
    return Err(SessionError::InvalidStructure {
        field: format!(
            "worktree path does not exist: {}",
            session.worktree_path.display()
        ),
    });
}
```

↓ ROOT CAUSE: **Worktree existence check is too aggressive for session loading**
The validation treats a missing worktree as an "invalid structure" when it's actually a recoverable state. A session file with a missing worktree should still be loadable so users can see it, investigate, and manually clean up.

### Affected Files

| File                                              | Lines   | Action | Description                                                            |
| ------------------------------------------------- | ------- | ------ | ---------------------------------------------------------------------- |
| `crates/kild-core/src/sessions/validation.rs`     | 64-71   | UPDATE | Remove worktree existence check from `validate_session_structure()`    |
| `crates/kild-core/src/sessions/persistence.rs`    | 107-117 | UPDATE | Optionally: Add parameter to control validation strictness             |
| `crates/kild-core/src/sessions/handler.rs`        | 577-581 | NONE   | Already has worktree check in `open_session()` - this is the right place |

### Integration Points

- `crates/kild/src/commands.rs:451` - `list_sessions()` calls `load_sessions_from_files()`
- `crates/kild/src/commands.rs:467` - `open_session()` validates worktree before opening
- `crates/kild-ui/src/actions.rs:89` - GUI refresh uses same loading path
- `crates/kild-core/src/cleanup/operations.rs:287-354` - `detect_stale_sessions()` separately checks worktree existence

### Git History

```bash
git log --oneline -5 -- crates/kild-core/src/sessions/validation.rs
160314d Rebrand Shards to KILD (#110)
```

- **Introduced**: Part of original design, predates KILD rebrand
- **Implication**: Long-standing behavior, not a regression

---

## Implementation Plan

### Step 1: Remove worktree existence check from `validate_session_structure()`

**File**: `crates/kild-core/src/sessions/validation.rs`
**Lines**: 64-71
**Action**: DELETE

**Current code:**
```rust
// Line 64-71
if !session.worktree_path.exists() {
    return Err(SessionError::InvalidStructure {
        field: format!(
            "worktree path does not exist: {}",
            session.worktree_path.display()
        ),
    });
}
```

**Required change:**
Remove this entire block. The worktree existence check should NOT be part of structural validation. Operations that need a worktree (like `open_session`) already have their own checks.

**Why**: Worktree existence is a runtime state, not a structural property. Sessions with missing worktrees are still valid session files - they just can't be operated on until the worktree issue is resolved.

---

### Step 2: Add "orphaned" status indicator to Session type (optional enhancement)

**File**: `crates/kild-core/src/sessions/types.rs`
**Action**: Consider adding a computed field or method

This step is optional but would improve UX by showing sessions with missing worktrees as "orphaned" rather than hiding them entirely.

```rust
impl Session {
    /// Returns true if the session's worktree path exists
    pub fn is_worktree_valid(&self) -> bool {
        self.worktree_path.exists()
    }
}
```

**Why**: Users can then see orphaned sessions in `kild list` with a visual indicator, making debugging easier.

---

### Step 3: Update tests in validation.rs

**File**: `crates/kild-core/src/sessions/validation.rs`
**Action**: UPDATE tests

Remove or update tests that expect worktree existence to fail validation.

---

### Step 4: Add test for loading session with missing worktree

**File**: `crates/kild-core/src/sessions/persistence.rs`
**Action**: ADD test

Add a test that verifies sessions with missing worktrees are still loaded (not skipped):

```rust
#[test]
fn test_load_sessions_includes_missing_worktree() {
    // Create session file with non-existent worktree path
    // Verify it IS included in load_sessions_from_files() result
    // Verify skipped_count is NOT incremented
}
```

---

## Patterns to Follow

**From codebase - worktree validation at operation time:**

```rust
// SOURCE: crates/kild-core/src/sessions/handler.rs:577-581
// Pattern: Validate worktree exists WHEN performing operations, not during load
if !session.worktree_path.exists() {
    return Err(SessionError::WorktreeNotFound {
        path: session.worktree_path.clone(),
    });
}
```

This is the correct pattern - validate at operation time, not at load time.

---

## Edge Cases & Risks

| Risk/Edge Case                       | Mitigation                                                                 |
| ------------------------------------ | -------------------------------------------------------------------------- |
| Stale sessions clutter list          | Cleanup command (`kild cleanup --orphans`) already handles this separately |
| User confusion about orphaned state  | Add visual indicator in `kild list` for sessions with missing worktrees    |
| Cleanup logic affected               | `detect_stale_sessions()` has its own worktree check, unaffected by this fix |

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

1. Create a kild: `kild create test-orphan`
2. Stop it: `kild stop test-orphan`
3. Manually delete the worktree directory: `rm -rf ~/.kild/worktrees/*/test-orphan`
4. List kilds: `kild list`
5. **Expected**: Session still appears (possibly with orphaned indicator)
6. **Before fix**: Session "disappears" from list

---

## Scope Boundaries

**IN SCOPE:**

- Remove worktree existence check from `validate_session_structure()`
- Update related tests
- Ensure sessions with missing worktrees are loadable

**OUT OF SCOPE (do not touch):**

- Operation-level worktree validation (keep checks in `open_session`, `restart_session`, etc.)
- Cleanup logic (`detect_stale_sessions()` - separate concern)
- GUI display changes (separate issue if needed)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-28T14:30:00Z
- **Artifact**: `.claude/PRPs/issues/issue-102.md`
