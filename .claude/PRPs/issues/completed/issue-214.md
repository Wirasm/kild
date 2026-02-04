# Investigation: Allow kild in a local repo

**Issue**: #214 (https://github.com/Wirasm/kild/issues/214)
**Type**: BUG
**Investigated**: 2026-02-04

### Assessment

| Metric     | Value  | Reasoning                                                                                                                       |
| ---------- | ------ | ------------------------------------------------------------------------------------------------------------------------------- |
| Severity   | MEDIUM | Users with local-only repos are completely blocked from using kild, but `--no-fetch` workaround exists (though not discoverable) |
| Complexity | LOW    | Single function change in `git/handler.rs` — add a remote-existence check before calling `fetch_remote`                         |
| Confidence | HIGH   | Clear root cause at `handler.rs:185-186`, existing patterns in codebase show how to detect missing remotes                      |

---

## Problem Statement

When running `kild create` in a git repository with no remote configured (e.g., a purely local repo), kild fails with: "Git operation failed: Failed to fetch from remote 'origin'". The `fetch_before_create` flag defaults to `true` and `fetch_remote()` executes `git fetch origin main` without first checking whether the remote exists. Local-only repos are a valid use case and should work out of the box without requiring `--no-fetch`.

---

## Analysis

### Root Cause

WHY: `kild create` fails in local repos without remotes
↓ BECAUSE: `fetch_remote()` at `handler.rs:305` runs `git fetch origin main` unconditionally
Evidence: `crates/kild-core/src/git/handler.rs:316-318` - `Command::new("git").args(["fetch", remote, branch])`

↓ BECAUSE: `create_worktree()` calls `fetch_remote()` when `fetch_before_create()` is true (default)
Evidence: `crates/kild-core/src/git/handler.rs:185-186`:
```rust
if git_config.fetch_before_create() {
    fetch_remote(&project.path, git_config.remote(), git_config.base_branch())?;
}
```

↓ ROOT CAUSE: No check for remote existence before attempting fetch. The codebase already handles missing remotes gracefully elsewhere (e.g., `detect_project` at `handler.rs:30-33` uses `.ok()` on `find_remote("origin")`), but this pattern isn't applied at the fetch callsite.

### Affected Files

| File                                            | Lines   | Action | Description                                                   |
| ----------------------------------------------- | ------- | ------ | ------------------------------------------------------------- |
| `crates/kild-core/src/git/handler.rs`           | 184-187 | UPDATE | Add remote-existence check before calling `fetch_remote`      |
| `crates/kild-core/src/git/handler.rs`           | tests   | UPDATE | Add test for create_worktree succeeding in repo without remote |

### Integration Points

- `crates/kild-core/src/sessions/handler.rs:140-147` calls `create_worktree` — no changes needed, receives the fix transparently
- `crates/kild/src/commands.rs:142-152` has fetch-failure hint — still useful for network errors when remote exists but is unreachable
- `resolve_base_commit()` at `handler.rs:355-405` already handles missing remote tracking branches gracefully with HEAD fallback — no changes needed

### Git History

- **Fetch introduced**: `7130cb7` - "fix: fetch latest base branch before creating worktree (#196) (#203)"
- **HEAD fallback added**: `6a77249` - "fix: suppress HEAD fallback warning when --no-fetch is active"
- **Implication**: The fetch was added for collaboration freshness but didn't account for local-only repos

---

## Implementation Plan

### Step 1: Check remote existence before fetching

**File**: `crates/kild-core/src/git/handler.rs`
**Lines**: 184-187
**Action**: UPDATE

**Current code:**
```rust
// Fetch latest base branch from remote if configured
if git_config.fetch_before_create() {
    fetch_remote(&project.path, git_config.remote(), git_config.base_branch())?;
}
```

**Required change:**
```rust
// Fetch latest base branch from remote if configured and remote exists
let remote_exists = repo
    .find_remote(git_config.remote())
    .is_ok();

if git_config.fetch_before_create() && remote_exists {
    fetch_remote(&project.path, git_config.remote(), git_config.base_branch())?;
} else if git_config.fetch_before_create() && !remote_exists {
    info!(
        event = "core.git.fetch_skipped",
        remote = git_config.remote(),
        reason = "remote not configured"
    );
    eprintln!(
        "Note: Remote '{}' not found, branching from local HEAD.",
        git_config.remote()
    );
}
```

**Why**: Uses git2's `find_remote()` to check if the configured remote exists before attempting to fetch. This mirrors the existing pattern at `handler.rs:30-33` used in `detect_project()`. When the remote doesn't exist, the user is informed and execution continues to `resolve_base_commit()` which falls back to HEAD.

### Step 2: Pass remote_exists to resolve_base_commit for correct warning behavior

**File**: `crates/kild-core/src/git/handler.rs`
**Lines**: 189-191
**Action**: UPDATE

**Current code:**
```rust
// Resolve base commit: prefer remote tracking branch, fall back to HEAD
let fetched = git_config.fetch_before_create();
let base_commit = resolve_base_commit(&repo, git_config, fetched)?;
```

**Required change:**
```rust
// Resolve base commit: prefer remote tracking branch, fall back to HEAD
// Only consider fetch "enabled" if remote actually exists — no warning for local repos
let fetched = git_config.fetch_before_create() && remote_exists;
let base_commit = resolve_base_commit(&repo, git_config, fetched)?;
```

**Why**: When there's no remote, `resolve_base_commit` will fall back to HEAD. Without this change, it would print a misleading warning "Remote tracking branch 'origin/main' not found, using local HEAD. Consider running 'git fetch' first." — unhelpful advice for a repo with no remote. By passing `false` for `fetch_was_enabled` when no remote exists, the fallback is silent, matching the expected UX.

### Step 3: Add test for create_worktree in repo without remote

**File**: `crates/kild-core/src/git/handler.rs`
**Action**: UPDATE (add test in existing `mod tests`)

**Test cases to add:**
```rust
#[test]
fn test_create_worktree_succeeds_without_remote() {
    // fetch_before_create=true (default) but no remote configured should succeed
    let temp_dir = create_temp_test_dir("kild_test_no_remote");
    init_test_repo(&temp_dir);

    let project = ProjectInfo::new(
        "test-id".to_string(),
        "test-project".to_string(),
        temp_dir.clone(),
        None,
    );

    let base_dir = create_temp_test_dir("kild_test_no_remote_base");
    let git_config = GitConfig::default(); // fetch_before_create defaults to true

    let result = create_worktree(&base_dir, &project, "test-branch", None, &git_config);
    assert!(
        result.is_ok(),
        "should succeed in repo without remote even with fetch enabled: {:?}",
        result.err()
    );

    // Verify worktree was created and is on the correct branch
    let worktree_info = result.unwrap();
    let wt_repo = Repository::open(&worktree_info.path).unwrap();
    let head = wt_repo.head().unwrap();
    assert_eq!(
        head.shorthand().unwrap(),
        "kild/test-branch",
        "worktree HEAD should be on kild/test-branch"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
    let _ = std::fs::remove_dir_all(&base_dir);
}
```

### Step 4: Update existing test expectations

**File**: `crates/kild-core/src/git/handler.rs`
**Lines**: 1118-1160
**Action**: UPDATE

The existing test `test_create_worktree_fails_when_fetch_fails` uses `remote: Some("nonexistent".to_string())` with `fetch_before_create: Some(true)`. With the fix, this will no longer fail because `find_remote("nonexistent")` returns `Err`, so fetch is skipped.

**Current test expectation:** asserts `FetchFailed` error
**New test expectation:** asserts success (since nonexistent remote is skipped gracefully)

**Required change:** Rename to `test_create_worktree_succeeds_with_nonexistent_remote` and update assertions:
```rust
#[test]
fn test_create_worktree_succeeds_with_nonexistent_remote() {
    // fetch_before_create=true with nonexistent remote should skip fetch and succeed
    let temp_dir = create_temp_test_dir("kild_test_fetch_fail");
    init_test_repo(&temp_dir);

    let project = ProjectInfo::new(
        "test-id".to_string(),
        "test-project".to_string(),
        temp_dir.clone(),
        None,
    );

    let base_dir = create_temp_test_dir("kild_test_fetch_fail_base");
    let git_config = GitConfig {
        remote: Some("nonexistent".to_string()),
        fetch_before_create: Some(true),
        ..GitConfig::default()
    };

    let result = create_worktree(&base_dir, &project, "test-branch", None, &git_config);
    assert!(
        result.is_ok(),
        "should succeed when remote doesn't exist (fetch skipped): {:?}",
        result.err()
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
    let _ = std::fs::remove_dir_all(&base_dir);
}
```

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: crates/kild-core/src/git/handler.rs:30-33
// Pattern for gracefully detecting missing remote
let remote_url = repo
    .find_remote("origin")
    .ok()
    .and_then(|remote| remote.url().map(|s| s.to_string()));
```

```rust
// SOURCE: crates/kild-core/src/git/handler.rs:377-393
// Pattern for silent fallback when remote tracking branch is missing
Err(e) if e.code() == git2::ErrorCode::NotFound => {
    warn!(event = "core.git.base_fallback_to_head", ...);
    if fetch_was_enabled {
        eprintln!("Warning: ...");
    }
    // fall back to HEAD
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                               | Mitigation                                                                                      |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Remote exists but is unreachable (network)   | `fetch_remote` still returns `FetchFailed` — existing behavior preserved, hint in CLI still shown |
| Remote name configured but wrong             | `find_remote()` returns `Err` — same as no remote, fetch skipped gracefully                     |
| User adds remote later                       | Next `kild create` will detect it and fetch normally                                            |
| Existing test expects FetchFailed for no remote | Step 4 updates the test to match new behavior                                                  |

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

1. Create a local git repo without remote: `git init /tmp/test-local && cd /tmp/test-local && git commit --allow-empty -m "init"`
2. Run `kild create test-branch` — should succeed without `--no-fetch`
3. Run `kild create test-branch2` in a repo WITH a remote — should still fetch as before
4. Run `kild create test-branch3 --no-fetch` in a repo with remote — should skip fetch as before

---

## Scope Boundaries

**IN SCOPE:**
- Skip fetch when configured remote doesn't exist in the repository
- Suppress misleading "remote tracking branch not found" warning for local repos
- Update tests to match new behavior

**OUT OF SCOPE (do not touch):**
- Config system changes (no new config options needed)
- CLI error hint changes (still useful for network errors)
- `fetch_remote()` function itself (it works correctly, just shouldn't be called when remote is absent)
- `--no-fetch` flag behavior (still works as before)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-04
- **Artifact**: `.claude/PRPs/issues/issue-214.md`
