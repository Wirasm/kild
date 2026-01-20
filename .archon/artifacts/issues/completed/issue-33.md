# Investigation: Add branch validation to shards create command

**Issue**: #33 (https://github.com/Wirasm/shards/issues/33)
**Type**: ENHANCEMENT
**Investigated**: 2026-01-20T15:13:32.129+02:00

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Priority | MEDIUM | Prevents user confusion and duplicate branch issues, but doesn't block core functionality |
| Complexity | MEDIUM | Requires changes to 2-3 files with git integration logic, moderate risk of breaking existing workflows |
| Confidence | HIGH | Clear root cause identified in git/handler.rs line 137, well-understood code path with concrete evidence |

---

## Problem Statement

The `shards create` command always creates new branches with `worktree-{branch-name}` prefix, even when the user is already on a branch that should be used for the work. This leads to duplicate branches and merge conflicts when users expect to work on their current branch.

---

## Analysis

### Root Cause / Change Rationale

The system needs intelligent branch detection to avoid creating duplicate branches when the user is already on an appropriate branch that matches their intended work.

### Evidence Chain

WHY: User gets duplicate branches like `issue-16-empty-terminal` and `worktree-issue-16-empty-terminal`
↓ BECAUSE: System always prefixes branch names with "worktree-" regardless of context
  Evidence: `src/git/handler.rs:137` - `let worktree_name = format!("worktree-{}", validated_branch);`

↓ BECAUSE: No logic exists to check if user is already on an appropriate branch
  Evidence: `src/git/handler.rs:45-150` - No current branch detection in create_worktree function

↓ ROOT CAUSE: Hard-coded worktree naming without branch context awareness
  Evidence: `src/git/handler.rs:137` - Always uses format!("worktree-{}", validated_branch)

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `src/git/operations.rs` | NEW | UPDATE | Add get_current_branch function |
| `src/git/handler.rs` | 45-150 | UPDATE | Add branch validation logic to create_worktree |
| `src/git/handler.rs` | 137 | UPDATE | Make worktree naming conditional |

### Integration Points

- `src/sessions/handler.rs:15` calls `git::handler::create_worktree`
- `src/cli/commands.rs:49` initiates the flow via session handler
- Git2 library integration for branch operations

### Git History

- **Introduced**: a19478fe - 2026-01-09 - "Complete vertical slice architecture implementation"
- **Last modified**: 15841ab - Recent cleanup strategies
- **Implication**: Original design decision, not a regression

---

## Implementation Plan

### Step 1: Add current branch detection utility

**File**: `src/git/operations.rs`
**Lines**: After line 65 (after validate_branch_name)
**Action**: UPDATE

**Required change:**
```rust
pub fn get_current_branch(repo: &git2::Repository) -> Result<Option<String>, GitError> {
    let head = repo.head().map_err(|e| GitError::Git2Error { source: e })?;
    
    if let Some(branch_name) = head.shorthand() {
        Ok(Some(branch_name.to_string()))
    } else {
        Ok(None)
    }
}

pub fn should_use_current_branch(current_branch: &str, requested_branch: &str) -> bool {
    current_branch == requested_branch
}
```

**Why**: Provides pure logic functions for branch detection and comparison

---

### Step 2: Update worktree creation with branch validation

**File**: `src/git/handler.rs`
**Lines**: 45-150
**Action**: UPDATE

**Current code:**
```rust
// Line 137
let worktree_name = format!("worktree-{}", validated_branch);
```

**Required change:**
```rust
// Add after line 95 (after repo opening)
let current_branch = operations::get_current_branch(&repo)?;
let use_current = current_branch
    .as_ref()
    .map(|cb| operations::should_use_current_branch(cb, &validated_branch))
    .unwrap_or(false);

// Replace line 137
let worktree_name = if use_current {
    validated_branch.clone()
} else {
    format!("worktree-{}", validated_branch)
};

// Add logging after worktree creation
info!(
    event = "git.worktree.branch_decision",
    project_id = project.id,
    requested_branch = validated_branch,
    current_branch = current_branch.as_deref().unwrap_or("none"),
    used_current = use_current,
    worktree_name = worktree_name
);
```

**Why**: Implements smart branch detection while maintaining backward compatibility

---

### Step 3: Add error handling for branch conflicts

**File**: `src/git/errors.rs`
**Lines**: After existing error variants
**Action**: UPDATE

**Required change:**
```rust
#[error("Branch '{branch}' conflicts with current branch '{current}'")]
BranchConflict { 
    branch: String, 
    current: String 
},
```

**Why**: Provides clear error messaging for branch validation failures

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: src/git/operations.rs:44-58
// Pattern for validation functions with proper error handling
pub fn validate_branch_name(branch: &str) -> Result<String, GitError> {
    let trimmed = branch.trim();
    
    if trimmed.is_empty() {
        return Err(GitError::OperationFailed {
            message: "Branch name cannot be empty".to_string(),
        });
    }
    
    Ok(trimmed.to_string())
}
```

```rust
// SOURCE: src/git/handler.rs:104-109
// Pattern for structured logging with event names
debug!(
    event = "git.branch.check_completed",
    project_id = project.id,
    branch = validated_branch,
    exists = branch_exists
);
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| Detached HEAD state | Return None from get_current_branch, fall back to worktree- prefix |
| Branch name conflicts | Use existing GitError::WorktreeAlreadyExists handling |
| Backward compatibility | Only change behavior when current branch matches requested branch |

---

## Validation

### Automated Checks

```bash
cargo test git::operations::test_get_current_branch
cargo test git::operations::test_should_use_current_branch
cargo test git::handler::test_create_worktree_with_current_branch
```

### Manual Verification

1. Create shard when on matching branch - should use current branch
2. Create shard when on different branch - should use worktree- prefix
3. Verify no regression in existing workflows

---

## Scope Boundaries

**IN SCOPE:**
- Smart branch detection for exact name matches
- Conditional worktree naming
- Structured logging for branch decisions

**OUT OF SCOPE (do not touch):**
- Complex branch relationship detection (partial matches, etc.)
- CLI flag additions (--use-current, etc.)
- Session persistence changes
- Terminal launching logic

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-20T15:13:32.129+02:00
- **Artifact**: `.archon/artifacts/issues/issue-33.md`
