# Investigation: kild destroy leaves orphaned kild/* git branches

**Issue**: #352 (https://github.com/Wirasm/kild/issues/352)
**Type**: BUG
**Investigated**: 2026-02-11T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                                            |
| ---------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------ |
| Severity   | LOW    | Cosmetic accumulation of stale branches over time; no data loss, no broken workflows, easy manual workaround (`git branch -d`)       |
| Complexity | LOW    | 2 files need changes, isolated to git operations and cleanup detection; no architectural changes, well-understood code paths          |
| Confidence | HIGH   | Root causes clearly identified in source code with exact line references; both gaps are straightforward prefix-matching bugs          |

---

## Problem Statement

`kild destroy` (especially `--force`) and `kild cleanup` fail to clean up orphaned `kild/*` git branches. The force-destroy path (`remove_worktree_force`) never calls branch deletion. The cleanup detection (`detect_orphaned_branches`) only looks for the legacy `worktree-*` prefix, not the current `kild/` prefix. Over time, power users accumulate dozens of stale local branches.

---

## Analysis

### Root Cause / Change Rationale

### Evidence Chain

WHY: `kild destroy --force` leaves `kild/<branch>` branch behind
↓ BECAUSE: `remove_worktree_force()` only prunes the worktree and deletes the directory
Evidence: `crates/kild-core/src/git/removal.rs:263-302` - no call to `delete_kild_branch_if_managed()`

↓ BECAUSE: The force path was written for directory cleanup only, branch deletion was overlooked
Evidence: `remove_worktree_by_path()` at line 229 DOES call `delete_kild_branch_if_managed()` but `remove_worktree_force()` does not

↓ ROOT CAUSE 1: `remove_worktree_force()` is missing branch deletion logic
Evidence: `crates/kild-core/src/git/removal.rs:263-302`

---

WHY: `kild cleanup` reports "No orphaned resources found" when orphaned `kild/*` branches exist
↓ BECAUSE: `detect_orphaned_branches()` only matches `"worktree-"` prefix
Evidence: `crates/kild-core/src/cleanup/operations.rs:69` - `branch_name.starts_with("worktree-")`

↓ ROOT CAUSE 2: Stale prefix filter from before the `kild/` branch naming convention
Evidence: `crates/kild-core/src/git/naming.rs:16` - `KILD_BRANCH_PREFIX = "kild/"`

---

**Additional robustness gap**: Even in normal (non-force) destroy, `remove_worktree_by_path()` reads the branch name from the worktree HEAD at line 206-214. If the worktree is in a bad state (detached HEAD, corrupted), branch extraction fails silently and `delete_kild_branch_if_managed()` is never called. The session metadata (`session.branch`) already has the branch name, making it the more reliable source.

### Affected Files

| File                                               | Lines   | Action | Description                                                   |
| -------------------------------------------------- | ------- | ------ | ------------------------------------------------------------- |
| `crates/kild-core/src/sessions/destroy.rs`         | 370-387 | UPDATE | Add explicit branch deletion after worktree removal           |
| `crates/kild-core/src/cleanup/operations.rs`       | 65-73   | UPDATE | Fix orphan detection to match `kild/` and legacy `kild_` prefixes |

### Integration Points

- `crates/kild/src/commands/destroy.rs:59` calls `session_ops::destroy_session()`
- `crates/kild-core/src/sessions/complete.rs:123` calls `destroy_session()` (benefits from fix)
- `crates/kild-core/src/cleanup/handler.rs:21` calls `detect_orphaned_branches()`
- `crates/kild-core/src/git/removal.rs:229` existing `delete_kild_branch_if_managed()` helper already handles graceful errors and race conditions

### Git History

- **Introduced**: 959ebf8 - refactor: decompose git/operations.rs into focused modules (#376)
- **Last modified**: 937fdc3 - refactor: decompose sessions/handler.rs into focused modules (#381)
- **Implication**: Long-standing gap from original implementation; `remove_worktree_force` never had branch deletion, `detect_orphaned_branches` was written for the legacy `worktree-*` prefix and never updated for `kild/`

---

## Implementation Plan

### Step 1: Add explicit branch deletion in `destroy_session()`

**File**: `crates/kild-core/src/sessions/destroy.rs`
**Lines**: After line 387 (after worktree removal, before PID cleanup)
**Action**: UPDATE

**Current code:**
```rust
// Line 370-391
// 4. Remove git worktree
if force {
    info!(
        event = "core.session.destroy_worktree_force",
        worktree = %session.worktree_path.display()
    );
    git::removal::remove_worktree_force(&session.worktree_path)
        .map_err(|e| SessionError::GitError { source: e })?;
} else {
    git::removal::remove_worktree_by_path(&session.worktree_path)
        .map_err(|e| SessionError::GitError { source: e })?;
}

info!(
    event = "core.session.destroy_worktree_removed",
    session_id = session.id,
    worktree_path = %session.worktree_path.display()
);

// 5. Clean up PID files (best-effort, don't fail if missing)
```

**Required change:**
```rust
// 4. Remove git worktree
if force {
    info!(
        event = "core.session.destroy_worktree_force",
        worktree = %session.worktree_path.display()
    );
    git::removal::remove_worktree_force(&session.worktree_path)
        .map_err(|e| SessionError::GitError { source: e })?;
} else {
    git::removal::remove_worktree_by_path(&session.worktree_path)
        .map_err(|e| SessionError::GitError { source: e })?;
}

info!(
    event = "core.session.destroy_worktree_removed",
    session_id = session.id,
    worktree_path = %session.worktree_path.display()
);

// 5. Delete local kild branch (best-effort, don't block destroy)
let kild_branch = git::naming::kild_branch_name(&session.branch);
git::removal::delete_branch_if_exists(&session.worktree_path, &kild_branch);

// 6. Clean up PID files (best-effort, don't fail if missing)
```

**Why**: The session metadata (`session.branch`) reliably provides the branch name, unlike reading from worktree HEAD which can fail. This ensures branch deletion happens for both normal and force destroy paths. Non-fatal: uses the existing graceful error handling pattern.

Note: `remove_worktree_by_path()` already attempts branch deletion from worktree HEAD. Adding this explicit call provides a safety net for when that extraction fails AND covers the force path. `delete_kild_branch_if_managed()` already handles the "branch not found" case gracefully (debug log), so double-deletion attempts are safe.

---

### Step 2: Expose `delete_branch_if_exists` as a public helper

**File**: `crates/kild-core/src/git/removal.rs`
**Lines**: After existing `delete_kild_branch_if_managed()` (around line 193)
**Action**: UPDATE

**Current code:**
The existing `delete_kild_branch_if_managed()` is private and takes `(repo, branch_name, worktree_path)` where `repo` is already opened and `worktree_path` is used only for logging.

**Required change:**
Add a public function that opens the repo from the worktree path and delegates:

```rust
/// Delete a local git branch if it exists.
///
/// Opens the main repository from the worktree path and attempts deletion.
/// Best-effort: logs failures but never returns an error, matching the
/// non-fatal pattern used throughout destroy operations.
pub fn delete_branch_if_exists(worktree_path: &Path, branch_name: &str) {
    let repo = match find_main_repository(worktree_path) {
        Ok(repo) => repo,
        Err(_) => {
            // Worktree directory may already be removed by force destroy.
            // Fall back to discovering from current directory.
            match Repository::discover(".") {
                Ok(repo) => repo,
                Err(e) => {
                    warn!(
                        event = "core.git.branch.delete_repo_not_found",
                        branch = branch_name,
                        error = %e,
                    );
                    return;
                }
            }
        }
    };

    delete_kild_branch_if_managed(&repo, branch_name, worktree_path);
}
```

**Why**: `destroy_session()` needs to delete the branch after the worktree directory may already be removed (especially in force mode). The existing `find_main_repository()` traverses parent directories which may fail if the worktree is gone. Falling back to `Repository::discover(".")` (which discovers from CWD, typically the main repo) handles this case.

---

### Step 3: Fix orphan detection prefix in `detect_orphaned_branches()`

**File**: `crates/kild-core/src/cleanup/operations.rs`
**Lines**: 65-73
**Action**: UPDATE

**Current code:**
```rust
// Line 65-73
// Check each branch to see if it's orphaned
for (branch, _) in branches.flatten() {
    if let Some(branch_name) = branch.name().ok().flatten() {
        // Check if this is a worktree branch that's not actively used
        if branch_name.starts_with("worktree-") && !active_branches.contains(branch_name) {
            orphaned_branches.push(branch_name.to_string());
        }
    }
}
```

**Required change:**
```rust
// Check each branch to see if it's orphaned
for (branch, _) in branches.flatten() {
    if let Some(branch_name) = branch.name().ok().flatten() {
        // Check if this is a kild-managed branch that's not actively used by a worktree
        let is_kild_branch = branch_name.starts_with(crate::git::naming::KILD_BRANCH_PREFIX)
            || branch_name.starts_with("kild_");
        if is_kild_branch && !active_branches.contains(branch_name) {
            orphaned_branches.push(branch_name.to_string());
        }
    }
}
```

**Why**: Updates the stale `"worktree-"` prefix to match the actual naming convention (`kild/` and legacy `kild_`). Uses the `KILD_BRANCH_PREFIX` constant from `naming.rs` for consistency. The existing `active_branches` set correctly excludes branches still checked out in worktrees.

Note: The `use` imports at the top of `operations.rs` don't currently include anything from `crate::git::naming`. This import needs to be added, OR use the literal `"kild/"` string. Using the constant is preferred for consistency with `removal.rs:119`.

---

### Step 4: Update the `removal.rs` module's public exports

**File**: `crates/kild-core/src/git/removal.rs`
**Action**: UPDATE

Ensure `delete_branch_if_exists` is exported. Check the `mod.rs` re-exports if needed.

---

### Step 5: Add tests

**File**: `crates/kild-core/src/cleanup/operations.rs` (existing test module)
**Action**: UPDATE

**Test cases to add:**

```rust
#[test]
fn test_detect_orphaned_branches_finds_kild_prefix() {
    // Set up a test repo with a kild/test-feature branch
    // that has no active worktree
    // Verify it's detected as orphaned
}

#[test]
fn test_detect_orphaned_branches_finds_legacy_kild_prefix() {
    // Set up a test repo with a kild_test-feature branch
    // Verify it's detected as orphaned
}

#[test]
fn test_detect_orphaned_branches_excludes_active_worktree_branches() {
    // Set up a test repo with a kild/test branch that IS checked out
    // in an active worktree
    // Verify it's NOT detected as orphaned
}
```

**File**: `crates/kild-core/src/git/removal.rs` (existing test module)
**Action**: UPDATE

```rust
#[test]
fn test_remove_worktree_force_cleans_up_branch() {
    // Mirror the existing test_remove_worktree_cleans_up_legacy_kild_prefix
    // but use remove_worktree_force and verify branch is deleted via
    // the new explicit deletion path (which will be called from destroy_session)
}
```

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: crates/kild-core/src/git/removal.rs:143-193
// Pattern for best-effort branch deletion with graceful error handling
fn delete_kild_branch_if_managed(repo: &Repository, branch_name: &str, worktree_path: &Path) {
    if !is_kild_managed_branch(branch_name) {
        return;
    }
    let mut branch = match repo.find_branch(branch_name, BranchType::Local) {
        Ok(branch) => branch,
        Err(e) => {
            debug!(event = "core.git.branch.not_found_for_cleanup", ...);
            return;
        }
    };
    match branch.delete() {
        Ok(()) => { info!(event = "core.git.branch.delete_completed", ...); }
        Err(e) => {
            // Handle race conditions vs real failures
            if error_msg.contains("not found") || error_msg.contains("does not exist") {
                debug!(event = "core.git.branch.delete_race_condition", ...);
            } else {
                warn!(event = "core.git.branch.delete_failed", ...);
            }
        }
    }
}
```

```rust
// SOURCE: crates/kild-core/src/git/removal.rs:119-124
// Pattern for checking kild-managed branches (both current and legacy)
fn is_kild_managed_branch(branch_name: &str) -> bool {
    branch_name.starts_with(naming::KILD_BRANCH_PREFIX) || branch_name.starts_with("kild_")
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                                    | Mitigation                                                                         |
| ------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Branch already deleted (double deletion)          | `delete_kild_branch_if_managed` handles "not found" gracefully (debug log)         |
| Branch checked out in another worktree            | git2 `branch.delete()` will error; logged as warn, destroy continues               |
| Repo discovery fails after force directory delete | Fall back to `Repository::discover(".")` which uses CWD                            |
| Cleanup detects branch that user manually created | Only matches `kild/` prefix + not in active worktree; false positive unlikely      |
| Race condition: branch deleted between scan/clean | Existing race condition handling in `cleanup_orphaned_branches()` (handler.rs:386) |

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

1. `kild create test-branch --no-daemon --no-agent && kild destroy test-branch --force && git branch | grep kild/test-branch` should return nothing
2. Create and destroy several kilds, then run `kild cleanup` - should detect and offer to clean orphaned branches
3. `kild create test-branch --no-daemon --no-agent && kild destroy test-branch && git branch | grep kild/test-branch` should return nothing (normal destroy)

---

## Scope Boundaries

**IN SCOPE:**

- `destroy_session()`: Add explicit branch deletion after worktree removal (covers both normal and force paths)
- `detect_orphaned_branches()`: Fix prefix filter from `"worktree-"` to `kild/` + `kild_`
- `removal.rs`: Add public `delete_branch_if_exists()` helper
- Tests for new behavior

**OUT OF SCOPE (do not touch):**

- Remote branch deletion (handled by `kild complete`, not `kild destroy`)
- `remove_worktree_by_path()` internal logic (already has branch deletion; the session-level explicit call is the safety net)
- `remove_worktree_force()` internals (add explicit call at session level instead)
- Cleanup CLI display/formatting
- Any changes to `kild complete` (it already calls `destroy_session` which will now handle local branch cleanup)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-11T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-352.md`
