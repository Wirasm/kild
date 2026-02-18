# Investigation: cleanup --orphans removes worktrees with uncommitted changes and active processes

**Issue**: #496 (https://github.com/Wirasm/kild/issues/496)
**Type**: BUG
**Investigated**: 2026-02-18T00:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                      |
| ---------- | ------ | ---------------------------------------------------------------------------------------------- |
| Severity   | HIGH   | Confirmed data loss in production: active kilds with uncommitted changes were destroyed by an e2e test cleanup call |
| Complexity | MEDIUM | 5 files touched, but all changes are additive (new safety check layer, new `force` parameter threaded down the call chain) |
| Confidence | HIGH   | Root cause is unambiguous: `cleanup_orphaned_worktrees` calls `git::removal::remove_worktree_by_path` with zero pre-flight checks; the fix pattern exists in `sessions/destroy.rs` |

---

## Problem Statement

`kild cleanup --orphans` identifies worktrees under `~/.kild/worktrees/<project>/` that lack a session file and removes them immediately via `git::removal::remove_worktree_by_path`. No uncommitted change detection, no active process check, and no `--force` flag exist in this path. This directly caused data loss: an e2e test's `cleanup --orphans` call destroyed active kilds (`split-oversized-files`, `per-session-dirs`) that had uncommitted work and running agents.

---

## Analysis

### Root Cause

WHY: `kild cleanup --orphans` destroyed worktrees with uncommitted changes and active agents
↓ BECAUSE: `cleanup_orphaned_worktrees` calls `remove_worktree_by_path` directly for every orphan
Evidence: `crates/kild-core/src/cleanup/handler.rs:444` — `git::removal::remove_worktree_by_path(worktree_path)` called inside the loop with no pre-check

↓ BECAUSE: The orphan detection criterion is purely "no session file" — valid but insufficient
Evidence: `crates/kild-core/src/cleanup/operations.rs:208-287` — `detect_untracked_worktrees` only checks `session_worktree_paths.contains(&worktree_path_str)`. Worktrees with active work that lost their session file (due to test teardown, bugs, or manual deletion) satisfy this criterion immediately.

↓ BECAUSE: `remove_worktree_by_path` uses git2's `prune(valid=true)` + `remove_dir_all` — no refusal on dirty state
Evidence: `crates/kild-core/src/git/removal.rs:251-258` — `prune_options.valid(true)` bypasses git CLI's own "uncommitted changes" refusal. `fs::remove_dir_all` then deletes the directory unconditionally.

ROOT CAUSE: `cleanup_orphaned_worktrees` (`handler.rs:428-467`) performs no safety checks before calling `remove_worktree_by_path`. The `destroy` command already has these checks (`get_destroy_safety_info` → `get_worktree_status` → `DestroySafetyInfo::should_block()`), but the cleanup path never calls them.

### Evidence Chain

**`handler.rs:428-466`** — The entire `cleanup_orphaned_worktrees` function:
```rust
fn cleanup_orphaned_worktrees(
    worktree_paths: &[std::path::PathBuf],
) -> Result<Vec<std::path::PathBuf>, CleanupError> {
    if worktree_paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut cleaned_worktrees = Vec::new();
    for worktree_path in worktree_paths {
        // ← NO safety check here
        match git::removal::remove_worktree_by_path(worktree_path) {
            Ok(()) => { cleaned_worktrees.push(worktree_path.clone()); }
            Err(e) => { return Err(CleanupError::CleanupFailed { ... }); }
        }
    }
    Ok(cleaned_worktrees)
}
```

**`git/status/worktree.rs:55-80`** — Reusable function that only needs a `&Path`:
```rust
pub fn get_worktree_status(worktree_path: &Path) -> Result<WorktreeStatus, GitError> {
    let repo = Repository::open(worktree_path)?;
    let (uncommitted_result, status_check_failed) = check_uncommitted_changes(&repo);
    let commit_counts = count_unpushed_commits(&repo);
    // ...
    Ok(WorktreeStatus { has_uncommitted_changes, ... })
}
```
This function does not require a session file — it only needs the path. The destroy path uses it via `get_destroy_safety_info`, but cleanup can call it directly.

**`sessions/types/safety.rs:28-30`** — The blocking decision:
```rust
pub fn should_block(&self) -> bool {
    self.git_status.has_uncommitted_changes
}
```

### Affected Files

| File                                               | Lines    | Action | Description                                              |
| -------------------------------------------------- | -------- | ------ | -------------------------------------------------------- |
| `crates/kild-core/src/cleanup/types.rs`            | 29-76    | UPDATE | Add `skipped_worktrees` field and `add_skipped_worktree` method to `CleanupSummary` |
| `crates/kild-core/src/process/operations.rs`       | NEW fn   | UPDATE | Add `find_processes_in_directory(dir: &Path) -> Vec<u32>` using sysinfo |
| `crates/kild-core/src/process/mod.rs`              | 7-9      | UPDATE | Export `find_processes_in_directory` |
| `crates/kild-core/src/cleanup/handler.rs`          | 92, 194, 428 | UPDATE | Add `force: bool` param to 3 functions; add safety checks in `cleanup_orphaned_worktrees` |
| `crates/kild-core/src/cleanup/mod.rs`              | 8-11     | UPDATE | Re-export updated `cleanup_all_with_strategy` and `cleanup_orphaned_resources` signatures |
| `crates/kild/src/app/misc.rs`                      | 4-38     | UPDATE | Add `--force` arg to `cleanup_command()` |
| `crates/kild/src/commands/cleanup.rs`              | 26, 30-57 | UPDATE | Read `--force`, pass to core, display skipped worktrees |

### Integration Points

- `crates/kild/src/commands/cleanup.rs:26` calls `cleanup::cleanup_all_with_strategy(strategy)` — signature change here
- `crates/kild-core/src/cleanup/handler.rs:211` `cleanup_all_with_strategy` calls `cleanup_orphaned_resources` — needs `force` threaded
- `crates/kild-core/src/cleanup/handler.rs:122` `cleanup_orphaned_resources` calls `cleanup_orphaned_worktrees` — needs `force` threaded
- `crates/kild-core/src/cleanup/handler.rs:176` `cleanup_all` (legacy path) calls `cleanup_orphaned_resources` — pass `false` as default
- `crates/kild-core/src/git/mod.rs:28` re-exports `get_worktree_status` — already accessible as `crate::git::get_worktree_status`
- `crates/kild-core/src/process/mod.rs:7-9` exports process functions — add `find_processes_in_directory`

### Git History

Not relevant — the `cleanup --orphans` feature predates this worktree; the missing safety check was present from the start.

---

## Implementation Plan

### Step 1: Add `skipped_worktrees` to `CleanupSummary`

**File**: `crates/kild-core/src/cleanup/types.rs`
**Lines**: 29-76
**Action**: UPDATE

**Current code:**
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct CleanupSummary {
    pub orphaned_branches: Vec<String>,
    pub orphaned_worktrees: Vec<PathBuf>,
    pub stale_sessions: Vec<String>,
    pub total_cleaned: usize,
}

impl CleanupSummary {
    pub fn new() -> Self {
        Self {
            orphaned_branches: Vec::new(),
            orphaned_worktrees: Vec::new(),
            stale_sessions: Vec::new(),
            total_cleaned: 0,
        }
    }
    // ... add_branch, add_worktree, add_session
}
```

**Required change:**
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct CleanupSummary {
    pub orphaned_branches: Vec<String>,
    pub orphaned_worktrees: Vec<PathBuf>,
    pub stale_sessions: Vec<String>,
    pub skipped_worktrees: Vec<(PathBuf, String)>,  // (path, reason)
    pub total_cleaned: usize,
}

impl CleanupSummary {
    pub fn new() -> Self {
        Self {
            orphaned_branches: Vec::new(),
            orphaned_worktrees: Vec::new(),
            stale_sessions: Vec::new(),
            skipped_worktrees: Vec::new(),
            total_cleaned: 0,
        }
    }

    pub fn add_skipped_worktree(&mut self, path: PathBuf, reason: String) {
        self.skipped_worktrees.push((path, reason));
        // note: does NOT increment total_cleaned
    }
    // ... existing add_branch, add_worktree, add_session unchanged
}
```

**Why**: The cleanup result summary needs to communicate skipped worktrees back to the CLI for display. `total_cleaned` is intentionally not incremented for skipped items.

---

### Step 2: Add `find_processes_in_directory` to process module

**File**: `crates/kild-core/src/process/operations.rs`
**Action**: UPDATE (add new function)

**Add at end of file (before the `#[cfg(test)]` block):**
```rust
/// Find all running process PIDs with a current working directory inside `dir`.
///
/// Returns an empty Vec if no processes are found or CWD information is unavailable.
/// On macOS, CWD is only readable for processes owned by the current user.
pub fn find_processes_in_directory(dir: &Path) -> Vec<u32> {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, false);
    system
        .processes()
        .values()
        .filter_map(|p| {
            p.cwd()
                .filter(|cwd| cwd.starts_with(dir))
                .map(|_| p.pid().as_u32())
        })
        .collect()
}
```

**File**: `crates/kild-core/src/process/mod.rs`
**Lines**: 7-9
**Action**: UPDATE — add `find_processes_in_directory` to the re-export list:
```rust
pub use operations::{
    find_process_by_name, find_processes_in_directory, get_process_info, get_process_metrics,
    is_process_running, kill_process,
};
```

**Why**: The cleanup handler needs to detect if any user-owned process has its CWD inside the worktree being removed. Using `sysinfo` (already a dependency) avoids adding new crate deps. Only processes owned by the current user have readable CWD on macOS, which is precisely the right scope — kild agents are started as the current user.

---

### Step 3: Add `force` parameter and safety checks to cleanup handler

**File**: `crates/kild-core/src/cleanup/handler.rs`
**Action**: UPDATE

#### 3a. Update `cleanup_all_with_strategy` signature (line 194)

**Current:**
```rust
pub fn cleanup_all_with_strategy(
    strategy: CleanupStrategy,
) -> Result<CleanupSummary, CleanupError> {
```

**Required:**
```rust
pub fn cleanup_all_with_strategy(
    strategy: CleanupStrategy,
    force: bool,
) -> Result<CleanupSummary, CleanupError> {
```

Thread `force` to the `cleanup_orphaned_resources` call at line 211:
```rust
let cleanup_summary = cleanup_orphaned_resources(&scan_summary, force)?;
```

#### 3b. Update `cleanup_orphaned_resources` signature (line 92)

**Current:**
```rust
pub fn cleanup_orphaned_resources(
    summary: &CleanupSummary,
) -> Result<CleanupSummary, CleanupError> {
```

**Required:**
```rust
pub fn cleanup_orphaned_resources(
    summary: &CleanupSummary,
    force: bool,
) -> Result<CleanupSummary, CleanupError> {
```

Thread `force` to the `cleanup_orphaned_worktrees` call at line 122:
```rust
match cleanup_orphaned_worktrees(&summary.orphaned_worktrees, force) {
    Ok((cleaned_worktrees, skipped_worktrees)) => {
        for worktree_path in cleaned_worktrees {
            cleaned_summary.add_worktree(worktree_path);
        }
        for (path, reason) in skipped_worktrees {
            cleaned_summary.add_skipped_worktree(path, reason);
        }
    }
    Err(e) => { /* unchanged */ }
}
```

Also update the `cleanup_all` call at line 176 to pass `false`:
```rust
let cleanup_summary = cleanup_orphaned_resources(&scan_summary, false)?;
```

#### 3c. Rewrite `cleanup_orphaned_worktrees` (lines 428-467)

**Current:**
```rust
fn cleanup_orphaned_worktrees(
    worktree_paths: &[std::path::PathBuf],
) -> Result<Vec<std::path::PathBuf>, CleanupError> {
    if worktree_paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut cleaned_worktrees = Vec::new();
    for worktree_path in worktree_paths {
        info!(...);
        match git::removal::remove_worktree_by_path(worktree_path) {
            Ok(()) => { cleaned_worktrees.push(worktree_path.clone()); }
            Err(e) => { return Err(CleanupError::CleanupFailed { ... }); }
        }
    }
    Ok(cleaned_worktrees)
}
```

**Required:**
```rust
fn cleanup_orphaned_worktrees(
    worktree_paths: &[std::path::PathBuf],
    force: bool,
) -> Result<(Vec<std::path::PathBuf>, Vec<(std::path::PathBuf, String)>), CleanupError> {
    if worktree_paths.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut cleaned_worktrees = Vec::new();
    let mut skipped_worktrees = Vec::new();

    for worktree_path in worktree_paths {
        info!(
            event = "core.cleanup.worktree_delete_started",
            worktree_path = %worktree_path.display()
        );

        // Safety checks only apply when the directory exists.
        // If the directory is already gone, removal is safe (nothing to lose).
        if worktree_path.exists() {
            // Check 1: uncommitted changes via git status
            match git::get_worktree_status(worktree_path) {
                Ok(status) if status.has_uncommitted_changes => {
                    if force {
                        warn!(
                            event = "core.cleanup.worktree_unsafe_skip_overridden",
                            worktree_path = %worktree_path.display(),
                            reason = "uncommitted_changes",
                            "Removing worktree with uncommitted changes (--force)"
                        );
                    } else {
                        warn!(
                            event = "core.cleanup.worktree_delete_skipped",
                            worktree_path = %worktree_path.display(),
                            reason = "uncommitted_changes",
                            "Skipping orphaned worktree: has uncommitted changes"
                        );
                        skipped_worktrees.push((
                            worktree_path.clone(),
                            "has uncommitted changes".to_string(),
                        ));
                        continue;
                    }
                }
                Err(e) => {
                    // Conservative: if we can't check, skip unless forced
                    if force {
                        warn!(
                            event = "core.cleanup.worktree_status_check_failed",
                            worktree_path = %worktree_path.display(),
                            error = %e,
                            "Cannot verify git status, removing anyway (--force)"
                        );
                    } else {
                        warn!(
                            event = "core.cleanup.worktree_delete_skipped",
                            worktree_path = %worktree_path.display(),
                            reason = "status_check_failed",
                            error = %e,
                            "Skipping orphaned worktree: cannot verify git status"
                        );
                        skipped_worktrees.push((
                            worktree_path.clone(),
                            format!("cannot verify git status: {}", e),
                        ));
                        continue;
                    }
                }
                Ok(_) => {} // Clean worktree, proceed
            }

            // Check 2: active processes with CWD inside the worktree
            let active_pids = crate::process::find_processes_in_directory(worktree_path);
            if !active_pids.is_empty() {
                if force {
                    warn!(
                        event = "core.cleanup.worktree_unsafe_skip_overridden",
                        worktree_path = %worktree_path.display(),
                        reason = "active_processes",
                        pids = ?active_pids,
                        "Removing worktree with active processes (--force)"
                    );
                } else {
                    warn!(
                        event = "core.cleanup.worktree_delete_skipped",
                        worktree_path = %worktree_path.display(),
                        reason = "active_processes",
                        pids = ?active_pids,
                        "Skipping orphaned worktree: has active processes"
                    );
                    skipped_worktrees.push((
                        worktree_path.clone(),
                        format!("has active processes (PIDs: {:?})", active_pids),
                    ));
                    continue;
                }
            }
        }

        match git::removal::remove_worktree_by_path(worktree_path) {
            Ok(()) => {
                info!(
                    event = "core.cleanup.worktree_delete_completed",
                    worktree_path = %worktree_path.display()
                );
                cleaned_worktrees.push(worktree_path.clone());
            }
            Err(e) => {
                error!(
                    event = "core.cleanup.worktree_delete_failed",
                    worktree_path = %worktree_path.display(),
                    error = %e
                );
                return Err(CleanupError::CleanupFailed {
                    name: worktree_path.display().to_string(),
                    message: format!("Failed to remove worktree: {}", e),
                });
            }
        }
    }

    Ok((cleaned_worktrees, skipped_worktrees))
}
```

**Why**: The safety gate must be inside `cleanup_orphaned_worktrees` so it applies regardless of which detection strategy found the worktree. Uncommitted changes check uses the already-available `git::get_worktree_status` (only needs `&Path`, no session file required). Process check uses the new `find_processes_in_directory`. Conservative error handling matches the pattern in `get_destroy_safety_info`: when status is uncertain, refuse removal unless `--force`.

---

### Step 4: Update `cleanup_all_with_strategy` export in mod.rs

**File**: `crates/kild-core/src/cleanup/mod.rs`
**Lines**: 8-11
**Action**: No change needed — re-exports are not sensitive to signature changes for `force: bool`.

---

### Step 5: Add `--force` flag to cleanup CLI command

**File**: `crates/kild/src/app/misc.rs`
**Lines**: 32-37
**Action**: UPDATE — add `--force` arg after the `orphans` arg:

```rust
.arg(
    Arg::new("orphans")
        .long("orphans")
        .help("Clean worktrees in kild directory that have no session")
        .action(ArgAction::SetTrue),
)
.arg(
    Arg::new("force")
        .long("force")
        .short('f')
        .help("Remove orphaned worktrees even if they have uncommitted changes or active processes")
        .action(ArgAction::SetTrue),
)
```

---

### Step 6: Read `--force` and display skipped worktrees in CLI handler

**File**: `crates/kild/src/commands/cleanup.rs`
**Action**: UPDATE

**Current call site (line 26):**
```rust
match cleanup::cleanup_all_with_strategy(strategy) {
```

**Required:**
```rust
let force = sub_matches.get_flag("force");

match cleanup::cleanup_all_with_strategy(strategy, force) {
```

**Current output block (lines 30-57) — add skipped worktrees display after the existing blocks:**
```rust
if !summary.skipped_worktrees.is_empty() {
    eprintln!(
        "  Worktrees skipped (unsafe to remove): {}",
        summary.skipped_worktrees.len()
    );
    for (path, reason) in &summary.skipped_worktrees {
        eprintln!("    - {} ({})", shorten_home_path(path), reason);
    }
    eprintln!(
        "  Use --force to remove skipped worktrees (changes will be lost)."
    );
}
```

Place this block after the `stale_sessions` display, inside the `Ok(summary)` arm, regardless of `total_cleaned`. Skipped items should always be reported.

---

### Step 7: Add tests

**File**: `crates/kild-core/src/process/operations.rs` — inside `#[cfg(test)]`

```rust
#[test]
fn test_find_processes_in_directory_current_process() {
    // The current process has CWD somewhere; find it and verify the function works
    let cwd = std::env::current_dir().unwrap();
    let pids = find_processes_in_directory(&cwd);
    // Current process should appear (at minimum)
    assert!(!pids.is_empty(), "Expected at least the current process in CWD");
    let current_pid = std::process::id();
    assert!(
        pids.contains(&current_pid),
        "Expected current PID {} in results, got: {:?}",
        current_pid,
        pids
    );
}

#[test]
fn test_find_processes_in_directory_nonexistent() {
    let pids = find_processes_in_directory(std::path::Path::new("/nonexistent/path/xyz"));
    assert!(pids.is_empty());
}
```

**File**: `crates/kild-core/src/cleanup/handler.rs` — inside `#[cfg(test)]`

```rust
#[test]
fn test_cleanup_orphaned_worktrees_empty_list_with_force() {
    let result = cleanup_orphaned_worktrees(&[], true);
    assert!(result.is_ok());
    let (cleaned, skipped) = result.unwrap();
    assert_eq!(cleaned.len(), 0);
    assert_eq!(skipped.len(), 0);
}

#[test]
fn test_cleanup_orphaned_worktrees_nonexistent_path_is_removed() {
    // A path that doesn't exist skips safety checks (no uncommitted work to lose)
    // and goes directly to git removal (which gracefully handles missing dirs)
    let nonexistent = std::path::PathBuf::from("/tmp/kild-test-nonexistent-worktree-xyz");
    assert!(!nonexistent.exists());
    // This will call remove_worktree_by_path which may fail since it's not a real
    // git worktree, but critically it should NOT be in the skipped list
    let result = cleanup_orphaned_worktrees(&[nonexistent.clone()], false);
    // Whether Ok or Err, the key is it wasn't skipped due to safety checks
    match result {
        Ok((cleaned, skipped)) => {
            assert!(skipped.is_empty(), "Nonexistent path should not be in skipped list");
        }
        Err(CleanupError::CleanupFailed { .. }) => {
            // Expected: path is not a real git worktree, removal fails
        }
        Err(e) => panic!("Unexpected error: {}", e),
    }
}

#[test]
fn test_cleanup_summary_skipped_worktrees() {
    let mut summary = CleanupSummary::new();
    assert_eq!(summary.skipped_worktrees.len(), 0);

    let path = std::path::PathBuf::from("/tmp/test-worktree");
    summary.add_skipped_worktree(path.clone(), "has uncommitted changes".to_string());

    assert_eq!(summary.skipped_worktrees.len(), 1);
    assert_eq!(summary.skipped_worktrees[0].0, path);
    assert_eq!(summary.skipped_worktrees[0].1, "has uncommitted changes");
    // Skipped does NOT count toward total_cleaned
    assert_eq!(summary.total_cleaned, 0);
}
```

---

## Patterns to Follow

**From `sessions/destroy.rs:567-607` — conservative fallback when git status check fails:**
```rust
// If the call fails, use a conservative fallback
Err(e) => {
    warn!(event = "core.cleanup.worktree_status_check_failed", error = %e);
    // Conservative: refuse unless forced
}
```

**From `commands/destroy.rs:47-56` — blocking vs warning distinction:**
```rust
if safety_info.should_block() {
    eprintln!("{} {}", color::warning("Warning:"), warning);
} else {
    println!("{} {}", color::copper("Warning:"), warning);
}
```
For cleanup's skipped worktrees, use `eprintln!` (stderr) since these are warnings about skipped work, not the normal output.

**From `handler.rs:388-394` — graceful race condition handling (mirrors the branch cleanup pattern):**
```rust
if error_msg.contains("not found") || error_msg.contains("does not exist") {
    // Consider as cleaned
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                              | Mitigation                                                                                          |
| ------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| Worktree directory doesn't exist            | Skip safety checks entirely (no uncommitted work to lose); proceed directly to `remove_worktree_by_path` |
| `get_worktree_status` fails (git2 error)    | Conservative: skip worktree unless `--force`. Log warning with error details.                       |
| `find_processes_in_directory` returns wrong PIDs on macOS | Only user-owned process CWDs are readable via sysinfo; false negatives are safe (we might miss a root process), false positives are safe (we'd over-skip, not over-delete) |
| e2e tests calling `cleanup --orphans`       | Tests should pass `force: false` (default) — safety checks will prevent destroying live worktrees. Tests that explicitly need cleanup should use `--force` or delete the worktrees they own. |
| `cleanup_all` legacy path                   | Passes `force: false` to `cleanup_orphaned_resources` — safe default, applies safety checks there too |
| Existing test `test_cleanup_orphaned_worktrees_empty_list` | Must update to call `cleanup_orphaned_worktrees(&[], false)` and assert on tuple return |

---

## Validation

```bash
cargo fmt --check
cargo clippy --all -- -D warnings
cargo test --all
cargo build --all

# Specific test targets
cargo test -p kild-core test_find_processes_in_directory
cargo test -p kild-core test_cleanup_orphaned_worktrees
cargo test -p kild-core test_cleanup_summary_skipped_worktrees

# Manual verification
cargo run -p kild -- create test-kild --no-agent
echo "important work" > $(cargo run -p kild -- cd test-kild 2>/dev/null)/new-file.txt
rm ~/.kild/sessions/*_test-kild.json  # simulate orphan

# Should now skip with warning instead of destroying:
cargo run -p kild -- cleanup --orphans
# Expected output:
#   Cleanup complete.
#   Worktrees skipped (unsafe to remove): 1
#     - ~/.kild/worktrees/kild/test-kild (has uncommitted changes)
#   Use --force to remove skipped worktrees (changes will be lost).

# Force removal should work:
cargo run -p kild -- cleanup --orphans --force
# Expected: worktree removed with warning logged
```

---

## Scope Boundaries

**IN SCOPE:**
- Safety checks (git status + active process) in `cleanup_orphaned_worktrees`
- `--force` flag for `kild cleanup`
- `skipped_worktrees` field on `CleanupSummary`
- `find_processes_in_directory` in process module
- Tests for new behavior

**OUT OF SCOPE (do not touch):**
- `kild cleanup --all`, `--stopped`, `--no-pid`, `--older-than` strategies (session file cleanup, no worktree deletion risk)
- PR check from `get_destroy_safety_info` (adds forge dependency, not justified for orphan cleanup)
- Confirmation prompt (issue asks for `--force` override, not interactive confirmation)
- Changing `detect_untracked_worktrees` detection logic
- e2e test changes (fixing tests is a separate concern from the safety feature)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-18T00:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-496.md`
