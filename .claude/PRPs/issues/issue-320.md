# Investigation: Daemon PTY sessions exit immediately after open/resume

**Issue**: #320 (https://github.com/Wirasm/kild/issues/320)
**Type**: BUG
**Investigated**: 2026-02-11T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                                                     |
| ---------- | ------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Severity   | HIGH   | Core daemon workflow broken: users can't reliably open/resume daemon sessions, and get a confusing error with no diagnostic info               |
| Complexity | MEDIUM | 4 files across 2 crates (kild-daemon state + wire type, kild-core client + handler), well-understood code paths, moderate integration surface  |
| Confidence | HIGH   | Race condition clearly traced through code: PTY exits before client saves session, no post-creation health check exists, exit code not surfaced |

---

## Problem Statement

When using `kild open --daemon` or `kild open --resume --daemon`, the daemon PTY session sometimes exits immediately after spawning. The `kild open` command reports success (session set to Active), but when the user runs `kild attach`, the daemon correctly reports the session as Stopped, producing the confusing error: "Attach failed: session not running". There is no early exit detection, no exit code surfacing, and no PTY output (scrollback) shown to help diagnose why the agent exited.

---

## Analysis

### Root Cause

There are two interrelated problems:

**Problem 1: No early exit detection.** `open_session()` calls `create_pty_session()` via IPC, which returns as soon as the daemon spawns the PTY and transitions to Running. There is no post-creation health check. If the PTY process exits within milliseconds (e.g., agent fails to start, bad resume session, env issue), kild-core saves the session as Active and returns success to the CLI. The user sees a success message, but `kild attach` fails.

**Problem 2: No diagnostic information.** The daemon captures the exit code in `handle_pty_exit()` but only logs it — it's not stored on `DaemonSession` or available via the `get_session` IPC query. The PTY scrollback buffer (which likely contains the agent's error output) exists in the daemon but kild-core has no client function to retrieve it. The user gets "session not running" with zero context about why.

### Evidence Chain

WHY: `kild attach` fails with "session not running"
↓ BECAUSE: Daemon session is in Stopped state when attach is called
Evidence: `crates/kild-daemon/src/session/manager.rs:145-147` — `attach_client()` checks `session.state() != SessionState::Running`

↓ BECAUSE: PTY process exited and daemon handled the exit event
Evidence: `crates/kild-daemon/src/server/mod.rs:97-105` — main loop receives PtyExitEvent and calls `handle_pty_exit()`
Evidence: `crates/kild-daemon/src/session/manager.rs:388-397` — `handle_pty_exit()` calls `session.set_stopped()`

↓ BECAUSE: `kild open` reported success without verifying PTY is still alive
Evidence: `crates/kild-core/src/sessions/handler.rs:859-864` — `create_pty_session()` returns immediately after daemon spawns PTY
Evidence: `crates/kild-core/src/sessions/handler.rs:927` — session status set to Active unconditionally
Evidence: `crates/kild-core/src/sessions/handler.rs:942` — session saved to file, function returns Ok

↓ ROOT CAUSE: No post-creation health check in `open_session()` daemon path
Evidence: `crates/kild-core/src/sessions/handler.rs:836-877` — entire daemon code path has no verification step after `create_pty_session()` returns

AND: Exit code is logged but not stored/surfaced
Evidence: `crates/kild-daemon/src/session/manager.rs:381-385` — exit_code logged at info level but discarded
Evidence: `crates/kild-daemon/src/session/state.rs:36-52` — `DaemonSession` has no `exit_code` field

### Affected Files

| File                                               | Lines     | Action | Description                                                 |
| -------------------------------------------------- | --------- | ------ | ----------------------------------------------------------- |
| `crates/kild-daemon/src/session/state.rs`          | 36-52     | UPDATE | Add `exit_code: Option<i32>` field to DaemonSession         |
| `crates/kild-daemon/src/session/manager.rs`        | 346-402   | UPDATE | Store exit_code on DaemonSession in handle_pty_exit         |
| `crates/kild-daemon/src/types.rs`                  | 172-183   | UPDATE | Add `exit_code: Option<i32>` to SessionInfo wire type       |
| `crates/kild-core/src/daemon/client.rs`            | 144-199   | UPDATE | Add `read_scrollback()` client function + health check helper |
| `crates/kild-core/src/sessions/handler.rs`         | 836-877   | UPDATE | Add post-creation health check in daemon path               |
| `crates/kild-core/src/sessions/handler.rs`         | 240-270   | UPDATE | Add same health check in create_session daemon path         |
| `crates/kild-core/src/sessions/errors.rs`          | 75-76     | UPDATE | Add DaemonPtyExitedEarly error variant                      |

### Integration Points

- `crates/kild-daemon/src/session/state.rs:198-208` — `to_session_info()` converts DaemonSession to wire type (needs exit_code)
- `crates/kild-daemon/src/server/connection.rs:349-358` — `GetSession` handler returns SessionInfo (will automatically include exit_code)
- `crates/kild-daemon/src/server/connection.rs:361-371` — `ReadScrollback` handler already exists and works
- `crates/kild-core/src/daemon/client.rs:290-365` — `get_session_status()` already queries daemon, can be extended to return exit_code
- `crates/kild-core/src/sessions/handler.rs:240-270` — `create_session()` daemon path also needs the same health check
- `crates/kild-daemon/tests/integration.rs:333-372` — Existing test for PTY exit detection pattern

### Git History

- **Prior fix**: `7d5358f` (2026-02-10) — "fix: daemon PTY commands exit immediately (#302)" — Fixed missing login shell wrapping. Different root cause (commands couldn't find binaries). This issue is about detecting when the process starts but exits quickly.
- **Open introduced**: `dc889ca` (2026-02-09) — "feat: daemon status sync and open command daemon support (#299)" — Added daemon mode to `kild open`
- **Implication**: The early exit detection gap has existed since the daemon open feature was first added. The #302 fix reduced the frequency (commands no longer fail due to missing PATH) but didn't add verification.

---

## Implementation Plan

### Step 1: Add exit_code field to DaemonSession

**File**: `crates/kild-daemon/src/session/state.rs`
**Lines**: 36-52
**Action**: UPDATE

**Current code:**
```rust
pub struct DaemonSession {
    id: String,
    working_directory: String,
    command: String,
    created_at: String,
    state: SessionState,
    output_tx: Option<broadcast::Sender<Vec<u8>>>,
    scrollback: Arc<Mutex<ScrollbackBuffer>>,
    attached_clients: HashSet<ClientId>,
    pty_pid: Option<u32>,
}
```

**Required change:**
```rust
pub struct DaemonSession {
    id: String,
    working_directory: String,
    command: String,
    created_at: String,
    state: SessionState,
    output_tx: Option<broadcast::Sender<Vec<u8>>>,
    scrollback: Arc<Mutex<ScrollbackBuffer>>,
    attached_clients: HashSet<ClientId>,
    pty_pid: Option<u32>,
    /// Exit code of the PTY child process. Set when the process exits.
    exit_code: Option<i32>,
}
```

Also update:
- `DaemonSession::new()` at line 56-74: Initialize `exit_code: None`
- Add getter `pub fn exit_code(&self) -> Option<i32>` alongside existing getters
- Add setter `pub fn set_exit_code(&mut self, code: Option<i32>)` for use by handle_pty_exit
- `to_session_info()` at line 198-208: Include exit_code in SessionInfo

**Why**: The exit code is currently captured in `handle_pty_exit()` but discarded after logging. Storing it on the session makes it available to IPC queries.

---

### Step 2: Add exit_code to SessionInfo wire type

**File**: `crates/kild-daemon/src/types.rs`
**Lines**: 172-183
**Action**: UPDATE

**Current code:**
```rust
pub struct SessionInfo {
    pub id: String,
    pub working_directory: String,
    pub command: String,
    pub status: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pty_pid: Option<u32>,
}
```

**Required change:**
```rust
pub struct SessionInfo {
    pub id: String,
    pub working_directory: String,
    pub command: String,
    pub status: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pty_pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}
```

**Why**: The wire type needs the field so `get_session` IPC responses include exit_code. Using `skip_serializing_if` keeps backward compatibility with existing clients.

---

### Step 3: Store exit_code in handle_pty_exit

**File**: `crates/kild-daemon/src/session/manager.rs`
**Lines**: 346-402
**Action**: UPDATE

**Current code (relevant section):**
```rust
info!(
    event = "daemon.session.pty_exited",
    session_id = session_id,
    exit_code = ?exit_code,
);

// Transition session to Stopped
if let Some(session) = self.sessions.get_mut(session_id) {
    let output_tx = session.output_tx();
    if let Err(e) = session.set_stopped() {
```

**Required change:**
```rust
info!(
    event = "daemon.session.pty_exited",
    session_id = session_id,
    exit_code = ?exit_code,
);

// Transition session to Stopped and record exit code
if let Some(session) = self.sessions.get_mut(session_id) {
    session.set_exit_code(exit_code);
    let output_tx = session.output_tx();
    if let Err(e) = session.set_stopped() {
```

**Why**: Store the exit code before transitioning to Stopped so it's available when kild-core queries the session.

---

### Step 4: Add read_scrollback client function to kild-core

**File**: `crates/kild-core/src/daemon/client.rs`
**Action**: UPDATE — add new function after `get_session_status()`

**Required change:**
```rust
/// Read the scrollback buffer from a daemon session.
///
/// Returns the raw scrollback bytes (decoded from base64), or None if the
/// daemon is not running or the session is not found.
pub fn read_scrollback(daemon_session_id: &str) -> Result<Option<Vec<u8>>, DaemonClientError> {
    let socket_path = crate::daemon::socket_path();

    let request = serde_json::json!({
        "id": format!("scrollback-{}", daemon_session_id),
        "type": "read_scrollback",
        "session_id": daemon_session_id,
    });

    let mut stream = match connect(&socket_path) {
        Ok(s) => s,
        Err(DaemonClientError::NotRunning { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };

    stream.set_read_timeout(Some(Duration::from_secs(2)))?;

    match send_request(&mut stream, request) {
        Ok(response) => {
            let data = response
                .get("data")
                .and_then(|d| d.as_str())
                .unwrap_or("");
            use base64::Engine;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(data)
                .unwrap_or_default();
            Ok(Some(decoded))
        }
        Err(DaemonClientError::DaemonError { ref message })
            if message.contains("not_found") || message.contains("unknown_session") =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
```

Also add a helper function to get session info including exit_code:

```rust
/// Query the daemon for a session's status and exit code.
///
/// Returns (status, exit_code) if the daemon knows about this session.
pub fn get_session_info(
    daemon_session_id: &str,
) -> Result<Option<(String, Option<i32>)>, DaemonClientError> {
    let socket_path = crate::daemon::socket_path();

    let request = serde_json::json!({
        "id": format!("info-{}", daemon_session_id),
        "type": "get_session",
        "session_id": daemon_session_id,
    });

    let mut stream = match connect(&socket_path) {
        Ok(s) => s,
        Err(DaemonClientError::NotRunning { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };

    stream.set_read_timeout(Some(Duration::from_secs(2)))?;

    match send_request(&mut stream, request) {
        Ok(response) => {
            let session = response.get("session");
            let status = session
                .and_then(|s| s.get("status"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            let exit_code = session
                .and_then(|s| s.get("exit_code"))
                .and_then(|c| c.as_i64())
                .map(|c| c as i32);
            match status {
                Some(s) => Ok(Some((s, exit_code))),
                None => Ok(None),
            }
        }
        Err(DaemonClientError::DaemonError { ref message })
            if message.contains("not_found") || message.contains("unknown_session") =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
```

**Why**: kild-core needs these functions to detect early exits and retrieve diagnostic information from the daemon.

---

### Step 5: Add DaemonPtyExitedEarly error variant

**File**: `crates/kild-core/src/sessions/errors.rs`
**Lines**: 75-76
**Action**: UPDATE

**Required change — add new variant:**
```rust
#[error("Daemon PTY exited immediately (exit code: {exit_code:?}). Last output:\n{scrollback_tail}")]
DaemonPtyExitedEarly {
    exit_code: Option<i32>,
    scrollback_tail: String,
},
```

Add to error_code match:
```rust
SessionError::DaemonPtyExitedEarly { .. } => "DAEMON_PTY_EXITED_EARLY",
```

Not a user error (is_user_error returns false) — this indicates a system/agent issue.

**Why**: Distinct error variant with exit code and scrollback tail gives the user actionable diagnostic information.

---

### Step 6: Add early exit detection in open_session daemon path

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 859-877
**Action**: UPDATE

**Current code:**
```rust
let daemon_result =
    crate::daemon::client::create_pty_session(&daemon_request).map_err(|e| {
        SessionError::DaemonError {
            message: e.to_string(),
        }
    })?;

AgentProcess::new(
    agent.clone(),
    spawn_id,
    None,
    // ... rest of AgentProcess construction
```

**Required change:**
```rust
let daemon_result =
    crate::daemon::client::create_pty_session(&daemon_request).map_err(|e| {
        SessionError::DaemonError {
            message: e.to_string(),
        }
    })?;

// Early exit detection: wait briefly, then verify PTY is still alive.
// Fast-failing processes (bad resume session, missing binary, env issues)
// typically exit within 200ms of spawn.
std::thread::sleep(std::time::Duration::from_millis(200));

if let Ok(Some((status, exit_code))) =
    crate::daemon::client::get_session_info(&daemon_result.daemon_session_id)
{
    if status == "stopped" {
        // PTY exited immediately — fetch scrollback for diagnostics
        let scrollback_tail = crate::daemon::client::read_scrollback(
            &daemon_result.daemon_session_id,
        )
        .ok()
        .flatten()
        .map(|bytes| {
            let text = String::from_utf8_lossy(&bytes);
            // Take last 20 lines for error display
            let lines: Vec<&str> = text.lines().collect();
            let start = lines.len().saturating_sub(20);
            lines[start..].join("\n")
        })
        .unwrap_or_default();

        // Clean up the stopped daemon session
        let _ = crate::daemon::client::destroy_daemon_session(
            &daemon_result.daemon_session_id,
            true,
        );

        return Err(SessionError::DaemonPtyExitedEarly {
            exit_code,
            scrollback_tail,
        });
    }
}

AgentProcess::new(
    agent.clone(),
    spawn_id,
    None,
    // ... rest unchanged
```

**Why**: The 200ms sleep gives fast-failing processes time to exit. Then we poll the daemon: if the session is still "running", proceed normally. If it already stopped, we catch it immediately with diagnostics instead of letting the user discover it later via `kild attach`.

---

### Step 7: Add same early exit detection in create_session daemon path

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: ~257-270 (in `create_session()`, daemon code path)
**Action**: UPDATE

Apply the same pattern from Step 6 after `create_pty_session()` returns in the `create_session` function. The exact insertion point is after the daemon_result is received and before `AgentProcess::new()`.

**Why**: `create_session --daemon` has the same gap. Both code paths should be consistent.

---

### Step 8: Add/Update Tests

**File**: `crates/kild-daemon/tests/integration.rs`
**Action**: UPDATE

**Test case: exit_code is stored and returned via get_session**
```rust
#[tokio::test]
async fn test_pty_exit_stores_exit_code() {
    // Create session running `false` (exits with code 1)
    // After exit, get_session should return status="stopped" with exit_code=1
}
```

**File**: `crates/kild-daemon/src/session/state.rs`
**Action**: UPDATE existing tests

**Test case: exit_code getter/setter**
```rust
#[test]
fn test_exit_code_stored_after_stop() {
    let mut session = test_session();
    let (tx, _) = broadcast::channel(1);
    session.set_running(tx, Some(123)).unwrap();
    session.set_exit_code(Some(42));
    session.set_stopped().unwrap();
    assert_eq!(session.exit_code(), Some(42));
}
```

**File**: `crates/kild-daemon/src/types.rs`
**Action**: UPDATE existing SessionInfo tests

**Test case: exit_code in SessionInfo serialization**
```rust
#[test]
fn test_session_info_with_exit_code() {
    let info = SessionInfo {
        id: "test".to_string(),
        // ... other fields
        exit_code: Some(1),
    };
    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("\"exit_code\":1"));
}

#[test]
fn test_session_info_exit_code_omitted_when_none() {
    let info = SessionInfo {
        // ... exit_code: None
    };
    let json = serde_json::to_string(&info).unwrap();
    assert!(!json.contains("exit_code"));
}
```

---

## Patterns to Follow

**From codebase — mirror these exactly:**

```rust
// SOURCE: crates/kild-daemon/src/session/state.rs:86-88
// Pattern for simple getter
pub fn pty_pid(&self) -> Option<u32> {
    self.pty_pid
}
```

```rust
// SOURCE: crates/kild-core/src/daemon/client.rs:290-365
// Pattern for daemon IPC client function (get_session_status)
// - Connect with fallback for NotRunning → Ok(None)
// - Short 2-second timeout
// - Parse response JSON
// - Handle session_not_found as Ok(None)
```

```rust
// SOURCE: crates/kild-core/src/sessions/errors.rs:75-76
// Pattern for error variant with context
#[error("Daemon error: {message}")]
DaemonError { message: String },
```

---

## Edge Cases & Risks

| Risk/Edge Case                         | Mitigation                                                                                         |
| -------------------------------------- | -------------------------------------------------------------------------------------------------- |
| 200ms delay for every daemon create    | Small cost, only in daemon mode. Could be configurable later if needed.                            |
| PTY exits after 200ms grace period     | Existing lazy status sync handles this (list/status detects stopped sessions). Not a regression.   |
| Daemon not reachable during health check | `get_session_info` returns Ok(None) on connection failure → proceed without health check (no worse than current behavior) |
| Scrollback empty (no output before exit) | Default to empty string in error message — exit code alone is still useful                        |
| base64 dependency in kild-core client  | Already used transitively. Add `base64` to kild-core Cargo.toml if not already a direct dependency |
| Cleanup of stopped daemon session      | destroy_daemon_session called with force=true on early exit to clean up                            |

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

1. `kild create test --agent claude --daemon && kild stop test && kild open test --resume --daemon` — should detect if PTY exits early and show diagnostic error
2. `kild create test --agent claude --daemon && kild attach test` — normal flow should still work (200ms delay barely noticeable)
3. `kild create test --no-agent --daemon && kild attach test` — bare shell should work normally
4. `kild list` with a previously-exited daemon session — lazy sync should still work

---

## Scope Boundaries

**IN SCOPE:**
- Adding exit_code storage to DaemonSession and SessionInfo
- Adding early exit detection with 200ms grace period in open_session and create_session
- Adding read_scrollback and get_session_info client functions
- Adding DaemonPtyExitedEarly error variant with diagnostics
- Tests for new functionality

**OUT OF SCOPE (do not touch):**
- Daemon's PTY spawning logic (the spawn itself is fine)
- Agent command construction / login shell wrapping (already fixed in #302)
- `kild attach` error handling (the daemon correctly reports "not running")
- Real-time push notifications for PTY exit (would require WebSocket-style protocol change)
- Configurable health check timeout (YAGNI — 200ms is fine for now)
- Terminal mode (non-daemon) open path
- kild-ui changes (UI has its own refresh mechanism)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-11T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-320.md`
