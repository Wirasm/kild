# Implementation Plan: CLI Phase 1.2 - Print Worktree Path (`shards cd`)

**Source PRD**: `.claude/PRPs/prds/cli-core-features.prd.md`
**Phase**: 1.2
**Status**: READY FOR IMPLEMENTATION

---

## Summary

Add a new `shards cd <branch>` command that prints the worktree path for a given shard branch name. This enables shell integration where users can define shell functions like `scd() { cd "$(shards cd "$1")" }` to quickly navigate to shard worktrees.

## User Story

As a power user, I want to quickly navigate to a shard's worktree directory so that I can work directly in that context without manually finding the path.

## Problem Statement

A subprocess cannot change the parent shell's directory. Users need a way to get the worktree path programmatically for shell integration.

## Solution Statement

Add a minimal `shards cd <branch>` command that:
1. Looks up the session by branch name
2. Prints ONLY the worktree path to stdout (no formatting, no prefix)
3. Exits with error if session not found

This enables shell integration: `cd "$(shards cd branch)"` or `scd() { cd "$(shards cd "$1")" }`

## Metadata

| Field | Value |
|-------|-------|
| Type | NEW FEATURE |
| Complexity | LOW |
| Systems Affected | shards (CLI only) |
| Dependencies | None |
| Estimated Tasks | 3 |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/shards/src/app.rs` | 48-65 | Command definition pattern (destroy command) |
| P0 | `crates/shards/src/commands.rs` | 296-365 | Handler pattern (status command uses get_session) |
| P1 | `crates/shards-core/src/sessions/handler.rs` | 207-220 | get_session() function for session lookup |
| P1 | `crates/shards-core/src/sessions/types.rs` | 26 | Session struct with worktree_path field |

---

## Patterns to Mirror

**CLI_COMMAND_DEFINITION_PATTERN:**
```rust
// SOURCE: crates/shards/src/app.rs:48-65 (destroy command)
.subcommand(
    Command::new("destroy")
        .about("Destroy a shard...")
        .arg(
            Arg::new("branch")
                .help("Branch name of the shard to destroy")
                .required(true)
                .index(1)
        )
)
```

**COMMAND_HANDLER_PATTERN:**
```rust
// SOURCE: crates/shards/src/commands.rs:296-310
fn handle_status_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    info!(event = "cli.status_started", branch = branch);

    match session_handler::get_session(branch) {
        Ok(session) => {
            // ... use session
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            error!(event = "cli.status_failed", ...);
            Err(e.into())
        }
    }
}
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/shards/src/app.rs` | UPDATE | Add `cd` subcommand definition |
| `crates/shards/src/commands.rs` | UPDATE | Add `handle_cd_command()` and wire into `run_command()` |

**Note**: No changes to shards-core needed - uses existing `get_session()` function.

---

## NOT Building (Scope Limits)

- **No fuzzy matching** - That's Phase 2.6
- **No shell alias setup** - Users can add their own shell functions
- **No directory validation** - Trust the stored path
- **No cd integration** - Subprocess can't change parent shell directory

---

## Step-by-Step Tasks

### Task 1: ADD `cd` subcommand to CLI definition

- **ACTION**: Add new subcommand after the `list` command
- **FILE**: `crates/shards/src/app.rs`
- **LOCATION**: After line 48 (after the `list` subcommand)
- **IMPLEMENT**:
```rust
.subcommand(
    Command::new("cd")
        .about("Print worktree path for shell integration")
        .arg(
            Arg::new("branch")
                .help("Branch name of the shard")
                .required(true)
                .index(1)
        )
)
```
- **VALIDATE**: `cargo check -p shards`

### Task 2: ADD command handler and wire into router

- **ACTION**: Add `handle_cd_command()` function and match arm in `run_command()`
- **FILE**: `crates/shards/src/commands.rs`
- **LOCATION**:
  1. Add match arm after line 57 (after list command): `Some(("cd", sub_matches)) => handle_cd_command(sub_matches),`
  2. Add handler function after `handle_list_command()` (around line 167)
- **IMPLEMENT**:
```rust
fn handle_cd_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    info!(event = "cli.cd_started", branch = branch);

    match session_handler::get_session(branch) {
        Ok(session) => {
            // Print only the path - no formatting, no newline prefix
            // This is critical for shell integration: cd "$(shards cd branch)"
            println!("{}", session.worktree_path.display());

            info!(
                event = "cli.cd_completed",
                branch = branch,
                path = %session.worktree_path.display()
            );

            Ok(())
        }
        Err(e) => {
            eprintln!("Error: {}", e);

            error!(
                event = "cli.cd_failed",
                branch = branch,
                error = %e
            );

            Err(e.into())
        }
    }
}
```
- **MIRROR**: `handle_status_command()` at lines 296-365
- **VALIDATE**: `cargo check -p shards`

### Task 3: ADD CLI tests for cd command

- **ACTION**: Add tests to verify CLI argument parsing
- **FILE**: `crates/shards/src/app.rs`
- **LOCATION**: In the `mod tests` block (after line 240)
- **IMPLEMENT**:
```rust
#[test]
fn test_cli_cd_command() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["shards", "cd", "test-branch"]);
    assert!(matches.is_ok());

    let matches = matches.unwrap();
    let cd_matches = matches.subcommand_matches("cd").unwrap();
    assert_eq!(
        cd_matches.get_one::<String>("branch").unwrap(),
        "test-branch"
    );
}

#[test]
fn test_cli_cd_requires_branch() {
    let app = build_cli();
    let matches = app.try_get_matches_from(vec!["shards", "cd"]);
    assert!(matches.is_err()); // Branch is required
}
```
- **VALIDATE**: `cargo test -p shards test_cli_cd`

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: TYPE_CHECK

```bash
cargo check --all
```

**EXPECT**: Exit 0, no type errors

### Level 3: BUILD

```bash
cargo build --all
```

**EXPECT**: Exit 0, clean build

### Level 4: TESTS

```bash
cargo test --all
```

**EXPECT**: All tests pass

### Level 5: MANUAL_VALIDATION

```bash
# Test with non-existent shard
cargo run -- cd non-existent
# Expected: Error message "Session 'non-existent' not found" (or similar)

# Create a test shard
cargo run -- create test-cd-feature

# Test cd command
cargo run -- cd test-cd-feature
# Expected: Prints path like /Users/x/.shards/worktrees/shards/test-cd-feature

# Test shell integration
cd "$(cargo run -- cd test-cd-feature)"
pwd
# Expected: Should be in the worktree directory

# Cleanup
cargo run -- destroy test-cd-feature --force
```

---

## Acceptance Criteria

- [ ] `shards cd <branch>` prints the worktree path to stdout and exits 0
- [ ] `shards cd <non-existent>` prints error to stderr and exits non-zero
- [ ] Output contains ONLY the path (no prefix, no formatting, no extra newlines)
- [ ] Shell integration works: `cd "$(shards cd branch)"` changes directory
- [ ] All existing tests pass
- [ ] `cargo clippy` reports no warnings
- [ ] `cargo fmt --check` passes
- [ ] Logging follows established convention (`cli.cd_started`, `cli.cd_completed`, `cli.cd_failed`)

---

## Completion Checklist

- [ ] Task 1: `cd` subcommand added to CLI definition
- [ ] Task 2: Handler function implemented and wired into router
- [ ] Task 3: CLI tests added
- [ ] Level 1-4 validation commands pass
- [ ] Manual testing completed
