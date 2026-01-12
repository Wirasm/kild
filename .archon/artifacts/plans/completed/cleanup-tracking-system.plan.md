# Feature: Cleanup Tracking System

## Summary

Implement automated tracking and cleanup of orphaned Git branches and worktrees to prevent "branch already exists" errors and maintain clean repository state. The system detects orphaned resources, provides manual cleanup commands, and enhances destroy operations to prevent orphaning.

## User Story

As a Shards CLI user
I want automatic detection and cleanup of orphaned branches and merged worktrees
So that I don't get "branch already exists" errors and my Git repository stays clean

## Problem Statement

When destroying shards, Git branches (`worktree-*`) are left behind causing creation conflicts. Worktrees can become corrupted with detached HEAD states. Session files persist after worktree destruction. No automatic detection or recovery mechanism exists for inconsistent states.

## Solution Statement

Create a new `cleanup` feature slice that tracks Git state, detects orphaned resources, enhances destroy operations to clean up completely, and provides manual recovery commands. Uses existing git2 crate patterns for worktree and branch management.

## Metadata

| Field            | Value                                             |
| ---------------- | ------------------------------------------------- |
| Type             | NEW_CAPABILITY                                    |
| Complexity       | HIGH                                              |
| Systems Affected | git, sessions, cli                                |
| Dependencies     | git2 = "0.18", existing core infrastructure      |
| Estimated Tasks  | 8                                                 |

---

## UX Design

### Before State
```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   shards    │───▶│   destroy   │───▶│ Orphaned    │
│   destroy   │    │  worktree   │    │  branches   │
└─────────────┘    └─────────────┘    └─────────────┘
                           │                   │
                           ▼                   ▼
                  ┌─────────────┐    ┌─────────────┐
                  │ Manual Git  │    │ Creation    │
                  │ Commands    │    │ Conflicts   │
                  └─────────────┘    └─────────────┘

USER_FLOW: Create shard → Work → Destroy → Manual cleanup required
PAIN_POINT: 32 orphaned branches, "branch already exists" errors
DATA_FLOW: Session deleted → Worktree removed → Branch remains orphaned
```

### After State
```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   shards    │───▶│   destroy   │───▶│   Clean     │
│   destroy   │    │  complete   │    │    State    │
└─────────────┘    └─────────────┘    └─────────────┘
                           │                   ▲
                           ▼                   │
                  ┌─────────────┐    ┌─────────────┐
                  │ Auto Branch │    │ Merge       │
                  │ Cleanup     │    │ Detection   │
                  └─────────────┘    └─────────────┘
                           │                   │
                           ▼                   ▼
                  ┌─────────────┐    ┌─────────────┐
                  │ shards      │◀───│ Tracking    │
                  │ cleanup     │    │ System      │
                  └─────────────┘    └─────────────┘

USER_FLOW: Create shard → Work → Auto-cleanup → Clean state maintained
VALUE_ADD: Zero manual intervention, automatic merge detection
DATA_FLOW: Session → Worktree → Branch → Tracking → Auto-cleanup
```

### Interaction Changes
| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| `shards destroy` | Removes worktree only | Removes worktree + branch + session | No orphaned branches |
| `shards list` | Shows stale sessions | Shows only valid sessions | Accurate state view |
| `shards cleanup` | Command doesn't exist | Fixes all orphaned resources | Recovery mechanism |
| Branch creation | "Branch already exists" errors | Clean creation always works | No conflicts |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `src/git/handler.rs` | 200-300 | Pattern to MIRROR for Git operations |
| P0 | `src/git/errors.rs` | 1-50 | Error patterns to FOLLOW exactly |
| P1 | `src/sessions/handler.rs` | 80-120 | Handler pattern to COPY |
| P1 | `src/cli/commands.rs` | 1-100 | CLI command pattern to MIRROR |
| P2 | `src/sessions/operations.rs` | 1-50 | Operations pattern to FOLLOW |

**External Documentation:**
| Source | Section | Why Needed |
|--------|---------|------------|
| [git2-rs v0.18](https://docs.rs/git2/latest/git2/struct.Worktree.html) | Worktree methods | prune() and path() usage |
| [git2-rs Branch](https://docs.rs/git2/latest/git2/struct.Branch.html) | Branch operations | upstream() and deletion methods |

---

## Patterns to Mirror

**NAMING_CONVENTION:**
```rust
// SOURCE: src/git/handler.rs:10-15
// COPY THIS PATTERN:
pub fn detect_project() -> Result<ProjectInfo, GitError> {
    info!(event = "git.project.detect_started");
    // ... implementation
}
```

**ERROR_HANDLING:**
```rust
// SOURCE: src/git/errors.rs:5-20
// COPY THIS PATTERN:
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Not in a git repository")]
    NotInRepository,
    // ... other variants
}
```

**LOGGING_PATTERN:**
```rust
// SOURCE: src/git/handler.rs:25-30
// COPY THIS PATTERN:
info!(
    event = "git.worktree.create_started",
    project_id = project.id,
    branch = validated_branch
);
```

**HANDLER_PATTERN:**
```rust
// SOURCE: src/sessions/handler.rs:10-40
// COPY THIS PATTERN:
pub fn create_session(request: CreateSessionRequest) -> Result<Session, SessionError> {
    info!(event = "session.create_started", branch = request.branch);
    // 1. Validate input (pure)
    // 2. I/O operations
    // 3. Log completion
}
```

**CLI_COMMAND_PATTERN:**
```rust
// SOURCE: src/cli/commands.rs:50-80
// COPY THIS PATTERN:
fn handle_destroy_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").unwrap();
    info!(event = "cli.destroy_started", branch = branch);
    // ... implementation
}
```

**TEST_STRUCTURE:**
```rust
// SOURCE: src/git/handler.rs:300-320
// COPY THIS PATTERN:
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_name() {
        // Test implementation
    }
}
```

---

## Files to Change

| File                             | Action | Justification                            |
| -------------------------------- | ------ | ---------------------------------------- |
| `src/cleanup/mod.rs`             | CREATE | Feature slice module exports            |
| `src/cleanup/types.rs`           | CREATE | Cleanup-specific data structures         |
| `src/cleanup/errors.rs`          | CREATE | Cleanup-specific error types             |
| `src/cleanup/operations.rs`      | CREATE | Pure cleanup logic (no I/O)             |
| `src/cleanup/handler.rs`         | CREATE | I/O orchestration for cleanup            |
| `src/lib.rs`                     | UPDATE | Add cleanup module export                |
| `src/cli/app.rs`                 | UPDATE | Add cleanup command definition           |
| `src/cli/commands.rs`            | UPDATE | Add cleanup command handler              |
| `src/git/handler.rs`             | UPDATE | Enhance destroy to delete branches       |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- Real-time monitoring daemon: Only scan-based detection to avoid complexity
- Cross-repository cleanup: Only works within current project scope  
- GUI interface: CLI-only to match existing architecture
- Automatic merge detection: Phase 2 feature, focus on orphan cleanup first
- Background processes: Keep it simple with on-demand scanning

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: CREATE `src/cleanup/types.rs`

- **ACTION**: CREATE cleanup data structures
- **IMPLEMENT**: OrphanedResource, CleanupSummary, ResourceType enums
- **MIRROR**: `src/git/types.rs:1-30` - follow existing struct pattern
- **IMPORTS**: `use std::path::PathBuf; use serde::{Deserialize, Serialize};`
- **TYPES**: `pub struct OrphanedResource { pub resource_type: ResourceType, pub path: PathBuf }`
- **VALIDATE**: `cargo check --lib`

### Task 2: CREATE `src/cleanup/errors.rs`

- **ACTION**: CREATE cleanup-specific error types
- **IMPLEMENT**: CleanupError enum with thiserror
- **MIRROR**: `src/git/errors.rs:1-50` - follow exact error pattern
- **IMPORTS**: `use crate::core::errors::ShardsError; use thiserror::Error;`
- **PATTERN**: Include error_code() and is_user_error() methods
- **VALIDATE**: `cargo check --lib`

### Task 3: CREATE `src/cleanup/operations.rs`

- **ACTION**: CREATE pure cleanup logic functions
- **IMPLEMENT**: detect_orphaned_branches, detect_orphaned_worktrees, validate_cleanup_request
- **MIRROR**: `src/sessions/operations.rs:1-50` - pure functions only
- **PATTERN**: No I/O operations, return Results, comprehensive tests
- **GOTCHA**: Use git2::Repository for branch enumeration, not shell commands
- **VALIDATE**: `cargo test cleanup::operations`

### Task 4: CREATE `src/cleanup/handler.rs`

- **ACTION**: CREATE I/O orchestration for cleanup operations
- **IMPLEMENT**: scan_for_orphans, cleanup_orphaned_resources, cleanup_all
- **MIRROR**: `src/git/handler.rs:200-300` - I/O orchestration pattern
- **IMPORTS**: `use tracing::{info, warn, error}; use crate::git; use crate::sessions;`
- **PATTERN**: Structured logging, error handling, delegate to operations
- **VALIDATE**: `cargo check --lib`

### Task 5: CREATE `src/cleanup/mod.rs`

- **ACTION**: CREATE feature slice public API
- **IMPLEMENT**: Export types, errors, handler functions
- **MIRROR**: `src/sessions/mod.rs:1-10` - module export pattern
- **PATTERN**: Public exports only, hide operations (internal)
- **VALIDATE**: `cargo check --lib`

### Task 6: UPDATE `src/lib.rs`

- **ACTION**: ADD cleanup module to library exports
- **IMPLEMENT**: `pub mod cleanup;` in appropriate location
- **MIRROR**: Existing module declarations in lib.rs
- **VALIDATE**: `cargo check --lib`

### Task 7: UPDATE `src/cli/app.rs`

- **ACTION**: ADD cleanup command to CLI definition
- **IMPLEMENT**: New "cleanup" subcommand with clap
- **MIRROR**: `src/cli/app.rs:30-50` - existing subcommand pattern
- **PATTERN**: Use Command::new("cleanup").about("Clean up orphaned resources")
- **VALIDATE**: `cargo check --bin shards`

### Task 8: UPDATE `src/cli/commands.rs`

- **ACTION**: ADD cleanup command handler
- **IMPLEMENT**: handle_cleanup_command function and route in run_command
- **MIRROR**: `src/cli/commands.rs:80-120` - existing command handler pattern
- **PATTERN**: Structured logging, error handling, user-friendly output
- **VALIDATE**: `cargo build && ./target/debug/shards cleanup --help`

### Task 9: UPDATE `src/git/handler.rs`

- **ACTION**: ENHANCE destroy operations to delete branches
- **IMPLEMENT**: Add branch deletion to remove_worktree_by_path
- **MIRROR**: Existing git2 patterns in same file
- **GOTCHA**: Must find and delete `worktree-{branch}` branch after worktree removal
- **VALIDATE**: `cargo test git::handler`

---

## Testing Strategy

### Unit Tests to Write

| Test File                                | Test Cases                 | Validates      |
| ---------------------------------------- | -------------------------- | -------------- |
| `src/cleanup/tests/operations.test.rs`  | orphan detection logic     | Pure functions |
| `src/cleanup/tests/handler.test.rs`     | I/O orchestration          | Handler logic  |
| `src/cleanup/tests/errors.test.rs`      | error properties           | Error classes  |

### Edge Cases Checklist

- [ ] Empty repository (no branches/worktrees)
- [ ] Corrupted worktree states (detached HEAD)
- [ ] Permission errors during cleanup
- [ ] Concurrent Git operations
- [ ] Non-existent session files
- [ ] Mixed orphaned/valid resources

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo check --lib && cargo clippy -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: UNIT_TESTS

```bash
cargo test cleanup::
```

**EXPECT**: All tests pass, coverage >= 80%

### Level 3: INTEGRATION_TESTS

```bash
cargo test && cargo build
```

**EXPECT**: All tests pass, build succeeds

### Level 4: CLI_VALIDATION

```bash
./target/debug/shards cleanup --help
./target/debug/shards --help | grep cleanup
```

**EXPECT**: Help text displays correctly, command listed

### Level 5: MANUAL_VALIDATION

1. Create test shard: `shards create test-cleanup`
2. Manually create orphaned branch: `git branch worktree-orphaned`
3. Run cleanup: `shards cleanup`
4. Verify branch removed: `git branch | grep worktree-orphaned` (should be empty)

---

## Acceptance Criteria

- [ ] `shards cleanup` command detects and removes orphaned branches
- [ ] `shards destroy` removes both worktree and associated branch
- [ ] Orphaned worktree directories are cleaned up
- [ ] Stale session files are removed
- [ ] All validation commands pass with exit 0
- [ ] No regressions in existing functionality
- [ ] Structured logging follows existing patterns

---

## Completion Checklist

- [ ] All tasks completed in dependency order
- [ ] Each task validated immediately after completion
- [ ] Level 1: `cargo check --lib && cargo clippy` passes
- [ ] Level 2: `cargo test cleanup::` passes
- [ ] Level 3: `cargo test && cargo build` succeeds
- [ ] Level 4: CLI help displays cleanup command
- [ ] Level 5: Manual validation passes
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk               | Likelihood   | Impact       | Mitigation                              |
| ------------------ | ------------ | ------------ | --------------------------------------- |
| Git state corruption | LOW | HIGH | Use git2 library safely, validate before operations |
| Concurrent operations | MEDIUM | MEDIUM | Add file locking, atomic operations where possible |
| Performance impact | LOW | LOW | Only scan when requested, cache results |
| False positive detection | MEDIUM | LOW | Conservative detection logic, dry-run mode |

---

## Notes

This implementation focuses on the core cleanup functionality first. Future enhancements could include:
- Automatic merge detection (check if branch is merged into main)
- Scheduled cleanup (cron-like functionality)
- More sophisticated orphan detection (age-based, activity-based)
- Integration with Git hooks for real-time cleanup

The design maintains the existing vertical slice architecture and follows all established patterns for consistency and maintainability.
