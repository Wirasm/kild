# Feature: Cleanup --orphans Flag

## Summary

Add `--orphans` flag to the cleanup command that detects and removes worktrees in the shards directory (`~/.shards/worktrees/<project>/`) that have no corresponding session file. This follows the principle "Git is the source of truth" - if a worktree exists in git but shards doesn't know about it, it's orphaned.

## User Story

As a shards user
I want to clean up worktrees that exist but aren't tracked by shards
So that I can reclaim disk space and keep my worktree directory clean

## Problem Statement

The current cleanup command only detects:
1. Corrupted worktrees (missing directory, bad HEAD)
2. Stale sessions (session file points to non-existent worktree)
3. Orphaned branches (`worktree-*` prefix, not checked out)

It does NOT detect worktrees that:
- Exist in git and filesystem (valid state)
- Are in the shards worktree directory
- Have no corresponding session file

This gap means abandoned worktrees accumulate and must be manually cleaned.

## Solution Statement

Add a new `--orphans` flag that:
1. Scans `~/.shards/worktrees/<current-project>/` for directories
2. For each directory, checks if there's a session file with matching `worktree_path`
3. If no session exists, marks the worktree as orphaned
4. Removes orphaned worktrees and their associated branches

Design principle: **Git is the source of truth** - we query git for worktrees, then cross-reference with sessions.

## Metadata

| Field            | Value                                |
| ---------------- | ------------------------------------ |
| Type             | ENHANCEMENT                          |
| Complexity       | LOW                                  |
| Systems Affected | cleanup, sessions, cli               |
| Dependencies     | git2, serde_json                     |
| Estimated Tasks  | 5                                    |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   $ shards cleanup                                                            ║
║   ✅ No orphaned resources found - repository is clean!                        ║
║                                                                               ║
║   But ~/.shards/worktrees/shards/ contains:                                   ║
║   - issue-26-last-activity-tracking/  (no session)                            ║
║   - issue-27-wire-cleanup-strategies/ (no session)                            ║
║   - ... 10 more abandoned worktrees                                           ║
║                                                                               ║
║   USER_FLOW: Run cleanup → reports clean → worktrees still exist              ║
║   PAIN_POINT: Cleanup doesn't detect valid-but-untracked worktrees            ║
║   DATA_FLOW: Scans sessions → checks git state → misses session-less worktrees║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   $ shards cleanup --orphans                                                  ║
║   ✅ Cleanup completed successfully!                                           ║
║      Resources cleaned:                                                       ║
║      📁 Worktrees removed: 12                                                  ║
║         - /Users/x/.shards/worktrees/shards/issue-26-last-activity-tracking   ║
║         - /Users/x/.shards/worktrees/shards/issue-27-wire-cleanup-strategies  ║
║         ...                                                                   ║
║      📦 Branches removed: 12                                                   ║
║         - worktree-issue-26-last-activity-tracking                            ║
║         ...                                                                   ║
║      Total: 24 resources cleaned                                              ║
║                                                                               ║
║   USER_FLOW: Run cleanup --orphans → finds untracked → removes them           ║
║   VALUE_ADD: Can now clean up abandoned worktrees automatically               ║
║   DATA_FLOW: Git worktrees → filter shards dir → cross-ref sessions → clean   ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location              | Before                              | After                                         | User Impact                        |
| --------------------- | ----------------------------------- | --------------------------------------------- | ---------------------------------- |
| `shards cleanup`      | Only detects corrupted worktrees    | Same (default behavior unchanged)             | No change to existing workflow     |
| `shards cleanup --orphans` | N/A                            | Detects worktrees without sessions            | Can clean abandoned worktrees      |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File                           | Lines   | Why Read This                                    |
| -------- | ------------------------------ | ------- | ------------------------------------------------ |
| P0       | `src/cleanup/operations.rs`    | 77-119  | Pattern for `detect_orphaned_worktrees()`        |
| P0       | `src/cleanup/handler.rs`       | 226-296 | Pattern for `scan_for_orphans_with_strategy()`   |
| P0       | `src/cleanup/types.rs`         | 11-17   | `CleanupStrategy` enum to extend                 |
| P1       | `src/cli/app.rs`               | 87-114  | CLI arg definition pattern for cleanup command   |
| P1       | `src/cli/commands.rs`          | 252-324 | `handle_cleanup_command()` strategy handling     |
| P2       | `src/git/operations.rs`        | 5-15    | `calculate_worktree_path()`, `derive_project_name_from_path()` |
| P2       | `src/core/config.rs`           | 313-319 | `worktrees_dir()`, `sessions_dir()` paths        |
| P2       | `src/sessions/types.rs`        | 12-77   | Session struct with `worktree_path` field        |

**External Documentation:**
| Source | Section | Why Needed |
|--------|---------|------------|
| None required | N/A | All patterns exist in codebase |

---

## Patterns to Mirror

**NAMING_CONVENTION:**
```rust
// SOURCE: src/cleanup/operations.rs:77
// COPY THIS PATTERN:
pub fn detect_orphaned_worktrees(repo: &Repository) -> Result<Vec<PathBuf>, CleanupError>
```

**ERROR_HANDLING:**
```rust
// SOURCE: src/cleanup/operations.rs:82-84
// COPY THIS PATTERN:
let worktrees = repo
    .worktrees()
    .map_err(|e| CleanupError::WorktreeScanFailed {
        message: format!("Failed to list worktrees: {}", e),
    })?;
```

**LOGGING_PATTERN:**
```rust
// SOURCE: src/cleanup/handler.rs:10
// COPY THIS PATTERN:
info!(event = "cleanup.scan_started");

// SOURCE: src/cleanup/operations.rs:159-164
// For warnings:
warn!(
    event = "cleanup.malformed_session_file",
    file_path = %path.display(),
    error = %e,
    "Found malformed session file during cleanup scan"
);
```

**STRATEGY_PATTERN:**
```rust
// SOURCE: src/cleanup/types.rs:11-17
// COPY THIS PATTERN:
#[derive(Debug, Clone, PartialEq)]
pub enum CleanupStrategy {
    All,            // Clean everything (default)
    NoPid,          // Only sessions with process_id: None
    Stopped,        // Only sessions with stopped processes
    OlderThan(u64), // Only sessions older than N days
}
```

**CLI_ARG_PATTERN:**
```rust
// SOURCE: src/cli/app.rs:95-100
// COPY THIS PATTERN:
.arg(
    Arg::new("stopped")
        .long("stopped")
        .help("Clean only sessions with stopped processes")
        .action(ArgAction::SetTrue)
)
```

**SESSION_READING_PATTERN:**
```rust
// SOURCE: src/cleanup/operations.rs:137-155
// COPY THIS PATTERN for reading session files:
match std::fs::read_to_string(&path) {
    Ok(content) => {
        match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(session) => {
                if let Some(worktree_path) =
                    session.get("worktree_path").and_then(|v| v.as_str())
                {
                    // Use worktree_path...
                }
            }
            Err(e) => { /* handle error */ }
        }
    }
    Err(e) => { /* handle error */ }
}
```

---

## Files to Change

| File                           | Action | Justification                                    |
| ------------------------------ | ------ | ------------------------------------------------ |
| `src/cleanup/types.rs`         | UPDATE | Add `Orphans` variant to `CleanupStrategy`       |
| `src/cleanup/operations.rs`    | UPDATE | Add `detect_untracked_worktrees()` function      |
| `src/cleanup/handler.rs`       | UPDATE | Handle `Orphans` strategy in scan/cleanup        |
| `src/cli/app.rs`               | UPDATE | Add `--orphans` CLI argument                     |
| `src/cli/commands.rs`          | UPDATE | Handle `--orphans` flag in command handler       |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Interactive confirmation mode** - Not adding `--interactive` in this PR
- **Dry run mode** - Not adding `--dry-run` in this PR
- **Cross-project cleanup** - Only cleans current project's worktrees
- **Worktrees outside shards directory** - Not touching worktrees created elsewhere
- **Automatic orphan cleanup** - `--orphans` is opt-in, not default behavior

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `src/cleanup/types.rs` - Add Orphans strategy variant

- **ACTION**: ADD `Orphans` variant to `CleanupStrategy` enum
- **IMPLEMENT**:
  ```rust
  #[derive(Debug, Clone, PartialEq)]
  pub enum CleanupStrategy {
      All,            // Clean everything (default)
      NoPid,          // Only sessions with process_id: None
      Stopped,        // Only sessions with stopped processes
      OlderThan(u64), // Only sessions older than N days
      Orphans,        // Worktrees in shards dir without sessions
  }
  ```
- **MIRROR**: `src/cleanup/types.rs:11-17`
- **GOTCHA**: Keep existing variants unchanged, just add new one at the end
- **VALIDATE**: `cargo check`

### Task 2: UPDATE `src/cleanup/operations.rs` - Add detect_untracked_worktrees()

- **ACTION**: ADD new detection function for worktrees without sessions
- **IMPLEMENT**:
  ```rust
  /// Detect worktrees in the shards directory that have no corresponding session.
  ///
  /// This finds worktrees that:
  /// 1. Are registered in git
  /// 2. Have paths under `~/.shards/worktrees/<project>/`
  /// 3. Have no session file with matching `worktree_path`
  ///
  /// # Arguments
  /// * `repo` - The git repository
  /// * `worktrees_dir` - Base worktrees directory (~/.shards/worktrees)
  /// * `sessions_dir` - Sessions directory (~/.shards/sessions)
  /// * `project_name` - Current project name for scoping
  pub fn detect_untracked_worktrees(
      repo: &Repository,
      worktrees_dir: &Path,
      sessions_dir: &Path,
      project_name: &str,
  ) -> Result<Vec<PathBuf>, CleanupError>
  ```
- **LOGIC**:
  1. Get all worktrees from git via `repo.worktrees()`
  2. Filter to only those under `worktrees_dir/<project_name>/`
  3. Load all session files from `sessions_dir`
  4. Collect worktree_paths from sessions into a HashSet
  5. Return worktrees not in the session paths set
- **MIRROR**: `src/cleanup/operations.rs:77-119` for git worktree iteration
- **MIRROR**: `src/cleanup/operations.rs:121-188` for session file reading
- **IMPORTS**: Add `use std::collections::HashSet;` if not present
- **GOTCHA**: Worktree paths must be compared as canonical paths (use `.canonicalize()`)
- **VALIDATE**: `cargo check`

### Task 3: UPDATE `src/cleanup/handler.rs` - Handle Orphans strategy

- **ACTION**: UPDATE `scan_for_orphans_with_strategy()` to handle `Orphans` variant
- **IMPLEMENT**: Add match arm in `scan_for_orphans_with_strategy()`:
  ```rust
  CleanupStrategy::Orphans => {
      // Get project info for scoping
      let project = crate::git::handler::detect_project()?;

      // Detect untracked worktrees
      let untracked = operations::detect_untracked_worktrees(
          &repo,
          &config.worktrees_dir(),
          &config.sessions_dir(),
          &project.name,
      )?;

      for worktree_path in untracked {
          summary.add_worktree(worktree_path);
      }

      // Also detect orphaned branches (worktree-* not checked out)
      let orphaned_branches = operations::detect_orphaned_branches(&repo)?;
      for branch in orphaned_branches {
          summary.add_branch(branch);
      }
  }
  ```
- **MIRROR**: `src/cleanup/handler.rs:240-296` for other strategy handling
- **IMPORTS**: May need `use crate::git;`
- **GOTCHA**: Must also clean up associated branches, not just worktrees
- **VALIDATE**: `cargo check`

### Task 4: UPDATE `src/cli/app.rs` - Add --orphans CLI argument

- **ACTION**: ADD `--orphans` argument to cleanup subcommand
- **IMPLEMENT**: Add after the `--all` argument (around line 113):
  ```rust
  .arg(
      Arg::new("orphans")
          .long("orphans")
          .help("Clean worktrees in shards directory that have no session")
          .action(ArgAction::SetTrue)
  )
  ```
- **MIRROR**: `src/cli/app.rs:95-100` for argument pattern
- **GOTCHA**: Place after `--all` to maintain logical grouping
- **VALIDATE**: `cargo check`

### Task 5: UPDATE `src/cli/commands.rs` - Handle --orphans flag

- **ACTION**: UPDATE `handle_cleanup_command()` to check for `--orphans` flag
- **IMPLEMENT**: Add before the `else` clause that defaults to `All`:
  ```rust
  let strategy = if sub_matches.get_flag("no-pid") {
      cleanup::CleanupStrategy::NoPid
  } else if sub_matches.get_flag("stopped") {
      cleanup::CleanupStrategy::Stopped
  } else if let Some(days) = sub_matches.get_one::<u64>("older-than") {
      cleanup::CleanupStrategy::OlderThan(*days)
  } else if sub_matches.get_flag("orphans") {
      cleanup::CleanupStrategy::Orphans
  } else {
      cleanup::CleanupStrategy::All
  };
  ```
- **MIRROR**: `src/cli/commands.rs:255-263`
- **GOTCHA**: Order matters - check `orphans` before falling through to `All`
- **VALIDATE**: `cargo check && cargo test`

---

## Testing Strategy

### Unit Tests to Write

| Test File                                   | Test Cases                                          | Validates                    |
| ------------------------------------------- | --------------------------------------------------- | ---------------------------- |
| `src/cleanup/operations.rs` (inline tests)  | Empty worktrees dir, no sessions, mixed state       | `detect_untracked_worktrees` |

### Manual Testing

1. Create worktrees manually: `git worktree add ~/.shards/worktrees/shards/test-orphan -b worktree-test-orphan`
2. Run `shards cleanup` - should NOT detect the orphan
3. Run `shards cleanup --orphans` - should detect and clean the orphan
4. Verify worktree and branch are removed

### Edge Cases Checklist

- [ ] Empty worktrees directory
- [ ] No session files exist
- [ ] Worktree exists with matching session (should NOT be cleaned)
- [ ] Worktree outside shards directory (should NOT be touched)
- [ ] Multiple projects - only current project cleaned
- [ ] Symlinked worktree paths

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo clippy -- -D warnings && cargo fmt --check
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test cleanup
```

**EXPECT**: All cleanup-related tests pass

### Level 3: FULL_SUITE

```bash
cargo test && cargo build --release
```

**EXPECT**: All tests pass, build succeeds

### Level 6: MANUAL_VALIDATION

1. `git worktree add ~/.shards/worktrees/shards/manual-test -b worktree-manual-test`
2. `cargo run -- cleanup` → Should report "clean"
3. `cargo run -- cleanup --orphans` → Should find and clean the orphan
4. `git worktree list` → Should only show main
5. `git branch | grep worktree-manual-test` → Should be empty

---

## Acceptance Criteria

- [ ] `shards cleanup` default behavior unchanged
- [ ] `shards cleanup --orphans` detects worktrees without sessions
- [ ] Only worktrees under `~/.shards/worktrees/<current-project>/` are cleaned
- [ ] Associated `worktree-*` branches are also removed
- [ ] All validation commands pass

---

## Completion Checklist

- [ ] All 5 tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] Level 1: Static analysis passes
- [ ] Level 2: Unit tests pass
- [ ] Level 3: Full test suite + build succeeds
- [ ] Level 6: Manual validation passes
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk                                    | Likelihood | Impact | Mitigation                                        |
| --------------------------------------- | ---------- | ------ | ------------------------------------------------- |
| Delete user's active worktree           | LOW        | HIGH   | Only delete if NO session exists; require --orphans flag |
| Cross-project deletion                  | LOW        | MED    | Scope by project name derived from current repo   |
| Path comparison fails on symlinks       | MED        | LOW    | Use `.canonicalize()` for path comparison         |

---

## Notes

**Design Decisions:**

1. **Git as source of truth**: We iterate git worktrees first, then filter. This is simpler than scanning filesystem directories.

2. **Project scoping**: Only clean worktrees for the current project. This prevents accidental cleanup of other projects' worktrees.

3. **Opt-in behavior**: `--orphans` is explicit. Default `shards cleanup` behavior is unchanged. This is safer for users who may have intentional session-less worktrees.

4. **Branch cleanup included**: When removing orphan worktrees, we also clean up associated `worktree-*` branches via the existing `detect_orphaned_branches()` function.
