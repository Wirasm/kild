# Investigation: kild open overwrites PID tracking, orphaning previous agents

**Issue**: #217 (https://github.com/Wirasm/kild/issues/217)
**Type**: BUG
**Investigated**: 2026-02-05

### Assessment

| Metric     | Value    | Reasoning                                                                                                                                   |
| ---------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Severity   | HIGH     | Multiple agents orphaned as untrackable processes; stop/destroy only kills the last-opened agent, leaving others running with no cleanup path |
| Complexity | HIGH     | 8+ files across 3 crates (kild-core, kild, kild-ui); touches Session struct, all lifecycle handlers, CLI display, UI display, health, state |
| Confidence | HIGH     | Root cause is clear from code: direct field assignment on singular `Option<T>` fields in `open_session` at handler.rs:1039-1051              |

---

## Problem Statement

When `kild open` is called multiple times on the same session, `open_session` overwrites singular process tracking fields (`process_id`, `process_name`, `process_start_time`, `terminal_window_id`, `agent`, `command`) with the new spawn's data. The previously spawned agent continues running but becomes an orphaned process with no way to stop, track, or clean up via kild.

---

## Analysis

### Root Cause

WHY: `kild stop` and `kild destroy` only kill the last-opened agent
| BECAUSE: They read `session.process_id` which is a single `Option<u32>`
| Evidence: `handler.rs:1097` - `if let Some(pid) = session.process_id`

| BECAUSE: `open_session` overwrites that field with the newest spawn result
| Evidence: `handler.rs:1039` - `session.process_id = spawn_result.process_id;`

| ROOT CAUSE: The `Session` struct stores process tracking as singular fields, not a collection
| Evidence: `types.rs:198` - `pub process_id: Option<u32>`

### Evidence Chain

**Overwrite in open_session** (`handler.rs:1039-1051`):
```rust
session.process_id = spawn_result.process_id;           // overwrites
session.process_name = process_name;                     // overwrites
session.process_start_time = process_start_time;         // overwrites
session.terminal_type = Some(spawn_result.terminal_type.clone());
session.terminal_window_id = spawn_result.terminal_window_id.clone(); // overwrites
session.command = ...;                                   // overwrites
session.agent = agent.clone();                           // overwrites
```

**Single-PID kill in stop_session** (`handler.rs:1087-1123`):
```rust
// Only closes ONE terminal window
if let Some(ref terminal_type) = session.terminal_type {
    terminal::handler::close_terminal(terminal_type, session.terminal_window_id.as_deref());
}
// Only kills ONE process
if let Some(pid) = session.process_id {
    crate::process::kill_process(pid, session.process_name.as_deref(), session.process_start_time)?;
}
```

**Single-PID kill in destroy_session** (`handler.rs:286-334`): Same pattern.

### Affected Files

| File                                             | Lines     | Action | Description                                                        |
| ------------------------------------------------ | --------- | ------ | ------------------------------------------------------------------ |
| `crates/kild-core/src/sessions/types.rs`         | 173-246   | UPDATE | Add `AgentProcess` struct, add `agents` field to `Session`         |
| `crates/kild-core/src/sessions/handler.rs`       | 1039-1051 | UPDATE | `open_session`: push to `agents` vec instead of overwriting        |
| `crates/kild-core/src/sessions/handler.rs`       | 1068-1161 | UPDATE | `stop_session`: iterate all agents, kill all processes             |
| `crates/kild-core/src/sessions/handler.rs`       | 259-389   | UPDATE | `destroy_session`: iterate all agents, kill all processes          |
| `crates/kild-core/src/sessions/handler.rs`       | 130-191   | UPDATE | `create_session`: initialize `agents` vec with first agent         |
| `crates/kild-core/src/sessions/info.rs`          | 72-115    | UPDATE | `determine_process_status`: check all agent PIDs                   |
| `crates/kild-core/src/health/handler.rs`         | 62-97     | UPDATE | `enrich_session_with_metrics`: check all agent PIDs                |
| `crates/kild/src/commands.rs`                    | 555-591   | UPDATE | `handle_open_command`: display new agent info from returned session |
| `crates/kild/src/commands.rs`                    | 670-697   | UPDATE | `handle_stop_command`: display count of stopped agents             |
| `crates/kild/src/commands.rs`                    | 1068-1156 | UPDATE | `handle_status_command`: display all agents                        |
| `crates/kild/src/commands.rs`                    | 845-897   | UPDATE | `handle_focus_command`: focus most recent agent's window (or all)  |
| `crates/kild/src/table.rs`                       | 53-91     | UPDATE | `print_row`: show agent count / primary agent info                 |
| `crates/kild-core/src/state/events.rs`           | 13-34     | UPDATE | `KildOpened` event: add agent name field                           |
| `crates/kild-ui/src/views/detail_panel.rs`       | 20-60     | UPDATE | Display all agents with their statuses                             |
| `crates/kild-ui/src/views/kild_list.rs`          | 176       | UPDATE | Use agents vec for focus window ID                                 |
| `crates/kild-ui/src/state/app_state.rs`          | multiple  | UPDATE | Test helpers: update Session construction with agents vec          |
| `crates/kild-ui/src/state/sessions.rs`           | multiple  | UPDATE | Test helpers: update Session construction with agents vec          |

### Integration Points

- `crates/kild-core/src/sessions/persistence.rs:26-55` - `save_session_to_file` serializes Session to JSON (no change needed, serde handles Vec automatically)
- `crates/kild-core/src/sessions/persistence.rs:57-109` - `load_sessions_from_files` deserializes from JSON (no change needed with `#[serde(default)]`)
- `crates/kild-core/src/state/dispatch.rs:56-59` - `CoreStore::dispatch` for `OpenKild` command (minimal change - pass through)
- `crates/kild-core/src/terminal/types.rs:25-39` - `SpawnResult` struct (no change - represents single spawn)
- `crates/kild-core/src/process/operations.rs:68-112` - `kill_process` (no change - called per agent)

### Git History

- **Introduced**: `7b8dcdbd` (2026-01-24) - Original open_session implementation during Shards-to-KILD rebrand
- **Nature**: Original design bug - singular fields were never designed for multi-agent tracking
- **Implication**: Long-standing since open_session was first implemented

---

## Implementation Plan

### Step 1: Add `AgentProcess` struct to Session types

**File**: `crates/kild-core/src/sessions/types.rs`
**Action**: UPDATE

**Add new struct after `Session` definition (after line 246):**

```rust
/// Represents a single agent process spawned within a kild session.
///
/// Multiple agents can run concurrently in the same kild via `kild open`.
/// Each open operation appends an `AgentProcess` to the session's `agents` vec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentProcess {
    pub agent: String,
    pub process_id: Option<u32>,
    pub process_name: Option<String>,
    pub process_start_time: Option<u64>,
    pub terminal_type: Option<TerminalType>,
    pub terminal_window_id: Option<String>,
    pub command: String,
    pub opened_at: String,
}
```

**Add `agents` field to `Session` struct (after `note` field, line 245):**

```rust
    /// All agent processes opened in this kild session.
    ///
    /// Populated by `kild open`. Each open operation appends an entry.
    /// `kild stop` clears this vec. `kild create` initializes with one entry.
    ///
    /// Empty for sessions created before multi-agent tracking was added.
    #[serde(default)]
    pub agents: Vec<AgentProcess>,
```

**Why**: The core data model change. `Vec<AgentProcess>` tracks all spawned agents. The existing singular fields (`process_id`, `agent`, `command`, etc.) remain for backward compatibility during transition and serve as the "primary" agent (the one from `create`). The `agents` vec tracks all agents from `open` calls.

**Backward compatibility**: `#[serde(default)]` means old session JSON files without `agents` will deserialize with an empty vec. Since sessions are ephemeral (destroyed or completed, never archived), this is sufficient.

---

### Step 2: Update `create_session` to initialize agents vec

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 166-191

**Current code (Session construction):**
```rust
let session = Session {
    id: session_id.clone(),
    // ... all fields ...
    note: request.note.clone(),
};
```

**Required change**: Add `agents: vec![]` to the Session construction. The initial agent from `create` is already tracked in the singular fields. The `agents` vec is for *additional* agents from `open`.

Actually, for consistency and to make stop/destroy logic uniform, the `agents` vec should include the initial agent too:

```rust
let now = chrono::Utc::now().to_rfc3339();
let initial_agent = AgentProcess {
    agent: validated.agent.clone(),
    process_id: spawn_result.process_id,
    process_name: spawn_result.process_name.clone(),
    process_start_time: spawn_result.process_start_time,
    terminal_type: Some(spawn_result.terminal_type.clone()),
    terminal_window_id: spawn_result.terminal_window_id.clone(),
    command: if spawn_result.command_executed.trim().is_empty() {
        format!("{} (command not captured)", validated.agent)
    } else {
        spawn_result.command_executed.clone()
    },
    opened_at: now.clone(),
};
let session = Session {
    // ... existing fields unchanged ...
    agents: vec![initial_agent],
};
```

**Why**: Uniform handling - stop/destroy iterate `agents` vec for all cleanup. No special-casing needed for the "first" agent.

---

### Step 3: Update `open_session` to append to agents vec

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 1039-1051

**Current code:**
```rust
session.process_id = spawn_result.process_id;
session.process_name = process_name;
session.process_start_time = process_start_time;
session.terminal_type = Some(spawn_result.terminal_type.clone());
session.terminal_window_id = spawn_result.terminal_window_id.clone();
session.command = if spawn_result.command_executed.trim().is_empty() {
    format!("{} (command not captured)", agent)
} else {
    spawn_result.command_executed.clone()
};
session.agent = agent.clone();
session.status = SessionStatus::Active;
session.last_activity = Some(chrono::Utc::now().to_rfc3339());
```

**Required change:**
```rust
let now = chrono::Utc::now().to_rfc3339();
let new_agent = AgentProcess {
    agent: agent.clone(),
    process_id: spawn_result.process_id,
    process_name: process_name,
    process_start_time: process_start_time,
    terminal_type: Some(spawn_result.terminal_type.clone()),
    terminal_window_id: spawn_result.terminal_window_id.clone(),
    command: if spawn_result.command_executed.trim().is_empty() {
        format!("{} (command not captured)", agent)
    } else {
        spawn_result.command_executed.clone()
    },
    opened_at: now.clone(),
};

// Keep singular fields updated to latest for backward compat and "primary" display
session.process_id = spawn_result.process_id;
session.process_name = new_agent.process_name.clone();
session.process_start_time = new_agent.process_start_time;
session.terminal_type = Some(spawn_result.terminal_type.clone());
session.terminal_window_id = spawn_result.terminal_window_id.clone();
session.command = new_agent.command.clone();
session.agent = agent.clone();
session.status = SessionStatus::Active;
session.last_activity = Some(now);

// Track all agents for proper cleanup
session.agents.push(new_agent);
```

**Why**: Appends to `agents` vec instead of only overwriting. Singular fields still updated for backward compat with any code that reads them directly.

---

### Step 4: Update `stop_session` to kill all agents

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 1087-1150

**Current code kills one terminal and one process.**

**Required change**: Iterate all agents in the vec, kill each one:

```rust
// Close all terminal windows (fire-and-forget, best-effort)
for agent_proc in &session.agents {
    if let (Some(ref terminal_type), Some(ref window_id)) =
        (&agent_proc.terminal_type, &agent_proc.terminal_window_id)
    {
        info!(
            event = "core.session.stop_close_terminal",
            terminal_type = ?terminal_type,
            agent = agent_proc.agent,
        );
        terminal::handler::close_terminal(terminal_type, Some(window_id.as_str()));
    }
}

// Kill all tracked processes (blocking, handle errors)
let mut kill_errors: Vec<(u32, String)> = Vec::new();
for agent_proc in &session.agents {
    if let Some(pid) = agent_proc.process_id {
        info!(event = "core.session.stop_kill_started", pid = pid, agent = agent_proc.agent);
        match crate::process::kill_process(
            pid,
            agent_proc.process_name.as_deref(),
            agent_proc.process_start_time,
        ) {
            Ok(()) => {
                info!(event = "core.session.stop_kill_completed", pid = pid);
            }
            Err(crate::process::ProcessError::NotFound { .. }) => {
                info!(event = "core.session.stop_kill_already_dead", pid = pid);
            }
            Err(e) => {
                error!(event = "core.session.stop_kill_failed", pid = pid, error = %e);
                kill_errors.push((pid, e.to_string()));
            }
        }
    }
}

// Fail if any kill failed (report first error)
if let Some((pid, message)) = kill_errors.into_iter().next() {
    return Err(SessionError::ProcessKillFailed { pid, message });
}
```

Then clear agents vec and singular fields:

```rust
session.agents.clear();
session.process_id = None;
session.process_name = None;
session.process_start_time = None;
session.status = SessionStatus::Stopped;
session.last_activity = Some(chrono::Utc::now().to_rfc3339());
```

**Fallback**: Also keep the existing singular-field kill logic as a fallback for old sessions with empty `agents` vec but populated singular fields. Check `if session.agents.is_empty()` and fall back to singular field logic.

**Why**: All agents must be stopped, not just the last one.

---

### Step 5: Update `destroy_session` to kill all agents

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 286-334

Same pattern as Step 4. Iterate `session.agents` to close all terminal windows and kill all processes before removing the worktree.

**Fallback**: Same as Step 4 - fall back to singular fields if `agents` is empty.

**Why**: Destroy must clean up all processes to avoid orphans.

---

### Step 6: Update `determine_process_status` for multi-agent

**File**: `crates/kild-core/src/sessions/info.rs`
**Lines**: 72-115

**Current code** checks `session.process_id` (singular).

**Required change**: Check all agents. Session is "Running" if *any* agent is running:

```rust
pub fn determine_process_status(session: &Session) -> ProcessStatus {
    // Check agents vec first (new multi-agent path)
    if !session.agents.is_empty() {
        let mut any_running = false;
        let mut any_unknown = false;
        for agent_proc in &session.agents {
            if let Some(pid) = agent_proc.process_id {
                match is_process_running(pid) {
                    Ok(true) => { any_running = true; }
                    Ok(false) => {}
                    Err(e) => {
                        tracing::warn!(
                            event = "core.session.process_check_failed",
                            pid = pid, agent = agent_proc.agent, error = %e
                        );
                        any_unknown = true;
                    }
                }
            } else if let (Some(terminal_type), Some(window_id)) =
                (&agent_proc.terminal_type, &agent_proc.terminal_window_id)
            {
                match is_terminal_window_open(terminal_type, window_id) {
                    Ok(Some(true)) => { any_running = true; }
                    Ok(Some(false) | None) => {}
                    Err(_) => {}
                }
            }
        }
        if any_running { return ProcessStatus::Running; }
        if any_unknown { return ProcessStatus::Unknown; }
        return ProcessStatus::Stopped;
    }

    // Fallback: existing singular-field logic for old sessions
    // ... (keep existing code)
}
```

**Why**: Health status must reflect all agents, not just the last one.

---

### Step 7: Update health monitoring

**File**: `crates/kild-core/src/health/handler.rs`
**Lines**: 62-97

**Required change**: `enrich_session_with_metrics` should check all agent PIDs. If any agent has metrics, aggregate or pick the highest resource consumer. For simplicity, check the first running agent's metrics:

```rust
fn enrich_session_with_metrics(session: &sessions::types::Session) -> KildHealth {
    // Find first running agent for metrics
    let running_pid = session.agents.iter()
        .filter_map(|a| a.process_id)
        .find(|&pid| matches!(process::is_process_running(pid), Ok(true)));

    let (process_metrics, process_running) = if let Some(pid) = running_pid {
        let metrics = process::get_process_metrics(pid).ok();
        (metrics, true)
    } else if let Some(pid) = session.process_id {
        // Fallback to singular field for old sessions
        match process::is_process_running(pid) {
            Ok(true) => (process::get_process_metrics(pid).ok(), true),
            _ => (None, false),
        }
    } else {
        (None, false)
    };

    operations::enrich_session_with_health(session, process_metrics, process_running)
}
```

**Why**: Health must detect running agents from the vec, not just the singular field.

---

### Step 8: Update CLI status display

**File**: `crates/kild/src/commands.rs`
**Lines**: 1068-1156

**Required change**: In `handle_status_command`, display all agents:

```rust
// Display agents section
if !session.agents.is_empty() {
    println!("│ Agents:      {:<47} │", format!("{} agent(s)", session.agents.len()));
    for (i, agent_proc) in session.agents.iter().enumerate() {
        let status = agent_proc.process_id.map_or("No PID".to_string(), |pid| {
            match process::is_process_running(pid) {
                Ok(true) => format!("Running (PID: {})", pid),
                Ok(false) => format!("Stopped (PID: {})", pid),
                Err(_) => format!("Error (PID: {})", pid),
            }
        });
        println!("│   {}. {:<6} {:<38} │", i + 1, agent_proc.agent, status);
    }
} else {
    // Fallback: existing singular field display
    println!("│ Agent:       {:<47} │", session.agent);
    // ... existing PID check ...
}
```

**Why**: Status must show all tracked agents.

---

### Step 9: Update table formatter

**File**: `crates/kild/src/table.rs`
**Lines**: 53-91

**Required change**: Show agent count if multiple, or primary agent if single:

```rust
let agent_display = if session.agents.len() > 1 {
    format!("{} (+{})", session.agents.last().map_or(&session.agent, |a| &a.agent), session.agents.len() - 1)
} else {
    session.agent.clone()
};

let process_status = if !session.agents.is_empty() {
    let running = session.agents.iter()
        .filter(|a| a.process_id.map_or(false, |pid| {
            matches!(kild_core::process::is_process_running(pid), Ok(true))
        }))
        .count();
    let total = session.agents.len();
    if running == total { format!("Run({}/{})", running, total) }
    else if running == 0 { format!("Stop(0/{})", total) }
    else { format!("Run({}/{})", running, total) }
} else {
    // Fallback: existing singular field logic
    session.process_id.map_or("No PID".to_string(), |pid| { /* existing */ })
};
```

**Why**: List view must indicate multi-agent status at a glance.

---

### Step 10: Update focus command

**File**: `crates/kild/src/commands.rs`
**Lines**: 845-897

**Required change**: Focus the most recently opened agent's terminal window. Use `session.agents.last()` to get the latest:

```rust
let (terminal_type, window_id) = if let Some(latest) = session.agents.last() {
    (
        latest.terminal_type.as_ref(),
        latest.terminal_window_id.as_ref(),
    )
} else {
    // Fallback to singular fields
    (session.terminal_type.as_ref(), session.terminal_window_id.as_ref())
};

let terminal_type = terminal_type.ok_or_else(|| {
    eprintln!("No terminal type recorded for kild '{}'", branch);
    "No terminal type recorded for this kild"
})?;
let window_id = window_id.ok_or_else(|| {
    eprintln!("No window ID recorded for kild '{}'", branch);
    "No window ID recorded for this kild"
})?;
```

**Why**: Focus should target the most recent agent window by default.

---

### Step 11: Update KildOpened event

**File**: `crates/kild-core/src/state/events.rs`
**Lines**: 20

**Current:**
```rust
KildOpened { branch: String },
```

**Required change:**
```rust
KildOpened { branch: String, agent: String },
```

Update dispatch.rs accordingly to pass agent name through.

**Why**: UI needs to know which agent was opened for display purposes.

---

### Step 12: Update UI detail panel and list

**File**: `crates/kild-ui/src/views/detail_panel.rs`
**Lines**: 20-60

**Required change**: Display agent list instead of singular agent. Show each agent's name and status. For focus, use the most recent agent's window ID.

**File**: `crates/kild-ui/src/views/kild_list.rs`
**Line**: 176

**Required change**: Use `session.agents.last()` for focus window ID, falling back to `session.terminal_window_id`.

---

### Step 13: Update all test helpers constructing Session

**Files**:
- `crates/kild-ui/src/state/app_state.rs` (8+ test Session constructions)
- `crates/kild-ui/src/state/sessions.rs` (8+ test Session constructions)
- `crates/kild-ui/src/actions.rs` (1 test Session construction)
- `crates/kild-ui/src/views/kild_list.rs` (1 test Session construction)
- `crates/kild-core/src/sessions/types.rs` (test functions)
- `crates/kild-core/src/sessions/handler.rs` (test functions)
- `crates/kild-core/src/sessions/persistence.rs` (test functions)
- `crates/kild-core/src/sessions/validation.rs` (test functions)

**Required change**: Add `agents: vec![]` to all Session struct literals in tests.

**Why**: New field must be present in all Session constructions. Using empty vec is correct for tests that don't exercise multi-agent behavior.

---

### Step 14: Add new tests

**File**: `crates/kild-core/src/sessions/types.rs`
**Action**: UPDATE (add tests)

```rust
#[test]
fn test_agent_process_serialization_roundtrip() {
    let agent = AgentProcess {
        agent: "claude".to_string(),
        process_id: Some(12345),
        process_name: Some("claude-code".to_string()),
        process_start_time: Some(1705318200),
        terminal_type: Some(TerminalType::Ghostty),
        terminal_window_id: Some("kild-test".to_string()),
        command: "claude-code".to_string(),
        opened_at: "2024-01-15T10:30:00Z".to_string(),
    };
    let json = serde_json::to_string(&agent).unwrap();
    let deserialized: AgentProcess = serde_json::from_str(&json).unwrap();
    assert_eq!(agent, deserialized);
}

#[test]
fn test_session_with_agents_backward_compat() {
    // Old session JSON without "agents" field should deserialize with empty vec
    let json = r#"{
        "id": "test",
        "project_id": "test-project",
        "branch": "test-branch",
        "worktree_path": "/tmp/test",
        "agent": "claude",
        "status": "Active",
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let session: Session = serde_json::from_str(json).unwrap();
    assert!(session.agents.is_empty());
}

#[test]
fn test_session_with_multiple_agents_serialization() {
    let session = Session {
        // ... fields ...
        agents: vec![
            AgentProcess { agent: "claude".to_string(), /* ... */ },
            AgentProcess { agent: "kiro".to_string(), /* ... */ },
        ],
    };
    let json = serde_json::to_string_pretty(&session).unwrap();
    let deserialized: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.agents.len(), 2);
    assert_eq!(deserialized.agents[0].agent, "claude");
    assert_eq!(deserialized.agents[1].agent, "kiro");
}
```

---

## Patterns to Follow

**From codebase - backward compat with serde default:**
```rust
// SOURCE: types.rs:244-245
#[serde(default)]
pub note: Option<String>,
```

**From codebase - fire-and-forget terminal close:**
```rust
// SOURCE: handler.rs:287-294
if let Some(ref terminal_type) = session.terminal_type {
    terminal::handler::close_terminal(terminal_type, session.terminal_window_id.as_deref());
}
```

**From codebase - process kill with validation:**
```rust
// SOURCE: handler.rs:296-310
if let Some(pid) = session.process_id {
    match crate::process::kill_process(pid, session.process_name.as_deref(), session.process_start_time) {
        Ok(()) => { info!(event = "core.session.destroy_kill_completed", pid = pid); }
        Err(crate::process::ProcessError::NotFound { .. }) => {
            info!(event = "core.session.destroy_kill_already_dead", pid = pid);
        }
        Err(e) => { /* handle */ }
    }
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                                    | Mitigation                                                                                              |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Old sessions with empty `agents` vec              | Fallback to singular fields in all read paths (`if session.agents.is_empty()`)                          |
| `kild open` while previous agent already stopped  | Append anyway - stop/destroy skips `ProcessError::NotFound` gracefully                                  |
| Kill error on one agent blocks stop of others     | Collect all kill errors, attempt all kills, then report errors                                          |
| `kild focus` with multiple windows                | Focus most recent agent's window (`.last()`)                                                            |
| JSON output changes (breaking for scripts)        | `agents` field is additive - existing fields unchanged, scripts using singular fields continue to work  |
| UI displaying many agents                         | Show compact list with agent name + status indicator; detail panel shows full info                       |
| Singular fields become stale/misleading           | Keep them updated to latest agent for backward compat; document that `agents` is the source of truth    |

---

## Validation

### Automated Checks

```bash
cargo fmt --check && cargo clippy --all -- -D warnings && cargo test --all && cargo build --all
```

### Manual Verification

1. `kild create test-branch` - verify session JSON has `agents` vec with one entry
2. `kild open test-branch --agent kiro` - verify `agents` vec has two entries in JSON
3. `kild status test-branch` - verify both agents displayed with PID status
4. `kild list` - verify agent count shown (e.g., "claude (+1)")
5. `kild stop test-branch` - verify both processes killed, both terminal windows closed
6. `kild open test-branch` - verify re-open after stop works, agents vec repopulated
7. `kild destroy test-branch` - verify all cleanup occurs (no orphans)
8. Test with old session JSON (no `agents` field) - verify backward compat (no crash, singular field fallback works)

---

## Scope Boundaries

**IN SCOPE:**
- `AgentProcess` struct and `agents` Vec on Session
- All lifecycle handlers (create, open, stop, destroy, complete) updated for multi-agent
- Process status detection for all agents
- CLI display (status, list, focus) for multi-agent
- UI display for multi-agent
- Health monitoring for multi-agent
- Backward compatibility with old session JSON
- State events updated with agent info

**OUT OF SCOPE (do not touch):**
- `SpawnResult` struct (stays single-spawn, called once per open)
- `kill_process` function (stays single-PID, called in a loop)
- Terminal backend trait (stays single-window operations)
- Port allocation (ports are per-session, not per-agent)
- `restart_session` handler (deprecated/legacy - can be updated separately)
- Agent selection/targeting (e.g., "stop only the claude agent") - future enhancement
- Per-agent port ranges - future enhancement if needed

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-05
- **Artifact**: `.claude/PRPs/issues/issue-217.md`
