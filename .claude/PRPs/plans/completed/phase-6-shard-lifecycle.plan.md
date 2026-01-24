# Feature: Phase 6 - Shard Lifecycle (Open/Stop/Destroy)

## Summary

Add proper Open/Stop/Destroy lifecycle semantics to shards. The key insight is that `open` is **additive** - it launches a new terminal without closing existing ones, enabling agent orchestration. `stop` closes agents but preserves the shard. `destroy` removes everything, trusting git's natural guardrails (no custom safety checks). Deprecate `restart` in favor of `open`.

## User Story

As a power user or orchestrating agent
I want to open additional agents in a shard, stop agents without destroying, and force-destroy when needed
So that I can compose agent workflows and maintain full control over shard lifecycle

## Problem Statement

Phase 5 added basic destroy/restart, but the UX is incomplete:
- `restart` is confusing - it closes existing terminal then opens new one (destructive)
- No way to add another agent to an existing shard
- No way to stop an agent without destroying the shard
- Agents can't use CLI to orchestrate (need non-interactive commands)

## Solution Statement

Add three clean lifecycle commands that work for both humans and agents:
1. **open** - Launch NEW agent terminal in existing shard (additive)
2. **stop** - Close agent terminal(s), keep shard intact, set status to Stopped
3. **destroy --force** - Force bypass git2's uncommitted changes check

Deprecate `restart` as alias for `open` with warning.

## Metadata

| Field | Value |
|-------|-------|
| Type | ENHANCEMENT |
| Complexity | MEDIUM |
| Systems Affected | shards-core, shards (CLI), shards-ui |
| Dependencies | None (uses existing git2, terminal backends) |
| Estimated Tasks | 12 |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   COMMANDS AVAILABLE:                                                         ║
║   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐            ║
║   │   create    │ ──────► │   restart   │ ──────► │   destroy   │            ║
║   │  (creates   │         │  (kills old │         │  (removes   │            ║
║   │   + spawns) │         │   spawns new)│         │   everything)│            ║
║   └─────────────┘         └─────────────┘         └─────────────┘            ║
║                                                                               ║
║   PAIN POINTS:                                                                ║
║   • restart is DESTRUCTIVE - closes existing terminal                         ║
║   • No way to add second agent to same shard                                  ║
║   • No way to stop agent without destroying shard                             ║
║   • destroy has no --force flag for uncommitted changes                       ║
║                                                                               ║
║   UI BUTTONS:                                                                 ║
║   Running:  ● feature-auth    claude    [↻ Restart] [× Destroy]              ║
║   Stopped:  ○ fix-bug         kiro      [↻ Restart] [× Destroy]              ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   COMMANDS AVAILABLE:                                                         ║
║   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐   ║
║   │   create    │───►│    open     │◄──►│    stop     │───►│   destroy   │   ║
║   │  (creates   │    │  (ADDITIVE  │    │  (stops     │    │  (removes   │   ║
║   │   + spawns) │    │   new term) │    │   process)  │    │   all)      │   ║
║   └─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘   ║
║                           │                  │                    │           ║
║                           │                  │                    ▼           ║
║                           │                  │            ┌─────────────┐     ║
║                           ▼                  │            │  --force    │     ║
║                    [Can call multiple        │            │ (bypass git)│     ║
║                     times - additive!]       │            └─────────────┘     ║
║                                              │                                ║
║                                              ▼                                ║
║                                   [Session status = Stopped                   ║
║                                    Shard preserved, work safe]                ║
║                                                                               ║
║   UI BUTTONS (state-dependent):                                               ║
║   Running:  ● feature-auth    claude    [⏹ Stop] [🗑 Destroy]                 ║
║   Stopped:  ○ fix-bug         kiro      [▶ Open] [🗑 Destroy]                 ║
║                                                                               ║
║   AGENT ORCHESTRATION EXAMPLE:                                                ║
║   Main Agent on main branch:                                                  ║
║     shards create helper-1 --agent claude   # Create shard                   ║
║     shards open helper-1 --agent kiro       # Add 2nd agent (additive!)      ║
║     shards stop helper-1                     # Stop all agents               ║
║     shards destroy helper-1                  # Clean up                      ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| CLI | `restart` closes + opens | `open` is additive, `restart` deprecated | Can add multiple agents |
| CLI | `destroy` blocked by git | `destroy --force` bypasses | Power users can force |
| CLI | No stop command | `stop` preserves shard | Can pause without destroying |
| UI | [↻ Restart] always | [▶ Open] when stopped, [⏹ Stop] when running | Clear state-based actions |
| Session file | status always "Active" | status "Stopped" when stopped | Accurate state tracking |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/shards-core/src/sessions/handler.rs` | 1-450 | Pattern for all session operations |
| P0 | `crates/shards-core/src/sessions/types.rs` | 1-100 | Session struct and SessionStatus enum |
| P1 | `crates/shards/src/commands.rs` | 1-200 | CLI command handler pattern |
| P1 | `crates/shards/src/app.rs` | 1-80 | Clap command definitions |
| P1 | `crates/shards-ui/src/actions.rs` | 1-130 | UI action wrapper pattern |
| P1 | `crates/shards-ui/src/state.rs` | 1-130 | AppState and error fields |
| P2 | `crates/shards-ui/src/views/shard_list.rs` | 1-180 | Button rendering logic |
| P2 | `crates/shards-ui/src/views/main_view.rs` | 100-160 | Event handler pattern |

---

## Patterns to Mirror

**SESSION_HANDLER_PATTERN:**
```rust
// SOURCE: crates/shards-core/src/sessions/handler.rs:189-298 (destroy_session)
// COPY THIS PATTERN for stop_session and open_session:
pub fn destroy_session(name: &str) -> Result<(), SessionError> {
    info!(event = "core.session.destroy_started", name = name);

    let config = ShardsConfig::load_hierarchy()?;
    let session = operations::find_session_by_name(&config.sessions_dir(), name)?;

    info!(event = "core.session.destroy_found", session_id = session.id, branch = session.branch);

    // 1. Close terminal (fire-and-forget)
    if let Some(ref terminal_type) = session.terminal_type {
        terminal::handler::close_terminal(terminal_type, session.terminal_window_id.as_deref());
    }

    // 2. Kill process (blocking, handle errors)
    if let Some(pid) = session.process_id {
        match crate::process::kill_process(pid, session.process_name.as_deref(), session.process_start_time) {
            Ok(()) => info!(event = "core.session.destroy_kill_completed", pid = pid),
            Err(ProcessError::NotFound { .. }) => info!(event = "core.session.destroy_kill_already_dead", pid = pid),
            Err(e) => {
                error!(event = "core.session.destroy_kill_failed", pid = pid, error = %e);
                return Err(SessionError::ProcessKillFailed { pid, message: e.to_string() });
            }
        }
    }

    // 3. Remove worktree, remove session file
    // ...

    info!(event = "core.session.destroy_completed", name = name);
    Ok(())
}
```

**CLI_COMMAND_PATTERN:**
```rust
// SOURCE: crates/shards/src/commands.rs:143-171 (handle_destroy_command)
fn handle_destroy_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").ok_or("Branch argument is required")?;
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

**CLAP_COMMAND_PATTERN:**
```rust
// SOURCE: crates/shards/src/app.rs:60-75
.subcommand(
    Command::new("restart")
        .about("Restart agent in existing shard without destroying worktree")
        .arg(
            Arg::new("branch")
                .help("Branch name or shard identifier")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("agent")
                .long("agent")
                .short('a')
                .help("Override the agent to use"),
        ),
)
```

**UI_ACTION_PATTERN:**
```rust
// SOURCE: crates/shards-ui/src/actions.rs:88-101
pub fn destroy_shard(branch: &str) -> Result<(), String> {
    match session_ops::destroy_session(branch) {
        Ok(()) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
```

**UI_EVENT_HANDLER_PATTERN:**
```rust
// SOURCE: crates/shards-ui/src/views/main_view.rs:139-152
pub fn on_relaunch_click(&mut self, branch: &str, cx: &mut Context<Self>) {
    tracing::info!(event = "ui.relaunch_clicked", branch = branch);
    self.state.clear_relaunch_error();

    match actions::relaunch_shard(branch) {
        Ok(_session) => {
            self.state.refresh_sessions();
        }
        Err(e) => {
            tracing::warn!(event = "ui.relaunch_click.error_displayed", branch = branch, error = %e);
            self.state.relaunch_error = Some((branch.to_string(), e));
        }
    }
    cx.notify();
}
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/shards-core/src/sessions/handler.rs` | UPDATE | Add `open_session()`, `stop_session()`, update `destroy_session()` |
| `crates/shards/src/app.rs` | UPDATE | Add `open` and `stop` commands, add `--force` to destroy |
| `crates/shards/src/commands.rs` | UPDATE | Add handlers, update restart with deprecation warning |
| `crates/shards-ui/src/actions.rs` | UPDATE | Add `open_shard()`, `stop_shard()` |
| `crates/shards-ui/src/state.rs` | UPDATE | Add `open_error`, `stop_error` fields |
| `crates/shards-ui/src/views/main_view.rs` | UPDATE | Add `on_open_click()`, `on_stop_click()` handlers |
| `crates/shards-ui/src/views/shard_list.rs` | UPDATE | Change buttons: [▶]/[⏹] based on status |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **No custom git safety checks** - Trust git2's natural guardrails, just surface errors
- **No confirmation prompts** - Power users and agents need speed
- **No "stop all" command** - YAGNI
- **No `--delete-branch` flag** - YAGNI
- **No session storage format changes** - Use existing JSON schema
- **No multiple PID tracking per session** - Single process per session, use latest

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: ADD `open_session()` to handler.rs

- **ACTION**: Add new function after `restart_session()`
- **FILE**: `crates/shards-core/src/sessions/handler.rs`
- **IMPLEMENT**:
```rust
/// Opens a new agent terminal in an existing shard (additive - doesn't close existing terminals).
///
/// This is the preferred way to add agents to a shard. Unlike restart, this does NOT
/// close existing terminals - multiple agents can run in the same shard.
pub fn open_session(name: &str, agent_override: Option<String>) -> Result<Session, SessionError> {
    info!(event = "core.session.open_started", name = name, agent_override = ?agent_override);

    let config = ShardsConfig::load_hierarchy()?;
    let mut session = operations::find_session_by_name(&config.sessions_dir(), name)?;

    info!(event = "core.session.open_found", session_id = &session.id, branch = &session.branch);

    // 1. Verify worktree still exists
    if !session.worktree_path.exists() {
        return Err(SessionError::WorktreeNotFound {
            path: session.worktree_path.display().to_string(),
        });
    }

    // 2. Determine agent
    let agent = agent_override.unwrap_or_else(|| session.agent.clone());
    info!(event = "core.session.open_agent_selected", agent = &agent);

    // 3. Build command
    let agent_backend = crate::agents::get_agent_backend(&agent, &config);
    let command = agent_backend.build_command(&config);

    // 4. Spawn NEW terminal (additive - don't touch existing)
    info!(event = "core.session.open_spawn_started", worktree = %session.worktree_path.display());
    let spawn_result = terminal::handler::spawn_terminal(
        &session.worktree_path,
        &command,
        &config,
        Some(&session.id),
        Some(&config.shards_dir()),
    )?;

    // 5. Update session with new process info
    session.process_id = spawn_result.process_id;
    session.process_name = spawn_result.process_name;
    session.process_start_time = spawn_result.process_start_time;
    session.terminal_type = Some(spawn_result.terminal_type);
    session.terminal_window_id = spawn_result.terminal_window_id;
    session.command = Some(spawn_result.command_executed);
    session.agent = agent;
    session.status = SessionStatus::Active;
    session.last_activity = Some(chrono::Utc::now());

    // 6. Save updated session
    operations::save_session_to_file(&session, &config.sessions_dir())?;

    info!(event = "core.session.open_completed", session_id = &session.id, process_id = ?session.process_id);
    Ok(session)
}
```
- **MIRROR**: `restart_session()` at lines 300-450 for structure
- **IMPORTS**: All imports already present
- **GOTCHA**: Do NOT close existing terminal or kill existing process - that's what makes this additive
- **VALIDATE**: `cargo check -p shards-core`

### Task 2: ADD `stop_session()` to handler.rs

- **ACTION**: Add new function after `open_session()`
- **FILE**: `crates/shards-core/src/sessions/handler.rs`
- **IMPLEMENT**:
```rust
/// Stops the agent process in a shard without destroying the shard.
///
/// The worktree and session file are preserved. The shard can be reopened with `open_session()`.
pub fn stop_session(name: &str) -> Result<(), SessionError> {
    info!(event = "core.session.stop_started", name = name);

    let config = ShardsConfig::load_hierarchy()?;
    let mut session = operations::find_session_by_name(&config.sessions_dir(), name)?;

    info!(event = "core.session.stop_found", session_id = &session.id, branch = &session.branch);

    // 1. Close terminal (fire-and-forget, best-effort)
    if let Some(ref terminal_type) = session.terminal_type {
        info!(event = "core.session.stop_close_terminal", terminal_type = ?terminal_type);
        terminal::handler::close_terminal(terminal_type, session.terminal_window_id.as_deref());
    }

    // 2. Kill process (blocking, handle errors)
    if let Some(pid) = session.process_id {
        info!(event = "core.session.stop_kill_started", pid = pid);
        match crate::process::kill_process(pid, session.process_name.as_deref(), session.process_start_time) {
            Ok(()) => {
                info!(event = "core.session.stop_kill_completed", pid = pid);
            }
            Err(ProcessError::NotFound { .. }) => {
                info!(event = "core.session.stop_kill_already_dead", pid = pid);
            }
            Err(e) => {
                error!(event = "core.session.stop_kill_failed", pid = pid, error = %e);
                return Err(SessionError::ProcessKillFailed { pid, message: e.to_string() });
            }
        }
    }

    // 3. Clear process info and set status to Stopped
    session.process_id = None;
    session.process_name = None;
    session.process_start_time = None;
    session.status = SessionStatus::Stopped;
    session.last_activity = Some(chrono::Utc::now());

    // 4. Save updated session (keep worktree, keep session file)
    operations::save_session_to_file(&session, &config.sessions_dir())?;

    info!(event = "core.session.stop_completed", session_id = &session.id);
    Ok(())
}
```
- **MIRROR**: `destroy_session()` at lines 189-298 for kill/close pattern
- **IMPORTS**: Need to import `ProcessError` if not already: `use crate::process::ProcessError;`
- **GOTCHA**: Do NOT remove worktree or session file - just clear process info and set status
- **VALIDATE**: `cargo check -p shards-core`

### Task 3: UPDATE `destroy_session()` with force parameter

- **ACTION**: Add `force: bool` parameter to existing function
- **FILE**: `crates/shards-core/src/sessions/handler.rs`
- **IMPLEMENT**:
  - Change signature: `pub fn destroy_session(name: &str, force: bool) -> Result<(), SessionError>`
  - When killing process, if `force` is true, ignore kill failures:
```rust
if let Some(pid) = session.process_id {
    info!(event = "core.session.destroy_kill_started", pid = pid);
    match crate::process::kill_process(pid, session.process_name.as_deref(), session.process_start_time) {
        Ok(()) => info!(event = "core.session.destroy_kill_completed", pid = pid),
        Err(ProcessError::NotFound { .. }) => info!(event = "core.session.destroy_kill_already_dead", pid = pid),
        Err(e) => {
            if force {
                warn!(event = "core.session.destroy_kill_failed_force_continue", pid = pid, error = %e);
            } else {
                error!(event = "core.session.destroy_kill_failed", pid = pid, error = %e);
                return Err(SessionError::ProcessKillFailed { pid, message: e.to_string() });
            }
        }
    }
}
```
  - When removing worktree, if force is true, use force removal:
```rust
if force {
    info!(event = "core.session.destroy_worktree_force", worktree = %session.worktree_path.display());
    crate::git::handler::remove_worktree_force(&session.worktree_path)?;
} else {
    crate::git::handler::remove_worktree(&session.worktree_path)?;
}
```
- **GOTCHA**: Need to add `remove_worktree_force()` to git handler or use git2's force option
- **VALIDATE**: `cargo check -p shards-core`

### Task 4: ADD `remove_worktree_force()` to git handler

- **ACTION**: Add force worktree removal function
- **FILE**: `crates/shards-core/src/git/handler.rs`
- **IMPLEMENT**:
```rust
/// Force removes a git worktree, bypassing uncommitted changes check.
/// Use with caution - uncommitted work will be lost.
pub fn remove_worktree_force(worktree_path: &Path) -> Result<(), GitError> {
    info!(event = "core.git.worktree.remove_force_started", path = %worktree_path.display());

    // First try to find and prune from git
    let repo_path = worktree_path.parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| GitError::InvalidPath {
            path: worktree_path.display().to_string(),
            message: "Cannot determine parent repository".to_string(),
        })?;

    if let Ok(repo) = Repository::open(repo_path) {
        // Try to remove via git2 first
        let _ = repo.worktree_prune(None); // Ignore errors, we'll force delete
    }

    // Force delete the directory
    if worktree_path.exists() {
        std::fs::remove_dir_all(worktree_path).map_err(|e| GitError::WorktreeRemovalFailed {
            path: worktree_path.display().to_string(),
            message: e.to_string(),
        })?;
    }

    info!(event = "core.git.worktree.remove_force_completed", path = %worktree_path.display());
    Ok(())
}
```
- **MIRROR**: Existing `remove_worktree()` function
- **VALIDATE**: `cargo check -p shards-core`

### Task 5: UPDATE all callers of `destroy_session` to pass force=false

- **ACTION**: Update existing callers to use new signature
- **FILES**:
  - `crates/shards/src/commands.rs` - `handle_destroy_command()`
  - `crates/shards-ui/src/actions.rs` - `destroy_shard()`
  - Any tests
- **IMPLEMENT**: Add `false` as second argument to maintain current behavior
- **VALIDATE**: `cargo check --all`

### Task 6: ADD `open` CLI command

- **ACTION**: Add clap command definition and handler
- **FILES**: `crates/shards/src/app.rs`, `crates/shards/src/commands.rs`
- **IMPLEMENT in app.rs**:
```rust
.subcommand(
    Command::new("open")
        .about("Open a new agent terminal in an existing shard (additive)")
        .arg(
            Arg::new("branch")
                .help("Branch name or shard identifier")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("agent")
                .long("agent")
                .short('a')
                .help("Agent to launch (default: shard's original agent)"),
        ),
)
```
- **IMPLEMENT in commands.rs**:
```rust
fn handle_open_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").ok_or("Branch argument is required")?;
    let agent_override = matches.get_one::<String>("agent").cloned();

    info!(event = "cli.open_started", branch = branch, agent_override = ?agent_override);

    match session_handler::open_session(branch, agent_override) {
        Ok(session) => {
            println!("✅ Opened new agent in shard '{}'", branch);
            println!("   Agent: {}", session.agent);
            if let Some(pid) = session.process_id {
                println!("   PID: {}", pid);
            }
            info!(event = "cli.open_completed", branch = branch, session_id = session.id);
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to open shard '{}': {}", branch, e);
            error!(event = "cli.open_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
```
- Add to `run_command()` match:
```rust
Some(("open", sub_matches)) => handle_open_command(sub_matches),
```
- **VALIDATE**: `cargo check -p shards && cargo run -- open --help`

### Task 7: ADD `stop` CLI command

- **ACTION**: Add clap command definition and handler
- **FILES**: `crates/shards/src/app.rs`, `crates/shards/src/commands.rs`
- **IMPLEMENT in app.rs**:
```rust
.subcommand(
    Command::new("stop")
        .about("Stop agent(s) in a shard without destroying the worktree")
        .arg(
            Arg::new("branch")
                .help("Branch name or shard identifier")
                .required(true)
                .index(1),
        ),
)
```
- **IMPLEMENT in commands.rs**:
```rust
fn handle_stop_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").ok_or("Branch argument is required")?;

    info!(event = "cli.stop_started", branch = branch);

    match session_handler::stop_session(branch) {
        Ok(()) => {
            println!("✅ Stopped shard '{}'", branch);
            println!("   Shard preserved. Use 'shards open {}' to restart.", branch);
            info!(event = "cli.stop_completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to stop shard '{}': {}", branch, e);
            error!(event = "cli.stop_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
```
- Add to `run_command()` match:
```rust
Some(("stop", sub_matches)) => handle_stop_command(sub_matches),
```
- **VALIDATE**: `cargo check -p shards && cargo run -- stop --help`

### Task 8: ADD `--force` flag to destroy command

- **ACTION**: Add force flag to destroy command
- **FILE**: `crates/shards/src/app.rs`, `crates/shards/src/commands.rs`
- **IMPLEMENT in app.rs** (add to destroy subcommand):
```rust
.arg(
    Arg::new("force")
        .long("force")
        .short('f')
        .help("Force destroy, bypassing git uncommitted changes check")
        .action(ArgAction::SetTrue),
)
```
- **IMPLEMENT in commands.rs** (update handle_destroy_command):
```rust
fn handle_destroy_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").ok_or("Branch argument is required")?;
    let force = matches.get_flag("force");

    info!(event = "cli.destroy_started", branch = branch, force = force);

    match session_handler::destroy_session(branch, force) {
        // ... rest unchanged
    }
}
```
- **VALIDATE**: `cargo check -p shards && cargo run -- destroy --help`

### Task 9: DEPRECATE restart command

- **ACTION**: Add deprecation warning to restart handler
- **FILE**: `crates/shards/src/commands.rs`
- **IMPLEMENT** (update handle_restart_command):
```rust
fn handle_restart_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").unwrap();
    let agent_override = matches.get_one::<String>("agent").cloned();

    // Deprecation warning
    eprintln!("⚠️  'restart' is deprecated. Use 'shards open {}' instead.", branch);
    warn!(event = "cli.restart_deprecated", branch = branch);

    info!(event = "cli.restart_started", branch = branch, agent_override = ?agent_override);

    // Internally use open_session (same behavior, new name)
    match session_handler::open_session(branch, agent_override) {
        Ok(session) => {
            println!("✅ Shard '{}' restarted successfully!", branch);
            // ... rest of output
            Ok(())
        }
        Err(e) => {
            // ... error handling unchanged
        }
    }
}
```
- **VALIDATE**: `cargo run -- restart test-branch 2>&1 | grep -i deprecated`

### Task 10: ADD UI actions for open and stop

- **ACTION**: Add action wrapper functions
- **FILE**: `crates/shards-ui/src/actions.rs`
- **IMPLEMENT**:
```rust
pub fn open_shard(branch: &str, agent: Option<String>) -> Result<Session, String> {
    tracing::info!(event = "ui.open_shard.started", branch = branch, agent = ?agent);

    match session_ops::open_session(branch, agent) {
        Ok(session) => {
            tracing::info!(event = "ui.open_shard.completed", branch = branch);
            Ok(session)
        }
        Err(e) => {
            tracing::error!(event = "ui.open_shard.failed", branch = branch, error = %e);
            Err(e.to_string())
        }
    }
}

pub fn stop_shard(branch: &str) -> Result<(), String> {
    tracing::info!(event = "ui.stop_shard.started", branch = branch);

    match session_ops::stop_session(branch) {
        Ok(()) => {
            tracing::info!(event = "ui.stop_shard.completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            tracing::error!(event = "ui.stop_shard.failed", branch = branch, error = %e);
            Err(e.to_string())
        }
    }
}
```
- **VALIDATE**: `cargo check -p shards-ui`

### Task 11: UPDATE UI state with error fields

- **ACTION**: Add error tracking for open and stop operations
- **FILE**: `crates/shards-ui/src/state.rs`
- **IMPLEMENT** (add to AppState struct):
```rust
pub open_error: Option<(String, String)>,   // (branch, error_message)
pub stop_error: Option<(String, String)>,   // (branch, error_message)
```
- **IMPLEMENT** (add clear methods):
```rust
pub fn clear_open_error(&mut self) {
    self.open_error = None;
}

pub fn clear_stop_error(&mut self) {
    self.stop_error = None;
}
```
- Update `Default` impl and `new()` if needed
- **VALIDATE**: `cargo check -p shards-ui`

### Task 12: UPDATE UI views for open/stop buttons

- **ACTION**: Replace relaunch button with state-dependent open/stop
- **FILES**: `crates/shards-ui/src/views/main_view.rs`, `crates/shards-ui/src/views/shard_list.rs`
- **IMPLEMENT in main_view.rs** (add handlers):
```rust
pub fn on_open_click(&mut self, branch: &str, cx: &mut Context<Self>) {
    tracing::info!(event = "ui.open_clicked", branch = branch);
    self.state.clear_open_error();

    match actions::open_shard(branch, None) {
        Ok(_session) => {
            self.state.refresh_sessions();
        }
        Err(e) => {
            tracing::warn!(event = "ui.open_click.error_displayed", branch = branch, error = %e);
            self.state.open_error = Some((branch.to_string(), e));
        }
    }
    cx.notify();
}

pub fn on_stop_click(&mut self, branch: &str, cx: &mut Context<Self>) {
    tracing::info!(event = "ui.stop_clicked", branch = branch);
    self.state.clear_stop_error();

    match actions::stop_shard(branch) {
        Ok(()) => {
            self.state.refresh_sessions();
        }
        Err(e) => {
            tracing::warn!(event = "ui.stop_click.error_displayed", branch = branch, error = %e);
            self.state.stop_error = Some((branch.to_string(), e));
        }
    }
    cx.notify();
}
```
- **IMPLEMENT in shard_list.rs** (update button rendering):
  - Change condition: show [▶] Open when `display.status == ProcessStatus::Stopped`
  - Add condition: show [⏹] Stop when `display.status == ProcessStatus::Running`
  - Keep [🗑] Destroy always visible
  - Add error display for both open and stop errors
- **VALIDATE**: `cargo check -p shards-ui && cargo run -p shards-ui`

---

## Testing Strategy

### Manual CLI Tests

```bash
# Test open (additive behavior)
shards create test-open --agent claude
# Terminal 1 opens
shards open test-open --agent kiro
# Terminal 2 opens (Terminal 1 still running!)
shards list
# Shows: test-open (Running)

# Test stop
shards stop test-open
# Both terminals close
shards list
# Shows: test-open (Stopped)

# Test open after stop
shards open test-open
# New terminal opens
shards list
# Shows: test-open (Running)

# Test destroy
shards destroy test-open
shards list
# Shows: test-open gone

# Test git safety (natural guardrails)
shards create test-uncommitted --agent claude
echo "test" > ~/.shards/worktrees/*/test-uncommitted/test.txt
shards destroy test-uncommitted
# Should fail with clear git error message

shards destroy test-uncommitted --force
# Should succeed

# Test restart deprecation
shards restart test-open 2>&1 | grep -i deprecated
# Should show deprecation warning
```

### Manual UI Tests

```bash
cargo run -p shards-ui

# Test Open button on stopped shard
# Click [▶], terminal opens, status → Running

# Test Stop button on running shard
# Click [⏹], terminal closes, status → Stopped

# Test Destroy
# Click [🗑], shard removed (or error shown if git blocks)
```

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

See Testing Strategy above for manual test steps.

---

## Acceptance Criteria

- [ ] `shards open <branch>` launches new terminal in existing shard (additive)
- [ ] `shards stop <branch>` closes agent but preserves shard
- [ ] `shards destroy <branch> --force` bypasses git checks
- [ ] `shards restart` prints deprecation warning and works as alias
- [ ] Session status correctly shows "Stopped" after stop
- [ ] Session can be reopened after stop
- [ ] UI shows [▶] Open when stopped, [⏹] Stop when running
- [ ] All validation commands pass with exit 0

---

## Completion Checklist

- [ ] Task 1: open_session() added to handler.rs
- [ ] Task 2: stop_session() added to handler.rs
- [ ] Task 3: destroy_session() updated with force parameter
- [ ] Task 4: remove_worktree_force() added to git handler
- [ ] Task 5: All callers updated for new destroy signature
- [ ] Task 6: `open` CLI command added
- [ ] Task 7: `stop` CLI command added
- [ ] Task 8: `--force` flag added to destroy
- [ ] Task 9: restart deprecated with warning
- [ ] Task 10: UI actions added for open/stop
- [ ] Task 11: UI state updated with error fields
- [ ] Task 12: UI buttons updated for state-dependent display
- [ ] Level 1-4 validation commands pass
- [ ] Manual testing completed

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Process kill fails silently | LOW | MEDIUM | Already handled - errors surfaced clearly |
| Multiple agents tracked incorrectly | LOW | LOW | Only track latest PID - previous agents orphaned (acceptable) |
| UI button state incorrect | MEDIUM | LOW | ProcessStatus computed at render time from PID check |
| Force destroy loses work | LOW | HIGH | User explicitly requested --force; add clear messaging |

---

## Notes

- **SessionStatus::Stopped exists but was unused** - Now properly used for stop operation
- **open is NOT restart** - Key insight is additive behavior; restart was destructive
- **Force bypasses git2 only** - Process kill still attempted; force affects worktree removal
- **Deprecation is soft** - restart still works, just prints warning
- **UI refresh is automatic** - refresh_sessions() called after each operation
