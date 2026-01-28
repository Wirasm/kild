# Investigation: Support branch names with slashes (e.g., feature/foo)

**Issue**: #120 (https://github.com/Wirasm/kild/issues/120)
**Type**: BUG
**Investigated**: 2026-01-28T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                  |
| ---------- | ------ | ---------------------------------------------------------------------------------------------------------- |
| Severity   | HIGH   | Blocks common git workflow conventions (feature/, bugfix/, hotfix/ branches); no workaround except renaming |
| Complexity | LOW    | 2 files affected, isolated sanitization function, pattern already exists in codebase                       |
| Confidence | HIGH   | Clear root cause identified with exact code location; error message explicitly shows the path issue         |

---

## Problem Statement

Creating a kild with a branch name containing `/` (e.g., `feature/peek-cli`) fails because the branch name is used directly in filesystem paths and git worktree names, both of which cannot contain directory separators.

---

## Analysis

### Root Cause

**WHY 1**: Why does `kild create feature/peek-cli` fail with "failed to make directory"?
- Because git2 tries to create a worktree named `kild_feature/peek-cli` which contains a `/`
- Evidence: Error message shows path `.git/worktrees/kild_feature/peek-cli`

**WHY 2**: Why does the worktree name contain a `/`?
- Because `format!("kild_{}", validated_branch)` at `handler.rs:205` doesn't sanitize the branch name
- Evidence: `crates/kild-core/src/git/handler.rs:202-206`
```rust
let worktree_name = if use_current {
    validated_branch.clone()
} else {
    format!("kild_{}", validated_branch)
};
```

**WHY 3**: Why isn't the branch name sanitized?
- `validate_branch_name()` intentionally allows `/` because it's valid for git branches
- But git worktree **names** (internal identifier) cannot contain `/`
- Evidence: `crates/kild-core/src/git/operations.rs:48-70` - validation allows `/`

**ROOT CAUSE**: Two locations need sanitization:
1. `calculate_worktree_path()` uses branch directly as path component
2. `create_worktree()` uses branch directly as worktree name

Evidence: `operations.rs:7-9` and `handler.rs:202-206`

### Evidence Chain

```
User: kild create feature/peek-cli

Session creation:
↓ branch = "feature/peek-cli"  (valid git branch name)

operations::calculate_worktree_path() [operations.rs:7-9]
↓ path = ~/.kild/worktrees/project/feature/peek-cli
         ────────────────────────────┘      ────────
         This creates nested dirs:   "feature/peek-cli"

Git worktree name [handler.rs:205]
↓ worktree_name = "kild_feature/peek-cli"
                  ───────────────────────
                  Git worktree names cannot contain "/"

repo.worktree(&worktree_name, ...) [handler.rs:207]
↓ Git2 error: failed to make directory
             .git/worktrees/kild_feature/peek-cli
                            ────────────┘────────
                            "kild_feature" dir doesn't exist
```

### Affected Files

| File                                       | Lines   | Action | Description                                                        |
| ------------------------------------------ | ------- | ------ | ------------------------------------------------------------------ |
| `crates/kild-core/src/git/operations.rs`   | 7-9     | UPDATE | Add `sanitize_for_path()` fn, use in `calculate_worktree_path()`   |
| `crates/kild-core/src/git/handler.rs`      | 202-206 | UPDATE | Sanitize worktree name before `repo.worktree()` call               |
| `crates/kild-core/src/git/operations.rs`   | tests   | UPDATE | Add tests for branch names with slashes                            |

### Integration Points

- `sessions::handler::create_session()` calls `git::handler::create_worktree()` at handler.rs:129
- `git::handler::remove_worktree_by_path()` checks `branch_name.starts_with("kild_")` at handler.rs:431 - no change needed (branch name is correct, worktree name is sanitized)
- Branch deletion uses actual branch name (not worktree name) - no change needed

### Git History

- **Introduced**: a19478fe (2026-01-09) - Original implementation
- **Implication**: Original bug - slash handling was never implemented

---

## Implementation Plan

### Step 1: Add `sanitize_for_path()` function

**File**: `crates/kild-core/src/git/operations.rs`
**Lines**: After line 9 (after `calculate_worktree_path`)
**Action**: UPDATE

**Current code:**
```rust
// Line 7-9
pub fn calculate_worktree_path(base_dir: &Path, project_name: &str, branch: &str) -> PathBuf {
    base_dir.join("worktrees").join(project_name).join(branch)
}
```

**Required change:**
```rust
/// Sanitize a string for safe use in filesystem paths.
///
/// Replaces `/` with `-` to prevent nested directory creation
/// when branch names like `feature/foo` are used.
pub fn sanitize_for_path(s: &str) -> String {
    s.replace('/', "-")
}

pub fn calculate_worktree_path(base_dir: &Path, project_name: &str, branch: &str) -> PathBuf {
    let safe_branch = sanitize_for_path(branch);
    base_dir.join("worktrees").join(project_name).join(safe_branch)
}
```

**Why**: The worktree directory must be a single directory name, not a nested path. Using `-` matches the existing pattern in `process/pid_file.rs:23`.

---

### Step 2: Sanitize worktree name in `create_worktree()`

**File**: `crates/kild-core/src/git/handler.rs`
**Lines**: 202-206
**Action**: UPDATE

**Current code:**
```rust
// Line 202-206
let worktree_name = if use_current {
    validated_branch.clone()
} else {
    format!("kild_{}", validated_branch)
};
```

**Required change:**
```rust
// Line 202-206
let worktree_name = if use_current {
    operations::sanitize_for_path(&validated_branch)
} else {
    format!("kild_{}", operations::sanitize_for_path(&validated_branch))
};
```

**Why**: Git worktree names (the internal identifier in `.git/worktrees/`) cannot contain `/`. The actual git **branch** name remains unchanged (`feature/foo`), only the **worktree name** is sanitized.

---

### Step 3: Add tests for branch names with slashes

**File**: `crates/kild-core/src/git/operations.rs`
**Lines**: In the `mod tests` section (after existing tests)
**Action**: UPDATE

**Test cases to add:**
```rust
#[test]
fn test_sanitize_for_path() {
    assert_eq!(sanitize_for_path("feature/foo"), "feature-foo");
    assert_eq!(sanitize_for_path("bugfix/auth/login"), "bugfix-auth-login");
    assert_eq!(sanitize_for_path("simple-branch"), "simple-branch");
    assert_eq!(sanitize_for_path("no_slashes_here"), "no_slashes_here");
}

#[test]
fn test_calculate_worktree_path_with_slashes() {
    let base = Path::new("/home/user/.kild");

    // Branch with single slash
    let path = calculate_worktree_path(base, "my-project", "feature/auth");
    assert_eq!(
        path,
        PathBuf::from("/home/user/.kild/worktrees/my-project/feature-auth")
    );

    // Branch with multiple slashes
    let path = calculate_worktree_path(base, "my-project", "feature/auth/oauth");
    assert_eq!(
        path,
        PathBuf::from("/home/user/.kild/worktrees/my-project/feature-auth-oauth")
    );

    // Branch without slashes (unchanged behavior)
    let path = calculate_worktree_path(base, "my-project", "simple-branch");
    assert_eq!(
        path,
        PathBuf::from("/home/user/.kild/worktrees/my-project/simple-branch")
    );
}
```

---

## Patterns to Follow

**From codebase - mirror this pattern exactly:**

```rust
// SOURCE: crates/kild-core/src/process/pid_file.rs:21-24
// Pattern for sanitizing session IDs with slashes for filenames
pub fn get_pid_file_path(kild_dir: &Path, session_id: &str) -> PathBuf {
    // Sanitize session_id to be safe for filenames (replace / with -)
    let safe_id = session_id.replace('/', "-");
    kild_dir.join(PID_DIR_NAME).join(format!("{}.pid", safe_id))
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                              | Mitigation                                                                 |
| ------------------------------------------- | -------------------------------------------------------------------------- |
| Existing kilds with non-slashed branch names | No impact - sanitization is a no-op for branches without slashes           |
| Branch names with multiple slashes          | All slashes replaced with `-` (e.g., `a/b/c` -> `a-b-c`)                   |
| Collision: `feature-foo` vs `feature/foo`   | Rare in practice; git would already have same-name collision for branches  |
| Branch deletion after worktree removal      | Uses actual branch name (not sanitized), so no impact (handler.rs:431)     |

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

1. Create a kild with a slashed branch name:
   ```bash
   cargo run -p kild -- create feature/test-slash --note "Testing slash support"
   ```
   Expected: Kild created successfully, worktree at `~/.kild/worktrees/<project>/feature-test-slash`

2. Verify kild list shows the correct branch name:
   ```bash
   cargo run -p kild -- list
   ```
   Expected: Shows `feature/test-slash` as branch name (original, not sanitized)

3. Destroy the kild:
   ```bash
   cargo run -p kild -- destroy feature/test-slash
   ```
   Expected: Kild destroyed, branch deleted if it was `kild_*` prefixed

---

## Scope Boundaries

**IN SCOPE:**
- Sanitizing worktree directory names for filesystem safety
- Sanitizing worktree names for git2 compatibility
- Preserving original branch names for git operations
- Adding tests for slash-containing branches

**OUT OF SCOPE (do not touch):**
- Branch validation logic (`validate_branch_name()`) - slashes are valid git branch chars
- Session ID format (`project/branch`) - already handled by `pid_file.rs`
- Refactoring unrelated code
- Adding new features beyond slash support

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-28T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-120.md`
