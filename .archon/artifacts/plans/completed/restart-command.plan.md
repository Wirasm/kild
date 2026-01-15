# Feature: Shards Restart Command

## Summary

Implement `shards restart <name>` command that kills and restarts an agent process in an existing worktree without destroying the worktree itself. This enables users to restart agents with the same or different configuration while preserving their work context. The command reuses existing process killing logic from destroy but skips worktree removal, then relaunches the terminal and updates the session with the new PID.

## User Story

As a developer using Shards
I want to restart an agent in an existing shard
So that I can recover from agent crashes, switch agents, or refresh the agent state without losing my worktree and uncommitted work

## Problem Statement

Currently, when an agent crashes or needs to be restarted, users must:
1. Manually kill the process
2. Manually navigate to the worktree directory
3. Manually launch the agent again

Or they must use `shards destroy` which removes the entire worktree, losing uncommitted work. There's no way to restart an agent while preserving the worktree context.

## Solution Statement

Add a new `restart` subcommand that:
1. Finds the session by branch name
2. Kills the existing agent process (reusing destroy logic)
3. Keeps the worktree intact (unlike destroy)
4. Relaunches the terminal with same or new agent configuration
5. Updates the session file with new PID and metadata

This follows the existing vertical slice architecture with handler/operations pattern in the sessions feature slice.

## Metadata

| Field | Value |
|-------|-------|
| Type | NEW_CAPABILITY |
| Complexity | LOW |
| Systems Affected | sessions, cli, process, terminal |
| Dependencies | clap 4.0, sysinfo 0.37.2, tracing 0.1 |
| Estimated Tasks | 5 |

---

## UX Design

### Before State
```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │   Agent     │ ──────► │   Crashes   │ ──────► │   Manual    │            ║
║   │   Running   │         │   or Hangs  │         │   Recovery  │            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                                            │                  ║
║                                                            ▼                  ║
║                                                   ┌─────────────┐             ║
║                                                   │  Destroy    │             ║
║                                                   │  Worktree   │             ║
║                                                   │  (lose work)│             ║
║                                                   └─────────────┘             ║
║                                                                               ║
║   USER_FLOW:                                                                  ║
║   1. Agent crashes or becomes unresponsive                                    ║
║   2. User must manually kill process                                          ║
║   3. User must manually cd to worktree                                        ║
║   4. User must manually launch agent again                                    ║
║   OR: User runs `shards destroy` and loses uncommitted work                   ║
║                                                                               ║
║   PAIN_POINT: No way to restart agent without manual intervention or          ║
║               destroying worktree and losing uncommitted changes              ║
║                                                                               ║
║   DATA_FLOW: Session → Manual intervention → Lost context                     ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝

╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │   Agent     │ ──────► │   Crashes   │ ──────► │   shards    │            ║
║   │   Running   │         │   or Hangs  │         │   restart   │            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                                            │                  ║
║                                                            ▼                  ║
║                                                   ┌─────────────┐             ║
║                                                   │  Kill PID   │             ║
║                                                   │  Keep Tree  │             ║
║                                                   │  Relaunch   │             ║
║                                                   └─────────────┘             ║
║                                                            │                  ║
║                                                            ▼                  ║
║                                                   ┌─────────────┐             ║
║                                                   │   Agent     │             ║
║                                                   │   Running   │             ║
║                                                   │   (new PID) │             ║
║                                                   └─────────────┘             ║
║                                                                               ║
║   USER_FLOW:                                                                  ║
║   1. Agent crashes or becomes unresponsive                                    ║
║   2. User runs: `shards restart <branch>`                                     ║
║   3. Agent restarts automatically in same worktree                            ║
║   4. Uncommitted work is preserved                                            ║
║                                                                               ║
║   VALUE_ADD:                                                                  ║
║   - One command to restart agent                                              ║
║   - Worktree and uncommitted work preserved                                   ║
║   - Can switch agents with --agent flag                                       ║
║   - Session tracking updated automatically                                    ║
║                                                                               ║
║   DATA_FLOW: Session → Kill Process → Relaunch Terminal → Update Session     ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| CLI | No restart command | `shards restart <name>` | Can restart agent in one command |
| CLI | Must destroy to restart | `shards restart <name> --agent <agent>` | Can switch agents without losing work |
| Session | PID becomes stale after crash | PID updated after restart | Session tracking stays accurate |
| Worktree | Lost on destroy | Preserved on restart | Uncommitted work is safe |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `src/sessions/handler.rs` | 130-200 | destroy_session pattern to MIRROR for process killing |
| P0 | `src/sessions/handler.rs` | 10-100 | create_session pattern to MIRROR for terminal launching |
| P0 | `src/cli/commands.rs` | 80-120 | handle_destroy_command pattern to MIRROR for CLI handler |
| P1 | `src/sessions/types.rs` | 1-70 | Session struct with process_id, process_name, process_start_time |
| P1 | `src/sessions/operations.rs` | 150-280 | save_session_to_file, find_session_by_name patterns |
| P1 | `src/process/operations.rs` | 1-70 | kill_process function with PID validation |
| P1 | `src/terminal/handler.rs` | 1-80 | spawn_terminal function with SpawnResult |
| P2 | `src/cli/app.rs` | 1-150 | CLI command structure with clap |
| P2 | `src/core/config.rs` | 200-250 | get_agent_command for agent mapping |

**External Documentation:**
| Source | Section | Why Needed |
|--------|---------|------------|
| [clap 4.0 Docs](https://docs.rs/clap/4.0/clap/) | Subcommands | Adding restart subcommand with args |
| [sysinfo Docs](https://docs.rs/sysinfo/0.37.2/sysinfo/) | Process killing | Understanding PID validation |

---

## Patterns to Mirror

**CLI_COMMAND_STRUCTURE:**
```rust
// SOURCE: src/cli/app.rs:60-75
// COPY THIS PATTERN:
.subcommand(
    Command::new("destroy")
        .about("Remove shard completely")
        .arg(
            Arg::new("branch")
                .help("Branch name of the shard to destroy")
                .required(true)
                .index(1)
        )
)
```

**CLI_HANDLER_PATTERN:**
```rust
// SOURCE: src/cli/commands.rs:80-120
// COPY THIS PATTERN:
fn handle_destroy_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").unwrap();

    info!(event = "cli.destroy_started", branch = branch);

    match session_handler::destroy_session(branch) {
        Ok(()) => {
            println!("✅ Shard '{}' destroyed successfully!", branch);
            info!(event = "cli.destroy_completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to destroy shard '{}': {}", branch, e);
            error!(event = "cli.destroy_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
```

**SESSION_HANDLER_PATTERN:**
```rust
// SOURCE: src/sessions/handler.rs:130-200
// COPY THIS PATTERN FOR PROCESS KILLING:
pub fn destroy_session(name: &str) -> Result<(), SessionError> {
    info!(event = "session.destroy_started", name = name);

    let config = Config::new();
    
    // 1. Find session by name (branch name)
    let session = operations::find_session_by_name(&config.sessions_dir(), name)?
        .ok_or_else(|| SessionError::NotFound { name: name.to_string() })?;

    // 2. Kill process if PID is tracked
    if let Some(pid) = session.process_id {
        info!(event = "session.destroy_kill_started", pid = pid);

        match crate::process::kill_process(
            pid,
            session.process_name.as_deref(),
            session.process_start_time,
        ) {
            Ok(()) => {
                info!(event = "session.destroy_kill_completed", pid = pid);
            }
            Err(crate::process::ProcessError::NotFound { .. }) => {
                info!(event = "session.destroy_kill_already_dead", pid = pid);
            }
            Err(e) => {
                error!(event = "session.destroy_kill_failed", pid = pid, error = %e);
                return Err(SessionError::ProcessKillFailed {
                    pid,
                    message: format!("Process still running: {}", e),
                });
            }
        }
    }
    
    // NOTE: For restart, we DON'T remove worktree here
    // NOTE: For restart, we DON'T remove session file here
}
```

**TERMINAL_LAUNCHING_PATTERN:**
```rust
// SOURCE: src/sessions/handler.rs:70-90
// COPY THIS PATTERN FOR RELAUNCHING:
let spawn_result = terminal::handler::spawn_terminal(&worktree.path, &validated.command, shards_config)
    .map_err(|e| SessionError::TerminalError { source: e })?;

// Capture process metadata immediately for PID reuse protection
let (process_name, process_start_time) = if let Ok(info) = crate::process::get_process_info(spawn_result.process_id.unwrap()) {
    (Some(info.name), Some(info.start_time))
} else {
    (None, None)
};
```

**SESSION_UPDATE_PATTERN:**
```rust
// SOURCE: src/sessions/handler.rs:92-110
// COPY THIS PATTERN FOR UPDATING SESSION:
let session = Session {
    id: session_id.clone(),
    project_id: project.id,
    branch: validated.name.clone(),
    worktree_path: worktree.path,
    agent: validated.agent,
    status: SessionStatus::Active,
    created_at: chrono::Utc::now().to_rfc3339(),
    port_range_start: port_start,
    port_range_end: port_end,
    port_count: config.default_port_count,
    process_id: spawn_result.process_id,
    process_name: spawn_result.process_name.clone(),
    process_start_time: spawn_result.process_start_time,
};

// Save session to file
operations::save_session_to_file(&session, &config.sessions_dir())?;
```

**LOGGING_PATTERN:**
```rust
// SOURCE: src/sessions/handler.rs:10-20, 130-140
// COPY THIS PATTERN:
info!(
    event = "session.restart_started",
    branch = name,
    agent = agent
);

info!(
    event = "session.restart_completed",
    session_id = session.id,
    process_id = session.process_id
);

error!(
    event = "session.restart_failed",
    branch = name,
    error = %e
);
```

**ERROR_HANDLING:**
```rust
// SOURCE: src/sessions/errors.rs:1-70
// NO NEW ERRORS NEEDED - reuse existing:
// - SessionError::NotFound for missing session
// - SessionError::ProcessKillFailed for kill failures
// - SessionError::TerminalError for spawn failures
// - SessionError::IoError for file operations
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `src/cli/app.rs` | UPDATE | Add restart subcommand with --agent flag |
| `src/cli/commands.rs` | UPDATE | Add handle_restart_command function |
| `src/sessions/handler.rs` | UPDATE | Add restart_session function |
| `src/sessions/operations.rs` | UPDATE | Add update_session_process helper (optional) |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **--reset flag**: Git reset to origin/main before restart (mentioned as future feature)
- **--hard flag**: Clear command execution before restart (explicitly excluded by user)
- **Force flag**: Force kill if process won't die gracefully (can be added later if needed)
- **Session name changes**: Currently using branch names, future PR will address naming
- **Multiple restart strategies**: Only implementing basic kill + relaunch for now

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `src/cli/app.rs` - Add restart subcommand

- **ACTION**: ADD restart subcommand to CLI definition
- **IMPLEMENT**: Add after destroy subcommand, before status subcommand
- **MIRROR**: `src/cli/app.rs:60-75` - follow destroy command pattern exactly
- **ARGS**: 
  - `branch` (required, index 1) - branch name of shard to restart
  - `--agent` (optional, short 'a') - agent to use (overrides current)
- **GOTCHA**: Use same agent value_parser as create command: `["claude", "kiro", "gemini", "codex", "aether"]`
- **VALIDATE**: `cargo build --bin shards` - must compile without errors

```rust
.subcommand(
    Command::new("restart")
        .about("Restart agent in existing shard without destroying worktree")
        .arg(
            Arg::new("branch")
                .help("Branch name of the shard to restart")
                .required(true)
                .index(1)
        )
        .arg(
            Arg::new("agent")
                .long("agent")
                .short('a')
                .help("AI agent to use (overrides current agent)")
                .value_parser(["claude", "kiro", "gemini", "codex", "aether"])
        )
)
```

### Task 2: UPDATE `src/cli/commands.rs` - Add restart command handler

- **ACTION**: ADD handle_restart_command function and wire to run_command
- **IMPLEMENT**: 
  1. Add match arm in run_command for "restart" subcommand
  2. Create handle_restart_command function following destroy pattern
- **MIRROR**: `src/cli/commands.rs:80-120` - copy destroy handler structure
- **IMPORTS**: No new imports needed (session_handler already imported)
- **LOGGING**: Use events: `cli.restart_started`, `cli.restart_completed`, `cli.restart_failed`
- **OUTPUT**: Success message: "✅ Shard '{branch}' restarted successfully!"
- **GOTCHA**: Extract optional agent override: `matches.get_one::<String>("agent").cloned()`
- **VALIDATE**: `cargo build` - must compile, `cargo clippy` - no warnings

```rust
// In run_command match statement, add after destroy:
Some(("restart", sub_matches)) => handle_restart_command(sub_matches),

// Add function after handle_destroy_command:
fn handle_restart_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").unwrap();
    let agent_override = matches.get_one::<String>("agent").cloned();

    info!(event = "cli.restart_started", branch = branch, agent_override = ?agent_override);

    match session_handler::restart_session(branch, agent_override) {
        Ok(session) => {
            println!("✅ Shard '{}' restarted successfully!", branch);
            println!("   Agent: {}", session.agent);
            println!("   Process ID: {:?}", session.process_id);

            info!(event = "cli.restart_completed", branch = branch, process_id = session.process_id);

            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to restart shard '{}': {}", branch, e);

            error!(event = "cli.restart_failed", branch = branch, error = %e);

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
```

### Task 3: UPDATE `src/sessions/handler.rs` - Add restart_session function

- **ACTION**: ADD restart_session public function
- **IMPLEMENT**: Combine destroy's process killing + create's terminal launching
- **MIRROR**: 
  - Lines 130-180 from destroy_session for process killing
  - Lines 70-110 from create_session for terminal launching and session update
- **PATTERN**: 
  1. Load config and find session
  2. Kill existing process (if PID exists)
  3. Determine agent command (use override or keep current)
  4. Relaunch terminal in existing worktree
  5. Update session with new PID
  6. Save updated session to file
- **IMPORTS**: No new imports needed
- **LOGGING**: Events: `session.restart_started`, `session.restart_kill_started`, `session.restart_kill_completed`, `session.restart_spawn_started`, `session.restart_spawn_completed`, `session.restart_completed`, `session.restart_failed`
- **GOTCHA**: Don't remove worktree, don't remove session file, DO update existing session
- **VALIDATE**: `cargo build` - must compile

```rust
pub fn restart_session(name: &str, agent_override: Option<String>) -> Result<Session, SessionError> {
    info!(event = "session.restart_started", name = name, agent_override = ?agent_override);

    let config = Config::new();
    
    // 1. Find session by name (branch name)
    let mut session = operations::find_session_by_name(&config.sessions_dir(), name)?
        .ok_or_else(|| SessionError::NotFound { name: name.to_string() })?;

    info!(
        event = "session.restart_found",
        session_id = session.id,
        current_agent = session.agent,
        process_id = session.process_id
    );

    // 2. Kill process if PID is tracked
    if let Some(pid) = session.process_id {
        info!(event = "session.restart_kill_started", pid = pid);

        match crate::process::kill_process(
            pid,
            session.process_name.as_deref(),
            session.process_start_time,
        ) {
            Ok(()) => {
                info!(event = "session.restart_kill_completed", pid = pid);
            }
            Err(crate::process::ProcessError::NotFound { .. }) => {
                info!(event = "session.restart_kill_already_dead", pid = pid);
            }
            Err(e) => {
                error!(event = "session.restart_kill_failed", pid = pid, error = %e);
                return Err(SessionError::ProcessKillFailed {
                    pid,
                    message: format!("Process still running: {}", e),
                });
            }
        }
    }

    // 3. Determine agent and command
    let shards_config = ShardsConfig::load_hierarchy().unwrap_or_default();
    let agent = agent_override.unwrap_or_else(|| session.agent.clone());
    let agent_command = shards_config.get_agent_command(&agent);

    info!(
        event = "session.restart_agent_selected",
        session_id = session.id,
        agent = agent,
        command = agent_command
    );

    // 4. Relaunch terminal in existing worktree
    info!(event = "session.restart_spawn_started", worktree_path = %session.worktree_path.display());

    let spawn_result = terminal::handler::spawn_terminal(&session.worktree_path, &agent_command, &shards_config)
        .map_err(|e| SessionError::TerminalError { source: e })?;

    info!(
        event = "session.restart_spawn_completed",
        process_id = spawn_result.process_id,
        process_name = ?spawn_result.process_name
    );

    // 5. Update session with new process info
    session.agent = agent;
    session.process_id = spawn_result.process_id;
    session.process_name = spawn_result.process_name;
    session.process_start_time = spawn_result.process_start_time;
    session.status = SessionStatus::Active;

    // 6. Save updated session to file
    operations::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "session.restart_completed",
        session_id = session.id,
        branch = name,
        agent = session.agent,
        process_id = session.process_id
    );

    Ok(session)
}
```

### Task 4: UPDATE `src/cli/commands.rs` - Add restart to match statement

- **ACTION**: Wire restart subcommand to handler in run_command
- **IMPLEMENT**: Add match arm between destroy and status
- **MIRROR**: Existing match arms pattern
- **VALIDATE**: `cargo build && cargo test` - all tests pass

### Task 5: Manual testing and validation

- **ACTION**: Test restart command with real sessions
- **IMPLEMENT**: 
  1. Create a test shard: `cargo run -- create test-restart --agent kiro`
  2. Restart with same agent: `cargo run -- restart test-restart`
  3. Restart with different agent: `cargo run -- restart test-restart --agent claude`
  4. Verify worktree still exists after restart
  5. Verify session file updated with new PID
  6. Verify old process killed, new process running
- **VALIDATE**: All manual test scenarios pass

---

## Testing Strategy

### Unit Tests to Write

No new unit tests required - reusing existing tested functions:
- `process::kill_process` (already tested)
- `terminal::handler::spawn_terminal` (already tested)
- `operations::save_session_to_file` (already tested)
- `operations::find_session_by_name` (already tested)

### Edge Cases Checklist

- [x] Session not found - returns SessionError::NotFound
- [x] Process already dead - logs info, continues with restart
- [x] Process kill fails - returns SessionError::ProcessKillFailed
- [x] Terminal spawn fails - returns SessionError::TerminalError
- [x] Worktree path doesn't exist - terminal spawn will fail with error
- [x] Invalid agent override - clap validation prevents invalid agents
- [x] No PID tracked - skips kill step, proceeds to relaunch
- [x] Session file write fails - returns SessionError::IoError

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo build && cargo clippy -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: COMPILATION

```bash
cargo build --release
```

**EXPECT**: Binary builds successfully

### Level 3: HELP_TEXT

```bash
cargo run -- restart --help
```

**EXPECT**: Shows restart command help with branch arg and --agent flag

### Level 4: MANUAL_VALIDATION

```bash
# Test 1: Create a shard
cargo run -- create test-restart --agent kiro

# Test 2: Verify it's running
cargo run -- list
# Should show test-restart with Running status

# Test 3: Restart with same agent
cargo run -- restart test-restart
# Should show success message

# Test 4: Verify new PID
cargo run -- status test-restart
# Should show new PID, agent still kiro

# Test 5: Restart with different agent
cargo run -- restart test-restart --agent claude
# Should show success message

# Test 6: Verify agent changed
cargo run -- status test-restart
# Should show agent changed to claude

# Test 7: Verify worktree still exists
ls ~/.shards/worktrees/*/test-restart
# Should show worktree directory

# Test 8: Cleanup
cargo run -- destroy test-restart
```

### Level 5: ERROR_CASES

```bash
# Test 1: Restart non-existent shard
cargo run -- restart non-existent
# EXPECT: Error "Session 'non-existent' not found"

# Test 2: Invalid agent
cargo run -- restart test-restart --agent invalid
# EXPECT: Clap validation error before execution
```

---

## Acceptance Criteria

- [x] `shards restart <name>` kills process and relaunches agent in same worktree
- [x] `shards restart <name> --agent <agent>` switches to different agent
- [x] Worktree is preserved after restart (not removed)
- [x] Session file updated with new PID and process metadata
- [x] Old process is killed before new one launches
- [x] Structured logging events for all restart steps
- [x] Error handling for all failure modes (not found, kill failed, spawn failed)
- [x] Help text shows restart command and options
- [x] Level 1-5 validation commands pass

---

## Completion Checklist

- [ ] Task 1: restart subcommand added to CLI (app.rs)
- [ ] Task 2: handle_restart_command added to commands.rs
- [ ] Task 3: restart_session function added to handler.rs
- [ ] Task 4: restart wired to run_command match
- [ ] Task 5: Manual testing completed successfully
- [ ] Level 1: `cargo build && cargo clippy` passes
- [ ] Level 2: `cargo build --release` succeeds
- [ ] Level 3: `cargo run -- restart --help` shows correct help
- [ ] Level 4: All manual validation scenarios pass
- [ ] Level 5: Error cases handled correctly
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Process kill fails, leaves zombie | LOW | MEDIUM | Existing kill_process has PID validation and error handling; user can manually kill or use future --force flag |
| PID reused by OS between kill and check | LOW | LOW | Using process_name and process_start_time validation in kill_process |
| Terminal spawn fails after kill | MEDIUM | MEDIUM | Session file still exists, user can retry restart; process already dead so no resource leak |
| Worktree deleted externally | LOW | HIGH | Terminal spawn will fail with clear error; session validation checks worktree exists |
| Config load fails | LOW | LOW | Falls back to default config, same as create command |

---

## Notes

**Design Decisions:**
- Reusing existing destroy logic for process killing rather than duplicating code
- Reusing existing create logic for terminal launching rather than duplicating code
- Not adding new error types - existing SessionError variants cover all cases
- Using optional agent_override parameter rather than complex config merging
- Keeping session ID and port range unchanged (only updating process info)

**Trade-offs:**
- Simple implementation (5 tasks) vs feature-rich (would need more tasks for --reset, --hard, etc.)
- Chose simplicity - can add flags incrementally in future PRs

**Future Considerations:**
- Add --reset flag to git reset before restart (mentioned in requirements)
- Add --force flag to force kill stubborn processes
- Add restart-all command to restart all shards
- Add auto-restart on crash detection (requires process monitoring)
- Consider renaming from branch-based to session-name-based (future PR mentioned)

**Implementation Notes:**
- This is a LOW complexity feature because it composes existing well-tested functions
- No new data structures or complex logic needed
- Follows existing patterns exactly (vertical slice, handler/operations, structured logging)
- Should take < 1 hour to implement for someone familiar with codebase
