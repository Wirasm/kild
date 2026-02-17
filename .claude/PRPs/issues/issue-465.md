# Investigation: Daemon attach window not closed on kild complete/destroy

**Issue**: #465 (https://github.com/Wirasm/kild/issues/465)
**Type**: BUG
**Investigated**: 2026-02-17T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                        |
| ---------- | ------ | ---------------------------------------------------------------------------------------------------------------- |
| Severity   | MEDIUM | User-visible annoyance (orphaned terminal windows) with easy workaround (close manually). No data loss or crash. |
| Complexity | MEDIUM | 4 files changed, but changes are well-scoped to existing patterns already used in the terminal-managed path.     |
| Confidence | HIGH   | Root cause is clear from code inspection — terminal info is explicitly set to `None` at construction.            |

---

## Problem Statement

When completing or destroying a daemon-managed kild session, the Ghostty terminal window running `kild attach` stays open. The daemon PTY is killed correctly via IPC, but the terminal window hosting the attach process is never closed because its terminal type and window ID are never stored in the session's `AgentProcess`.

---

## Analysis

### Root Cause

WHY: The Ghostty attach window stays open after `kild destroy`/`kild complete`.
↓ BECAUSE: `destroy_session()` only closes terminal windows for agents that have `terminal_type` and `terminal_window_id` set.
Evidence: `crates/kild-core/src/sessions/destroy.rs:185-193` — conditional on `agent_proc.terminal_type()` and `agent_proc.terminal_window_id()` being `Some`

↓ BECAUSE: Daemon `AgentProcess` instances are constructed with `None, None` for terminal fields.
Evidence: `crates/kild-core/src/sessions/create.rs:408-409` — `None, None` passed to `AgentProcess::new()`

↓ BECAUSE: `spawn_attach_window()` is called from the CLI layer AFTER `create_session()` returns the already-persisted session.
Evidence: `crates/kild/src/commands/create.rs:96-101` — called after `session_ops::create_session()` succeeds

↓ ROOT CAUSE: The attach window spawning happens in the wrong layer (CLI instead of core), creating a timing gap where the `SpawnResult` with terminal info is available but can never be stored in the `AgentProcess` that was already constructed and saved.
Evidence: `crates/kild-core/src/sessions/daemon_helpers.rs:98-103` — `SpawnResult` logged but discarded (function returns `()`)

### Affected Files

| File                                                 | Lines   | Action | Description                                              |
| ---------------------------------------------------- | ------- | ------ | -------------------------------------------------------- |
| `crates/kild-core/src/sessions/daemon_helpers.rs`    | 65-116  | UPDATE | Return `Option<(TerminalType, String)>` from spawn       |
| `crates/kild-core/src/sessions/create.rs`            | 400-413 | UPDATE | Call `spawn_attach_window()`, pass result to AgentProcess |
| `crates/kild-core/src/sessions/open.rs`              | 384-396 | UPDATE | Call `spawn_attach_window()`, pass result to AgentProcess |
| `crates/kild-core/src/sessions/destroy.rs`           | 166-194 | UPDATE | Close terminal window in daemon path too                 |
| `crates/kild/src/commands/create.rs`                 | 95-101  | UPDATE | Remove `spawn_attach_window()` call                      |
| `crates/kild/src/commands/open.rs`                   | 7,36-43 | UPDATE | Remove `spawn_attach_window()` calls (single + --all)    |

### Integration Points

- `crates/kild/src/commands/create.rs:93` calls `create_session()` — CLI will no longer need to spawn attach window
- `crates/kild/src/commands/open.rs:34,113` calls `open_session()` — same for single and `--all` paths
- `crates/kild-core/src/sessions/destroy.rs:152-210` iterates agents for cleanup — daemon path needs terminal close
- `crates/kild-core/src/sessions/complete.rs:167` delegates to `destroy_session()` — inherits fix automatically
- `terminal::handler::close_terminal()` at `handler.rs:346` — existing API, fire-and-forget

### Git History

- **Introduced**: `cc77f9e` - "feat: auto-open Ghostty attach window for CLI daemon sessions (#454)"
- **Implication**: Original bug from when attach windows were first introduced. Terminal info was intentionally omitted because the attach window was considered "ephemeral," but cleanup was overlooked.

---

## Implementation Plan

### Step 1: Change `spawn_attach_window()` return type

**File**: `crates/kild-core/src/sessions/daemon_helpers.rs`
**Lines**: 65-116
**Action**: UPDATE

**Current code:**
```rust
pub fn spawn_attach_window(
    branch: &str,
    spawn_id: &str,
    worktree_path: &Path,
    kild_config: &KildConfig,
) {
    // ...
    match terminal::handler::spawn_terminal(...) {
        Ok(result) => {
            info!(...);
        }
        Err(e) => {
            warn!(...);
        }
    }
}
```

**Required change:**
```rust
pub fn spawn_attach_window(
    branch: &str,
    spawn_id: &str,
    worktree_path: &Path,
    kild_config: &KildConfig,
) -> Option<(TerminalType, String)> {
    // ...
    match terminal::handler::spawn_terminal(...) {
        Ok(result) => {
            info!(...);
            // Return terminal info for storage in AgentProcess
            result.terminal_window_id.map(|wid| (result.terminal_type, wid))
        }
        Err(e) => {
            warn!(...);
            None
        }
    }
}
```

Also update the early return for binary resolution failure to return `None`.

**Why**: The terminal type and window ID must be captured so they can be stored in the `AgentProcess` for cleanup during destroy.

---

### Step 2: Call `spawn_attach_window()` in `create_session()` daemon path

**File**: `crates/kild-core/src/sessions/create.rs`
**Lines**: 400-413
**Action**: UPDATE

**Current code:**
```rust
AgentProcess::new(
    validated.agent.clone(),
    spawn_id,
    None,
    None,
    None,
    None,  // terminal_type
    None,  // terminal_window_id
    validated.command.clone(),
    now.clone(),
    Some(daemon_result.daemon_session_id),
)?
```

**Required change:**
```rust
// Spawn attach window (best-effort) and capture terminal info
let attach_info = spawn_attach_window(
    &validated.name,
    &spawn_id,
    &worktree.path,
    kild_config,
);

AgentProcess::new(
    validated.agent.clone(),
    spawn_id,
    None,
    None,
    None,
    attach_info.as_ref().map(|(tt, _)| tt.clone()),
    attach_info.map(|(_, wid)| wid),
    validated.command.clone(),
    now.clone(),
    Some(daemon_result.daemon_session_id),
)?
```

**Why**: By spawning the attach window inside `create_session()` before constructing `AgentProcess`, the terminal info is available and stored in the session file.

---

### Step 3: Call `spawn_attach_window()` in `open_session()` daemon path

**File**: `crates/kild-core/src/sessions/open.rs`
**Lines**: 384-396
**Action**: UPDATE

**Current code:**
```rust
AgentProcess::new(
    agent.clone(),
    spawn_id,
    None,
    None,
    None,
    None,  // terminal_type
    None,  // terminal_window_id
    agent_command.clone(),
    now.clone(),
    Some(daemon_result.daemon_session_id),
)?
```

**Required change:**
```rust
// Spawn attach window (best-effort) and capture terminal info
let attach_info = spawn_attach_window(
    name,
    &spawn_id,
    &session.worktree_path,
    &kild_config,
);

AgentProcess::new(
    agent.clone(),
    spawn_id,
    None,
    None,
    None,
    attach_info.as_ref().map(|(tt, _)| tt.clone()),
    attach_info.map(|(_, wid)| wid),
    agent_command.clone(),
    now.clone(),
    Some(daemon_result.daemon_session_id),
)?
```

**Why**: Same rationale as Step 2 — capture attach window info at construction time.

---

### Step 4: Close terminal window in daemon destroy path

**File**: `crates/kild-core/src/sessions/destroy.rs`
**Lines**: 166-183
**Action**: UPDATE

**Current code:**
```rust
if let Some(daemon_sid) = agent_proc.daemon_session_id() {
    // Daemon-managed: destroy via IPC
    info!(...);
    if let Err(e) = crate::daemon::client::destroy_daemon_session(daemon_sid, force) {
        warn!(...);
    }
} else {
    // Terminal-managed: close window + kill process
    if let (Some(terminal_type), Some(window_id)) = ... {
        terminal::handler::close_terminal(terminal_type, Some(window_id));
    }
    // ... kill process
}
```

**Required change:**
```rust
if let Some(daemon_sid) = agent_proc.daemon_session_id() {
    // Daemon-managed: destroy via IPC
    info!(...);
    if let Err(e) = crate::daemon::client::destroy_daemon_session(daemon_sid, force) {
        warn!(...);
    }
    // Close the attach terminal window (if tracked)
    if let (Some(terminal_type), Some(window_id)) =
        (agent_proc.terminal_type(), agent_proc.terminal_window_id())
    {
        info!(
            event = "core.session.destroy_close_attach_window",
            terminal_type = ?terminal_type,
            agent = agent_proc.agent(),
        );
        terminal::handler::close_terminal(terminal_type, Some(window_id));
    }
} else {
    // Terminal-managed: close window + kill process (unchanged)
    ...
}
```

**Why**: Daemon agents now have terminal_type and window_id stored. After destroying the PTY, close the attach window using the same fire-and-forget mechanism.

---

### Step 5: Remove CLI-level `spawn_attach_window()` calls

**File**: `crates/kild/src/commands/create.rs`
**Lines**: 95-101
**Action**: UPDATE

**Current code:**
```rust
// Auto-attach: open a terminal window for daemon sessions
if session.runtime_mode == Some(kild_core::RuntimeMode::Daemon) {
    let spawn_id = session
        .latest_agent()
        .map(|a| a.spawn_id().to_string())
        .unwrap_or_default();
    spawn_attach_window(&session.branch, &spawn_id, &session.worktree_path, &config);
}
```

**Required change:** Remove the entire block (lines 95-102). Also remove the `spawn_attach_window` import if unused.

**Why**: Attach window spawning now happens inside `create_session()`.

---

### Step 6: Remove CLI-level `spawn_attach_window()` calls from open.rs

**File**: `crates/kild/src/commands/open.rs`
**Lines**: 7, 36-43, 121-128
**Action**: UPDATE

Remove:
1. Line 7: `use kild_core::sessions::daemon_helpers::spawn_attach_window;` import
2. Lines 36-44: Single-branch auto-attach block
3. Lines 121-128: `--all` path auto-attach block

**Why**: Attach window spawning now happens inside `open_session()`.

---

## Patterns to Follow

**From codebase — terminal cleanup in destroy (already exists):**
```rust
// SOURCE: crates/kild-core/src/sessions/destroy.rs:184-193
// Pattern for closing terminal windows during destroy
if let (Some(terminal_type), Some(window_id)) =
    (agent_proc.terminal_type(), agent_proc.terminal_window_id())
{
    info!(
        event = "core.session.destroy_close_terminal",
        terminal_type = ?terminal_type,
        agent = agent_proc.agent(),
    );
    terminal::handler::close_terminal(terminal_type, Some(window_id));
}
```

**From codebase — terminal info storage in AgentProcess (terminal path):**
```rust
// SOURCE: crates/kild-core/src/sessions/create.rs:238-239
// Pattern for passing terminal info to AgentProcess
Some(spawn_result.terminal_type.clone()),      // terminal_type
spawn_result.terminal_window_id.clone(),        // terminal_window_id
```

---

## Edge Cases & Risks

| Risk/Edge Case                                | Mitigation                                                                                                                           |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Attach window spawn fails                     | `spawn_attach_window()` returns `None`, AgentProcess gets `None, None` for terminal fields — same as current behavior. No regression. |
| Window closed before destroy                  | `close_terminal()` is fire-and-forget, handles missing windows gracefully                                                            |
| Multiple `kild open` on same session          | Each open spawns a new agent+attach window pair. Each `AgentProcess` tracks its own window. All cleaned up on destroy.                |
| `kild open --all` with mixed daemon/terminal  | Each session handled independently. Daemon sessions get attach windows, terminal sessions don't.                                     |
| Legacy sessions without terminal_window_id    | `None` fields cause the cleanup to skip gracefully — no regression                                                                   |
| Attach window spawned slightly earlier (timing)| Window opens during `create_session()` instead of after. Negligible UX difference — user sees window sooner.                         |

---

## Validation

### Automated Checks

```bash
cargo fmt --check && cargo clippy --all -- -D warnings && cargo test --all && cargo build --all
```

### Manual Verification

1. `kild create test-branch --daemon` — verify Ghostty window opens with `kild attach`
2. `kild destroy test-branch` — verify Ghostty window closes automatically
3. `kild create test-branch --daemon` → `kild complete test-branch` — verify window closes
4. `kild create test-branch` (terminal mode) — verify existing behavior unchanged
5. `kild create test-branch --daemon` → close Ghostty manually → `kild destroy test-branch` — verify no error

---

## Scope Boundaries

**IN SCOPE:**
- Storing attach window terminal info in daemon `AgentProcess`
- Closing attach window during daemon destroy path
- Moving `spawn_attach_window()` calls from CLI into kild-core

**OUT OF SCOPE (do not touch):**
- Terminal backend implementations
- Daemon PTY management
- Session persistence format (fields already exist, just set to None)
- Complete/destroy safety checks
- `kild attach` command itself

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-17T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-465.md`
