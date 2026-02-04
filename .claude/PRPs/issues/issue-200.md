# Investigation: Multiple branches created for kilds

**Issue**: #200 (https://github.com/Wirasm/kild/issues/200)
**Type**: BUG
**Investigated**: 2026-02-01T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                                  |
| ---------- | ------ | -------------------------------------------------------------------------------------------------------------------------- |
| Severity   | MEDIUM | Creates an unnecessary branch that pollutes the user's git namespace, but doesn't break functionality; workaround: delete manually |
| Complexity | LOW    | Fix is isolated to one function in `git/handler.rs` — remove the explicit branch creation and let git2 handle it            |
| Confidence | HIGH   | Root cause is clear from code: explicit `repo.branch()` at line 191 creates a branch that's never checked out              |

---

## Problem Statement

When a user runs `kild create my-feature`, KILD creates **two** local branches: `my-feature` and `kild_my-feature`. The `my-feature` branch is created explicitly but never used — the worktree checks out `kild_my-feature` instead. This is confusing and pollutes the git branch namespace.

---

## Analysis

### Root Cause

The `create_worktree` function in `git/handler.rs` performs two separate branch-creation operations:

1. **Line 191**: Explicitly creates branch `<branch>` via `repo.branch(&validated_branch, &head_commit, false)`
2. **Line 208**: Calls `repo.worktree("kild_<branch>", &worktree_path, None)` which implicitly creates a **second** branch named `kild_<branch>` (git2's worktree API creates a branch matching the worktree name)

The branch created at line 191 is orphaned — nothing checks it out, and cleanup at line 430-473 only deletes branches starting with `kild_`.

### Evidence Chain

WHY: Two branches (`my-feature` and `kild_my-feature`) appear after `kild create my-feature`
BECAUSE: KILD explicitly creates `my-feature` branch at `git/handler.rs:191`
Evidence: `crates/kild-core/src/git/handler.rs:178-199`:
```rust
if !branch_exists {
    let head = repo.head().map_err(|e| GitError::Git2Error { source: e })?;
    let head_commit = head
        .peel_to_commit()
        .map_err(|e| GitError::Git2Error { source: e })?;

    repo.branch(&validated_branch, &head_commit, false)
        .map_err(|e| GitError::Git2Error { source: e })?;
}
```

BECAUSE: git2's `repo.worktree()` then creates a second branch `kild_my-feature` at `git/handler.rs:208`
Evidence: `crates/kild-core/src/git/handler.rs:201-209`:
```rust
let worktree_name = if use_current {
    operations::sanitize_for_path(&validated_branch)
} else {
    format!("kild_{}", operations::sanitize_for_path(&validated_branch))
};
repo.worktree(&worktree_name, &worktree_path, None)
    .map_err(|e| GitError::Git2Error { source: e })?;
```

ROOT CAUSE: The explicit `repo.branch()` call at line 191 is unnecessary. The git2 worktree API handles branch creation automatically. The explicit branch creation was likely added as a safety measure but results in an orphaned branch.

### Affected Files

| File                                                | Lines   | Action | Description                                               |
| --------------------------------------------------- | ------- | ------ | --------------------------------------------------------- |
| `crates/kild-core/src/git/handler.rs`               | 165-199 | UPDATE | Remove explicit branch creation; let git2 worktree handle it |
| `crates/kild-core/src/sessions/handler.rs`          | 416-645 | CHECK  | Verify `complete_session` and `get_destroy_safety_info` still work (they reference `kild_` branches, which are unaffected) |
| `crates/kild-core/tests/` (relevant test files)     | -       | UPDATE | Update tests that verify branch creation behavior         |

### Integration Points

- `crates/kild-core/src/sessions/handler.rs:130` calls `git::handler::create_worktree()`
- `crates/kild-core/src/git/handler.rs:430-473` cleanup only deletes `kild_` branches (correct behavior, unaffected)
- `crates/kild-core/src/sessions/handler.rs:416-645` references `kild_` branches for PR checks (unaffected)

### Git History

- **Introduced**: `160314d` - Rebrand Shards to KILD
- **Last modified**: `5d1c242` - Address review feedback: improve comments and add edge case tests
- **Implication**: The dual branch creation has existed since the original implementation and was carried through the rebrand

---

## Implementation Plan

### Step 1: Remove explicit branch creation from `create_worktree`

**File**: `crates/kild-core/src/git/handler.rs`
**Lines**: 165-199
**Action**: UPDATE

**Current code (lines 165-199):**
```rust
// Check if branch exists
let branch_exists = repo
    .find_branch(&validated_branch, BranchType::Local)
    .is_ok();

debug!(
    event = "core.git.branch.check_completed",
    project_id = project.id,
    branch = validated_branch,
    exists = branch_exists
);

// Only create branch if it doesn't exist
if !branch_exists {
    debug!(
        event = "core.git.branch.create_started",
        project_id = project.id,
        branch = validated_branch
    );

    // Create new branch from HEAD
    let head = repo.head().map_err(|e| GitError::Git2Error { source: e })?;
    let head_commit = head
        .peel_to_commit()
        .map_err(|e| GitError::Git2Error { source: e })?;

    repo.branch(&validated_branch, &head_commit, false)
        .map_err(|e| GitError::Git2Error { source: e })?;

    debug!(
        event = "core.git.branch.create_completed",
        project_id = project.id,
        branch = validated_branch
    );
}
```

**Required change:**
Remove the entire block (lines 165-199). The git2 `repo.worktree()` call at line 208 already creates a branch matching the worktree name. No explicit pre-creation is needed.

**Why**: The explicit branch creation produces an orphaned branch (`<branch>`) that is never checked out by the worktree. The worktree API creates its own branch (`kild_<branch>`) automatically.

### Step 2: Verify `use_current` logic still works

**File**: `crates/kild-core/src/git/handler.rs`
**Lines**: 136-141
**Action**: CHECK

The `use_current` logic checks if the current branch matches the requested branch. When `use_current` is true, the worktree name omits the `kild_` prefix:
```rust
let worktree_name = if use_current {
    operations::sanitize_for_path(&validated_branch)
} else {
    format!("kild_{}", operations::sanitize_for_path(&validated_branch))
};
```

When `use_current` is true, git2 will try to create a worktree with the same name as the current branch. This should work because git2 can create a worktree that checks out the existing branch. **However**, this needs testing — git2 may fail if the branch is already checked out in the main worktree.

If the `use_current` path has issues, the fix is to always use the `kild_` prefix (remove the `use_current` optimization entirely).

### Step 3: Update/add tests

**File**: Relevant test files in `crates/kild-core/`
**Action**: UPDATE

**Test cases to verify:**
1. `kild create my-feature` creates only ONE branch: `kild_my-feature`
2. `kild create my-feature` does NOT create a branch named `my-feature`
3. Worktree is checked out on `kild_my-feature`
4. Cleanup (`destroy`) still correctly deletes `kild_my-feature` branch
5. If branch `my-feature` already exists before kild creation, it is not modified or deleted

---

## Patterns to Follow

**From codebase - cleanup pattern to mirror:**
```rust
// SOURCE: crates/kild-core/src/git/handler.rs:430-473
// Only delete branches with kild_ prefix — this remains correct
if let Some(ref branch_name) = branch_name
    && branch_name.starts_with("kild_")
{
    match repo.find_branch(branch_name, BranchType::Local) {
        Ok(mut branch) => match branch.delete() { ... }
    }
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                                                | Mitigation                                                                                         |
| ------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `use_current` path: git2 may fail creating worktree on current branch | Test this path; if it fails, remove `use_current` optimization and always use `kild_` prefix       |
| User expects `<branch>` to exist after creation               | Document that KILD creates `kild_<branch>` branches, not `<branch>` (already in CLAUDE.md)         |
| Existing branch named `<branch>` before kild create           | No longer modified — the explicit branch creation is removed, so existing branches are untouched    |
| Session complete/destroy relies on `kild_` naming             | Unaffected — cleanup already targets `kild_` branches exclusively                                  |

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

1. Run `kild create test-branch` and verify `git branch` shows only `kild_test-branch` (not `test-branch`)
2. Run `kild destroy test-branch` and verify `kild_test-branch` is cleaned up
3. Test with an existing branch: create `my-feature` manually, then `kild create my-feature` — verify `my-feature` is untouched

---

## Scope Boundaries

**IN SCOPE:**
- Remove explicit `repo.branch()` call in `create_worktree`
- Verify `use_current` path still works without pre-created branch
- Update tests

**OUT OF SCOPE (do not touch):**
- Cleanup logic in `remove_worktree` (already correct — targets `kild_` branches)
- Session handler PR checking logic (uses `kild_` branches, unaffected)
- Branch naming/sanitization logic (working correctly)
- Configuration changes

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-01
- **Artifact**: `.claude/PRPs/issues/issue-200.md`
