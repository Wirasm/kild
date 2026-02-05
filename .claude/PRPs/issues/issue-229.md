# Investigation: Remove singular process-tracking fields from Session

**Issue**: #229 (https://github.com/Wirasm/kild/issues/229)
**Type**: REFACTOR
**Investigated**: 2026-02-05T10:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                    |
| ---------- | ------ | ------------------------------------------------------------------------------------------------------------ |
| Priority   | HIGH   | Dual storage is active tech debt from #228; every new feature touching sessions must maintain two code paths. |
| Complexity | HIGH   | 9+ files across 3 crates, ~15 fallback sites, ~30+ test fixtures to update.                                  |
| Confidence | HIGH   | All dual-write/fallback sites are clearly marked with comments; the issue description maps exactly to code.   |

---

## Problem Statement

`Session` maintains 6 singular process-tracking fields (`process_id`, `process_name`, `process_start_time`, `terminal_type`, `terminal_window_id`, `command`) alongside `agents: Vec<AgentProcess>`. Every write path dual-writes to both, and every read path checks `has_agents()` then falls back to singular fields. This violates the project's "no backward-compatibility shims" principle and doubles the maintenance surface for all session-related code.

---

## Analysis

### Change Rationale

The singular fields were kept during #228 for backward compatibility with old session JSON files. However:
1. `#[serde(default)]` on the `agents` field already handles old files (deserializes as empty vec = "no running agents")
2. Old sessions without `agents` are legitimately in a "stopped" state (no process to track)
3. The project has no external consumers — there's nobody to keep compatible with

### Important: Keep `agent` field

The `agent` field on `Session` is **not** a process-tracking field. It represents the *primary agent name* set at create time and is used:
- As the default agent for `open`/`restart` (`agent_override.unwrap_or(session.agent.clone())` at `handler.rs:1024,1167`)
- For display in simple list views (`table.rs:114`, `kild_list.rs:269`)
- For validation (`validation.rs:55`)
- For event logging (`handler.rs:281,961,1110`)
- In health reporting (`operations.rs:88`)
- In state dispatch (`dispatch.rs:60`)

This field is distinct from per-process agent tracking in `AgentProcess` and must be retained.

### Affected Files

| File                                            | Lines     | Action | Description                                                |
| ----------------------------------------------- | --------- | ------ | ---------------------------------------------------------- |
| `crates/kild-core/src/sessions/types.rs`        | 189-228   | UPDATE | Remove 6 singular fields from Session struct               |
| `crates/kild-core/src/sessions/handler.rs`      | 264-269   | UPDATE | Remove singular field writes in create_session             |
| `crates/kild-core/src/sessions/handler.rs`      | 1092-1097 | UPDATE | Remove singular field writes in restart_session            |
| `crates/kild-core/src/sessions/handler.rs`      | 1228-1235 | UPDATE | Remove dual-write in open_session                          |
| `crates/kild-core/src/sessions/handler.rs`      | 1389-1392 | UPDATE | Remove singular field clears in stop_session               |
| `crates/kild-core/src/sessions/handler.rs`      | 451-495   | UPDATE | Remove fallback branch in destroy_session                  |
| `crates/kild-core/src/sessions/handler.rs`      | 1351-1383 | UPDATE | Remove fallback branch in stop_session                     |
| `crates/kild-core/src/sessions/handler.rs`      | 277-283   | UPDATE | Update create_session logging to use agents vec            |
| `crates/kild-core/src/sessions/handler.rs`      | 960-966   | UPDATE | Update restart_session read paths                          |
| `crates/kild-core/src/sessions/info.rs`         | 134-168   | UPDATE | Remove fallback branch in determine_process_status         |
| `crates/kild-core/src/health/handler.rs`        | 74-91     | UPDATE | Remove fallback branch in enrich_session_with_metrics      |
| `crates/kild/src/commands.rs`                   | 537,574   | UPDATE | Remove singular process_id display in list/destroy output  |
| `crates/kild/src/commands.rs`                   | 864-874   | UPDATE | Remove fallback in handle_focus_command                    |
| `crates/kild/src/commands.rs`                   | 1119-1168 | UPDATE | Remove fallback in handle_status_command                   |
| `crates/kild/src/table.rs`                      | 82-97     | UPDATE | Remove fallback process status display                     |
| `crates/kild/src/table.rs`                      | 122       | UPDATE | Replace singular command field with agents vec              |
| `crates/kild-ui/src/views/detail_panel.rs`      | 36,69-74  | UPDATE | Remove .or_else() fallbacks to singular fields             |
| `crates/kild-ui/src/views/kild_list.rs`         | 178,183   | UPDATE | Remove .or_else() fallbacks to singular fields             |
| `crates/kild-core/src/sessions/types.rs`        | 586-902   | UPDATE | Update test fixtures to not use singular fields            |
| `crates/kild-core/src/sessions/handler.rs`      | 1408-2225 | UPDATE | Update test fixtures to not use singular fields            |
| `crates/kild-core/src/sessions/info.rs`         | 217-489   | UPDATE | Update test fixtures to not use singular fields            |

### Integration Points

- `crates/kild-core/src/sessions/persistence.rs` — Serializes Session to JSON. No code changes needed; field removal automatically stops serializing them. `#[serde(default)]` on `agents` handles deserialization of old files.
- `crates/kild-core/src/sessions/validation.rs:55` — Validates `session.agent` (kept field), no change needed.
- `crates/kild-core/src/state/dispatch.rs:60` — Uses `session.agent` (kept field), no change needed.
- `crates/kild-core/src/health/operations.rs:88` — Uses `session.agent` (kept field), no change needed.

### Git History

- **Introduced**: `3fff816` — "Fix: track all agent processes to prevent orphaning (#217)" (PR #228)
- **Refined**: `d34eeb7`, `109b95b`, `1315549` — Review fixes and simplification
- **Last modified**: `ad0bf70` — "fix: per-agent PID file and window title isolation (#232)"
- **Implication**: Deliberate backward-compat shim, now ready for removal per #229 comments

---

## Implementation Plan

### Step 1: Remove singular process-tracking fields from Session struct

**File**: `crates/kild-core/src/sessions/types.rs`
**Lines**: 189-228
**Action**: UPDATE

**Current code (lines 189-228):**
```rust
    /// Process ID of the spawned terminal/agent process.
    pub process_id: Option<u32>,
    pub process_name: Option<String>,
    pub process_start_time: Option<u64>,
    #[serde(default)]
    pub terminal_type: Option<TerminalType>,
    #[serde(default)]
    pub terminal_window_id: Option<String>,
    #[serde(default)]
    pub command: String,
```

**Required change:** Delete all 6 fields and their doc comments (lines 189-228). Keep `agent`, `last_activity`, `note` fields.

**Why**: These fields duplicate data already stored in `AgentProcess` entries within the `agents` vec.

---

### Step 2: Make `agents` field private and add `#[serde(default)]` handling for removed fields

**File**: `crates/kild-core/src/sessions/types.rs`
**Lines**: 247-256
**Action**: UPDATE

**Current code:**
```rust
    #[serde(default)]
    pub agents: Vec<AgentProcess>,
```

**Required change:**
1. Change `pub agents` to `agents` (private) — per comment on #229
2. Add `#[serde(default)]` denying unknown fields is NOT needed (serde already ignores unknown fields by default during deserialization)
3. The existing `#[serde(default)]` on agents handles old JSON files without the field

**Why**: Enforces all mutations go through accessor methods (`add_agent()`, `clear_agents()`, `set_agents()`).

---

### Step 3: Remove dual-write in create_session

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 252-272
**Action**: UPDATE

**Current code (lines 252-272):**
```rust
    let session = Session {
        id: session_id.clone(),
        project_id: project.id,
        branch: validated.name.clone(),
        worktree_path: worktree.path,
        agent: validated.agent.clone(),
        status: SessionStatus::Active,
        created_at: now.clone(),
        last_activity: Some(now),
        port_range_start: port_start,
        port_range_end: port_end,
        port_count: config.default_port_count,
        process_id: spawn_result.process_id,
        process_name: spawn_result.process_name.clone(),
        process_start_time: spawn_result.process_start_time,
        terminal_type: Some(spawn_result.terminal_type.clone()),
        terminal_window_id: spawn_result.terminal_window_id.clone(),
        command,
        note: request.note.clone(),
        agents: vec![initial_agent],
    };
```

**Required change:** Remove the 6 singular field assignments (process_id through command). Update the logging at lines 277-283 to use `initial_agent` data instead of singular fields.

**Why**: All process data is already in `initial_agent` (the AgentProcess in the vec).

---

### Step 4: Remove dual-write in restart_session

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 1092-1097
**Action**: UPDATE

**Current code:**
```rust
    session.agent = agent;
    session.process_id = spawn_result.process_id;
    session.process_name = process_name;
    session.process_start_time = process_start_time;
    session.terminal_type = Some(spawn_result.terminal_type.clone());
    session.terminal_window_id = spawn_result.terminal_window_id.clone();
```

**Required change:** Keep `session.agent = agent;` (primary agent name update). Remove the 5 process-tracking field writes.

**Why**: Process data is stored in the AgentProcess added via `session.add_agent(new_agent)` at line 1101.

---

### Step 5: Remove dual-write in open_session

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 1228-1235
**Action**: UPDATE

**Current code:**
```rust
    // Keep singular fields updated to latest for backward compat
    session.process_id = spawn_result.process_id;
    session.process_name = process_name;
    session.process_start_time = process_start_time;
    session.terminal_type = Some(spawn_result.terminal_type.clone());
    session.terminal_window_id = spawn_result.terminal_window_id.clone();
    session.command = command;
    session.agent = agent.clone();
```

**Required change:** Remove all 7 lines. The `session.agent` update here was also a dual-write (open doesn't change the primary agent). Keep `session.status = SessionStatus::Active;` and `session.last_activity`.

**Why**: Data is in the AgentProcess added via `session.add_agent(new_agent)` at line 1240.

---

### Step 6: Remove singular field clears in stop_session

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 1389-1392
**Action**: UPDATE

**Current code:**
```rust
    session.clear_agents();
    session.process_id = None;
    session.process_name = None;
    session.process_start_time = None;
```

**Required change:** Keep `session.clear_agents();`. Remove the 3 singular field clears.

**Why**: Fields no longer exist.

---

### Step 7: Remove fallback branch in destroy_session

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 367-495
**Action**: UPDATE

**Current code:**
```rust
if session.has_agents() {
    // Multi-agent path...
} else {
    // Fallback: singular-field logic for old sessions with empty agents vec
    ...
}
```

**Required change:** Remove the `if session.has_agents()` check and the entire `else` fallback branch. Keep only the multi-agent path code (which already handles empty agents vec correctly — the loops simply don't execute).

**Why**: Old sessions deserialize with empty `agents` vec, which means no processes to kill — correct behavior.

---

### Step 8: Remove fallback branch in stop_session

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 1277-1383
**Action**: UPDATE

Same pattern as Step 7. Remove `if session.has_agents()` guard and the fallback `else` branch. Keep the multi-agent loop code.

**Why**: Same reasoning — empty vec = no processes to stop.

---

### Step 9: Remove fallback branch in determine_process_status

**File**: `crates/kild-core/src/sessions/info.rs`
**Lines**: 80-169
**Action**: UPDATE

**Current code:**
```rust
pub fn determine_process_status(session: &Session) -> ProcessStatus {
    if session.has_agents() {
        // Multi-agent path...
        return ProcessStatus::Running/Unknown/Stopped;
    }
    // Fallback: singular-field logic for old sessions
    if let Some(pid) = session.process_id { ... }
}
```

**Required change:** Remove `if session.has_agents()` guard and fallback. Just use the agents iteration directly. If agents vec is empty, the loop produces no results → returns `ProcessStatus::Stopped`.

**Why**: Empty agents vec = stopped session.

---

### Step 10: Remove fallback branch in enrich_session_with_metrics

**File**: `crates/kild-core/src/health/handler.rs`
**Lines**: 64-94
**Action**: UPDATE

**Current code:**
```rust
let running_pid = session.agents().iter()
    .filter_map(|a| a.process_id())
    .find(|&pid| matches!(process::is_process_running(pid), Ok(true)));

let (process_metrics, process_running) = if let Some(pid) = running_pid {
    (get_metrics_for_pid(pid, &session.branch), true)
} else if let Some(pid) = session.process_id {
    // Fallback to singular field
    ...
} else {
    (None, false)
};
```

**Required change:** Remove the `else if let Some(pid) = session.process_id` branch. Keep just the agents-based check and the final `else { (None, false) }`.

**Why**: No singular field to fall back to.

---

### Step 11: Remove fallback in handle_focus_command

**File**: `crates/kild/src/commands.rs`
**Lines**: 864-874
**Action**: UPDATE

**Current code:**
```rust
let (term_type, window_id) = if let Some(latest) = session.latest_agent() {
    (latest.terminal_type().cloned(), latest.terminal_window_id().map(|s| s.to_string()))
} else {
    (session.terminal_type.clone(), session.terminal_window_id.clone())
};
```

**Required change:** Replace with direct `session.latest_agent()` call. If `None`, there's no terminal to focus.

```rust
let Some(latest) = session.latest_agent() else {
    // No agent process tracked — nothing to focus
    warn!(event = "cli.focus.no_agent", branch = name);
    println!("No agent process to focus for kild '{}'", name);
    return Ok(());
};
let (term_type, window_id) = (
    latest.terminal_type().cloned(),
    latest.terminal_window_id().map(|s| s.to_string()),
);
```

**Why**: No fallback field. If no agents tracked, session is stopped.

---

### Step 12: Remove fallback in handle_status_command

**File**: `crates/kild/src/commands.rs`
**Lines**: 1119-1168
**Action**: UPDATE

**Required change:** Remove the `else` branch (lines 1142-1168). Keep only the `if session.has_agents()` multi-agent display. For sessions with no agents, show "No agents tracked" instead of singular field display.

**Why**: No singular fields to display.

---

### Step 13: Remove fallback in table.rs print_row

**File**: `crates/kild/src/table.rs`
**Lines**: 53-133
**Action**: UPDATE

Three changes:
1. **Process status (lines 55-100):** Remove `else` fallback using `session.process_id`. Keep agents iteration; empty vec produces `Run(0/0)` or similar.
2. **Agent name (lines 105-115):** Remove `session.agent` fallback. Use `session.latest_agent().map_or(session.agent.as_str(), |a| a.agent())` — `session.agent` is kept as the primary agent name, which is correct here.
3. **Command (line 122):** Replace `session.command` with `session.latest_agent().map_or("", |a| a.command())`.

**Why**: Process data lives in agents vec. Command lives in AgentProcess.

---

### Step 14: Remove .or_else() fallbacks in UI views

**File**: `crates/kild-ui/src/views/detail_panel.rs`
**Lines**: 27-36, 66-73
**Action**: UPDATE

**Current code:**
```rust
let agent = if session.agent_count() > 1 {
    session.agents().iter().map(|a| a.agent()).collect::<Vec<_>>().join(", ")
} else {
    session.agent.clone()  // singular field
};

let terminal_type_for_focus = session.latest_agent()
    .and_then(|a| a.terminal_type().cloned())
    .or_else(|| session.terminal_type.clone());
```

**Required change:**
1. Agent display: keep using `session.agent` for single-agent case (it's the kept primary agent field)
2. Terminal focus: remove `.or_else(|| session.terminal_type.clone())` and `.or_else(|| session.terminal_window_id.clone())`

**File**: `crates/kild-ui/src/views/kild_list.rs`
**Lines**: 174-183, 269
**Action**: UPDATE

Same changes — remove `.or_else()` fallbacks for terminal_type and window_id.

**Why**: No singular terminal/window fields to fall back to.

---

### Step 15: Update restart_session read paths

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 960-966
**Action**: UPDATE

Check if restart_session reads singular fields before the kill step. It reads `session.process_id`, `session.process_name`, `session.process_start_time` to kill the old process. Replace with reading from `session.latest_agent()`.

**Why**: Old process info is in the agents vec.

---

### Step 16: Update CLI display paths that use singular fields for logging

**File**: `crates/kild/src/commands.rs`
**Lines**: 126, 536, 537, 573, 574, 1099, 1145, 1175
**Action**: UPDATE

- Lines using `session.agent` — keep (primary agent field is retained)
- Lines using `session.process_id` for display — replace with `session.latest_agent().and_then(|a| a.process_id())`

**Why**: Process ID is in agents vec, not on Session directly.

---

### Step 17: Update all test fixtures

**Files**:
- `crates/kild-core/src/sessions/types.rs` (~586-902)
- `crates/kild-core/src/sessions/handler.rs` (~1408-2225)
- `crates/kild-core/src/sessions/info.rs` (~217-489)
**Action**: UPDATE

All test Session constructors currently include singular fields:
```rust
Session {
    process_id: None,
    process_name: None,
    process_start_time: None,
    terminal_type: None,
    terminal_window_id: None,
    command: "claude-code".to_string(),
    agents: vec![],
    ...
}
```

**Required change:** Remove all 6 singular field assignments. For tests that need process data, populate the `agents` vec with appropriate `AgentProcess` entries instead.

Consider creating a test helper:
```rust
#[cfg(test)]
fn test_session(branch: &str, agent: &str) -> Session {
    Session {
        id: format!("test/{}", branch),
        project_id: "test".to_string(),
        branch: branch.to_string(),
        worktree_path: PathBuf::from(format!("/tmp/{}", branch)),
        agent: agent.to_string(),
        status: SessionStatus::Active,
        created_at: "2024-01-01T00:00:00Z".to_string(),
        port_range_start: 3000,
        port_range_end: 3009,
        port_count: 10,
        last_activity: Some("2024-01-01T00:00:00Z".to_string()),
        note: None,
        agents: vec![],
    }
}
```

**Why**: Singular fields no longer exist on Session.

---

## Patterns to Follow

**From codebase — AgentProcess creation pattern:**

```rust
// SOURCE: crates/kild-core/src/sessions/handler.rs:241-251
let initial_agent = AgentProcess::new(
    validated.agent.clone(),
    spawn_id,
    spawn_result.process_id,
    spawn_result.process_name.clone(),
    spawn_result.process_start_time,
    Some(spawn_result.terminal_type.clone()),
    spawn_result.terminal_window_id.clone(),
    command.clone(),
    now.clone(),
)?;
```

**From codebase — accessor method usage pattern:**

```rust
// SOURCE: crates/kild-core/src/sessions/types.rs:441-473
session.agents()          // &[AgentProcess]
session.has_agents()      // bool
session.agent_count()     // usize
session.latest_agent()    // Option<&AgentProcess>
session.add_agent(agent)  // push
session.clear_agents()    // clear
session.set_agents(vec)   // replace
```

---

## Edge Cases & Risks

| Risk/Edge Case                             | Mitigation                                                                                                         |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| Old session JSON without `agents` field    | `#[serde(default)]` deserializes as empty vec → no processes tracked → correct "stopped" state                     |
| Old session JSON WITH singular fields      | Serde ignores unknown fields during deserialization by default. The singular fields simply won't be read.           |
| `agents` field made private breaks tests   | Tests must use `set_agents()` or `add_agent()` instead of direct field initialization in struct literal            |
| Private `agents` breaks struct literal     | Session struct literals (in create_session, tests) need a builder or constructor method                            |
| Display of empty agents in table           | Empty agents vec → show "No PID" and use `session.agent` for agent name (primary agent field is retained)          |
| restart_session kill of old process        | Must read old process info from `latest_agent()` before `clear_agents()` is called                                 |

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

1. Create a new kild session → verify JSON has no singular fields, agents vec is populated
2. Open a second agent in the session → verify agents vec has 2 entries
3. Stop the session → verify agents vec is cleared
4. Destroy a session → verify all terminal windows close and processes are killed
5. Focus a session → verify terminal window comes to foreground
6. List sessions → verify correct display of agent name, process status, command
7. Test with an old session JSON file (manually crafted without `agents` field) → verify it deserializes and shows as stopped

---

## Scope Boundaries

**IN SCOPE:**
- Remove 6 singular process-tracking fields from Session
- Remove all dual-write logic
- Remove all fallback read paths
- Make `agents` field private
- Update all test fixtures
- Add Session constructor/builder if needed for private `agents` field

**OUT OF SCOPE (do not touch):**
- `agent` field on Session (primary agent name — kept)
- `last_activity` field (not a process-tracking field)
- `note` field (not a process-tracking field)
- AgentProcess struct or its accessors (already correct)
- Session accessor methods (already correct, may need minor adjustments)
- Port allocation logic
- Config handling
- Git worktree operations

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-05T10:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-229.md`
