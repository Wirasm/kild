# Feature: CLI `shards commits` Command

## Summary

Add a `shards commits <branch>` command that shows recent git commits in a shard's worktree branch. This enables users to see work progress in a shard without navigating into the worktree directory.

## User Story

As a power user managing multiple shards, I want to see recent commits in a shard without cd-ing into it, so that I can track work progress across shards.

## Problem Statement

Users managing multiple shards cannot quickly see what commits have been made in each shard without manually entering each worktree directory and running git commands. This breaks workflow when monitoring progress across parallel AI agents working on different features.

## Solution Statement

Add a `shards commits <branch>` command that:
1. Looks up the session by branch name
2. Runs `git log --oneline -n <count>` in the worktree directory
3. Outputs the commit list to stdout

The command follows the existing pattern used by `cd`, `code`, and `focus` commands - simple session lookup followed by an action in the worktree context.

## Metadata

| Field | Value |
|-------|-------|
| Type | NEW_CAPABILITY |
| Complexity | LOW |
| Systems Affected | CLI (crates/shards) |
| Dependencies | None - uses existing session_handler::get_session |
| Estimated Tasks | 2 |

---

## Mandatory Reading

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/shards/src/commands.rs` | 184-225 | `handle_cd_command` pattern to MIRROR - session lookup + simple action |
| P0 | `crates/shards/src/app.rs` | 69-78 | `cd` command registration pattern |
| P0 | `crates/shards/src/commands.rs` | 591-643 | `handle_focus_command` pattern - session lookup with error handling |
| P1 | `crates/shards-core/src/sessions/handler.rs` | 208-221 | `get_session` function for session lookup |

---

## Patterns to Mirror

**COMMAND_REGISTRATION_PATTERN:**
```rust
// SOURCE: crates/shards/src/app.rs:69-78
// COPY THIS PATTERN for commits command:
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

**COMMAND_HANDLER_PATTERN:**
```rust
// SOURCE: crates/shards/src/commands.rs:184-225
// COPY THIS PATTERN for handle_commits_command:
fn handle_cd_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    // Validate branch name
    if !is_valid_branch_name(branch) {
        eprintln!("Invalid branch name: {}", branch);
        error!(event = "cli.cd_invalid_branch", branch = branch);
        return Err("Invalid branch name".into());
    }

    info!(event = "cli.cd_started", branch = branch);

    match session_handler::get_session(branch) {
        Ok(session) => {
            // ACTION: Print path
            println!("{}", session.worktree_path.display());

            info!(
                event = "cli.cd_completed",
                branch = branch,
                path = %session.worktree_path.display()
            );

            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to get path for shard '{}': {}", branch, e);

            error!(
                event = "cli.cd_failed",
                branch = branch,
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/shards/src/app.rs` | UPDATE | Add `commits` subcommand with `branch` positional arg and `--count` optional arg (default: 10) |
| `crates/shards/src/commands.rs` | UPDATE | Add `handle_commits_command` function and wire it into `run_command` match arm |

---

## Step-by-Step Tasks

### Task 1: ADD commits subcommand to app.rs

- **ACTION**: Add `commits` subcommand definition to `build_cli()` function
- **LOCATION**: After the focus command (around line 162)
- **IMPLEMENT**:
  ```rust
  .subcommand(
      Command::new("commits")
          .about("Show recent commits in a shard's branch")
          .arg(
              Arg::new("branch")
                  .help("Branch name of the shard")
                  .required(true)
                  .index(1)
          )
          .arg(
              Arg::new("count")
                  .long("count")
                  .short('n')
                  .help("Number of commits to show (default: 10)")
                  .value_parser(clap::value_parser!(usize))
                  .default_value("10")
          )
  )
  ```
- **VALIDATE**: `cargo check -p shards && cargo run -- commits --help`

### Task 2: IMPLEMENT handle_commits_command in commands.rs

- **ACTION**: Add `handle_commits_command` function and wire into `run_command` match
- **LOCATION**: After handle_focus_command (around line 643)
- **IMPLEMENT**:

1. Add match arm in `run_command` function (around line 74):
```rust
Some(("commits", sub_matches)) => handle_commits_command(sub_matches),
```

2. Add handler function:
```rust
fn handle_commits_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let count = *matches.get_one::<usize>("count").unwrap_or(&10);

    // Validate branch name
    if !is_valid_branch_name(branch) {
        eprintln!("Invalid branch name: {}", branch);
        error!(event = "cli.commits_invalid_branch", branch = branch);
        return Err("Invalid branch name".into());
    }

    info!(event = "cli.commits_started", branch = branch, count = count);

    match session_handler::get_session(branch) {
        Ok(session) => {
            // Run git log in worktree directory
            let output = std::process::Command::new("git")
                .current_dir(&session.worktree_path)
                .args(["log", "--oneline", "-n", &count.to_string()])
                .output()?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("Git error: {}", stderr);
                error!(
                    event = "cli.commits_git_failed",
                    branch = branch,
                    error = %stderr
                );
                return Err(format!("git log failed: {}", stderr).into());
            }

            // Output commits to stdout
            std::io::Write::write_all(&mut std::io::stdout(), &output.stdout)?;

            info!(
                event = "cli.commits_completed",
                branch = branch,
                count = count
            );

            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to find shard '{}': {}", branch, e);
            error!(
                event = "cli.commits_failed",
                branch = branch,
                error = %e
            );
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
```

- **VALIDATE**: `cargo build -p shards && cargo fmt --check && cargo clippy --all -- -D warnings`

---

## Validation Commands

### Level 1: STATIC_ANALYSIS
```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

### Level 2: BUILD
```bash
cargo build --all
```

### Level 3: UNIT_TESTS
```bash
cargo test --all
```

### Level 4: MANUAL_TEST
```bash
# Create test shard
cargo run -- create test-commits --note "Testing commits command"

# Make a test commit in worktree
cd "$(cargo run -- cd test-commits)"
echo "test" > test-file.txt
git add test-file.txt
git commit -m "Test commit for commits command"
cd -

# Test commits command - default count
cargo run -- commits test-commits

# Test commits command - custom count
cargo run -- commits test-commits --count 5
cargo run -- commits test-commits -n 3

# Test error case - non-existent branch
cargo run -- commits non-existent-branch 2>&1

# Cleanup
cargo run -- destroy test-commits --force
```

---

## Acceptance Criteria

- [ ] `shards commits <branch>` shows recent commits (default 10)
- [ ] `shards commits <branch> --count N` shows last N commits
- [ ] `shards commits <branch> -n N` shows last N commits (short flag)
- [ ] Error handling for non-existent branch with clear message
- [ ] Branch name validation (rejects invalid names like ".." or "/")
- [ ] Follows existing logging conventions (cli.commits_started, cli.commits_completed, cli.commits_failed)
- [ ] All static analysis passes (fmt, clippy)
- [ ] All existing tests pass

---

## Completion Checklist

- [ ] Task 1: commits subcommand added to app.rs
- [ ] Task 2: handle_commits_command implemented in commands.rs
- [ ] Test: `shards commits --help` shows usage
- [ ] Test: `shards commits <branch>` works with active shard
- [ ] Test: `shards commits <branch> --count 5` works
- [ ] Test: Non-existent branch shows error
- [ ] Validation: `cargo fmt --check` passes
- [ ] Validation: `cargo clippy --all -- -D warnings` passes
- [ ] Validation: `cargo test --all` passes
- [ ] Validation: `cargo build --all` succeeds
