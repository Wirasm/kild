# Investigation: Commands exit immediately in daemon PTY (bare shell and agents)

**Issue**: #302 (https://github.com/Wirasm/kild/issues/302)
**Type**: BUG
**Investigated**: 2026-02-10T12:00:00Z

### Assessment

| Metric     | Value    | Reasoning                                                                                                   |
| ---------- | -------- | ----------------------------------------------------------------------------------------------------------- |
| Severity   | HIGH     | All daemon-mode interactive sessions are broken; no workaround short of terminal mode                       |
| Complexity | MEDIUM   | 2 files changed (handler.rs, manager.rs), plus protocol type update; well-scoped with clear portable-pty API |
| Confidence | HIGH     | Root cause confirmed via portable-pty source audit; `new_default_prog()` API exists for exactly this case   |

---

## Problem Statement

Commands spawned in daemon-managed PTYs via `portable-pty` exit immediately. Bare shells (`/bin/zsh`) and agents (`claude-code`) both exit, while non-interactive commands (`sleep 60`) survive. This makes daemon mode unusable for its primary purpose: running interactive AI agents and shells.

---

## Analysis

### Root Cause

The daemon uses `CommandBuilder::new(command)` for all PTY spawns, which directly execs the target binary. This has two consequences:

1. **Bare shells get non-login argv[0]**: `CommandBuilder::new("/bin/zsh")` sets `argv[0] = "/bin/zsh"`. Shells check if `argv[0]` starts with `-` to decide login mode. Without it, profile files (`.zprofile`, `.zshrc`) are not sourced, and the shell may determine it has nothing to do and exit.

2. **Agent commands lack shell environment**: `claude-code` (Node.js) is exec'd directly. Without a parent shell that sourced profile files, `PATH` extensions from `.zprofile`/`.bashrc` are missing, and the process environment is minimal. Additionally, `portable-pty`'s `close_random_fds()` (unix.rs:118-143) closes ALL fds > 2 before exec, which can kill fds that Node.js needs during startup (libuv internal pipe, v8 profiler fd).

### Evidence Chain

WHY: Interactive commands exit immediately in daemon PTYs
BECAUSE: Commands are spawned with `CommandBuilder::new(command)` which directly execs

Evidence: `crates/kild-daemon/src/pty/manager.rs:138`:
```rust
let mut cmd = CommandBuilder::new(command);
cmd.args(args);
```

BECAUSE: `CommandBuilder::new()` takes the non-default-prog path in `as_command()`

Evidence: `portable-pty-0.8.1/src/cmdbuilder.rs:461-474`:
```rust
let mut cmd = if self.is_default_prog() {
    // Login shell path: argv[0] = "-zsh"
    let mut cmd = std::process::Command::new(&shell);
    cmd.arg0(&format!("-{}", basename));
    cmd
} else {
    // Our current path: argv[0] = "/bin/zsh" (no login prefix)
    let resolved = self.search_path(&self.args[0], dir)?;
    let mut cmd = std::process::Command::new(&resolved);
    cmd.arg0(&self.args[0]);
    cmd.args(&self.args[1..]);
    cmd
};
```

ROOT CAUSE: `PtyManager::create()` has no concept of "bare shell" vs "explicit command". It always uses `CommandBuilder::new(command)` which bypasses portable-pty's built-in login shell setup (`CommandBuilder::new_default_prog()`).

### Affected Files

| File                                             | Lines   | Action | Description                                                      |
| ------------------------------------------------ | ------- | ------ | ---------------------------------------------------------------- |
| `crates/kild-daemon/src/pty/manager.rs`          | 112-157 | UPDATE | Support `new_default_prog()` path for bare shell sessions        |
| `crates/kild-daemon/src/protocol/messages.rs`    | 18-39   | UPDATE | Add `use_login_shell` field to CreateSession message             |
| `crates/kild-daemon/src/session/manager.rs`      | 52-94   | UPDATE | Pass `use_login_shell` through to PtyManager                     |
| `crates/kild-daemon/src/server/handler.rs`       | varies  | UPDATE | Deserialize `use_login_shell` from IPC message                   |
| `crates/kild-core/src/sessions/handler.rs`       | 745-768 | UPDATE | Set `use_login_shell` for bare shell; wrap agents in login shell |
| `crates/kild-core/src/daemon/client.rs`          | varies  | UPDATE | Add `use_login_shell` to DaemonCreateRequest                     |

### Integration Points

- `crates/kild-core/src/sessions/handler.rs:248` calls `build_daemon_create_request()` and sends IPC
- `crates/kild-daemon/src/session/manager.rs:86` delegates to `PtyManager::create()`
- `crates/kild-daemon/src/pty/manager.rs:138` constructs `CommandBuilder`
- `feat/tmux-shim` branch adds shim env vars to `build_daemon_create_request()` - this fix must be compatible

### Git History

- **Introduced**: `6f1cfa7` - "feat: add kild-daemon crate with PTY ownership, IPC server, and session persistence (#294)"
- **Last modified**: `dc889ca` - "feat: daemon status sync and open command daemon support (#299)"
- **Implication**: Original bug since daemon inception; PTY spawn was always direct-exec

---

## Implementation Plan

### Step 1: Add `use_login_shell` to IPC protocol

**File**: `crates/kild-daemon/src/protocol/messages.rs`
**Action**: UPDATE

Add `use_login_shell: bool` to the `CreateSession` variant of `ClientMessage`. When `true`, the daemon uses `CommandBuilder::new_default_prog()` instead of `CommandBuilder::new(command)`.

**Why**: The daemon needs to know whether to spawn a login shell or an explicit command. This is determined by kild-core (bare shell vs agent) and communicated via IPC.

---

### Step 2: Update PtyManager to support login shell mode

**File**: `crates/kild-daemon/src/pty/manager.rs`
**Lines**: 112-157
**Action**: UPDATE

**Current code (line 138):**
```rust
let mut cmd = CommandBuilder::new(command);
cmd.args(args);
cmd.cwd(working_dir);
```

**Required change:**
```rust
let mut cmd = if use_login_shell {
    let mut cmd = CommandBuilder::new_default_prog();
    cmd.cwd(working_dir);
    cmd
} else {
    let mut cmd = CommandBuilder::new(command);
    cmd.args(args);
    cmd.cwd(working_dir);
    cmd
};
```

Add `use_login_shell: bool` parameter to `PtyManager::create()`.

**Why**: `CommandBuilder::new_default_prog()` uses `$SHELL` with login prefix (`argv[0] = "-zsh"`), matching how terminal emulators spawn shells. Note: `new_default_prog()` panics if `.arg()` is called, so args must NOT be added in this path.

---

### Step 3: Thread `use_login_shell` through SessionManager

**File**: `crates/kild-daemon/src/session/manager.rs`
**Lines**: 52-94
**Action**: UPDATE

Add `use_login_shell: bool` parameter to `SessionManager::create_session()` and pass it through to `self.pty_manager.create()`.

---

### Step 4: Deserialize `use_login_shell` in server handler

**File**: `crates/kild-daemon/src/server/handler.rs` (or wherever IPC messages are dispatched)
**Action**: UPDATE

Extract `use_login_shell` from the deserialized `CreateSession` message and pass to `SessionManager::create_session()`.

---

### Step 5: Update kild-core daemon client and handler

**File**: `crates/kild-core/src/daemon/client.rs`
**Action**: UPDATE

Add `use_login_shell: bool` to `DaemonCreateRequest` struct and include it in the JSONL payload.

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 745-768
**Action**: UPDATE

**Current code:**
```rust
fn build_daemon_create_request(
    agent_command: &str,
    agent_name: &str,
) -> Result<(String, Vec<String>, Vec<(String, String)>), SessionError> {
    let parts: Vec<&str> = agent_command.split_whitespace().collect();
    let (cmd, cmd_args) = parts.split_first().ok_or_else(|| ...)?;
    // ...
}
```

**Required change:**

Return a struct or tuple that includes `use_login_shell`:

```rust
fn build_daemon_create_request(
    agent_command: &str,
    agent_name: &str,
) -> Result<(String, Vec<String>, Vec<(String, String)>, bool), SessionError> {
    let use_login_shell = agent_name == "shell";

    let (cmd, cmd_args) = if use_login_shell {
        // For bare shell: command/args are ignored by new_default_prog(),
        // but we still pass them for logging. The daemon will use $SHELL.
        (agent_command.to_string(), vec![])
    } else {
        // For agents: wrap in login shell to get proper environment
        // sh -lc 'exec claude-code --flags'
        // This ensures profile files are sourced and the agent gets
        // a full environment (PATH extensions, etc.)
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let escaped = agent_command.replace('\'', "'\\''");
        (shell, vec!["-lc".to_string(), format!("exec {}", escaped)])
    };
    // ... env_vars collection ...
    Ok((cmd, cmd_args, env_vars, use_login_shell))
}
```

**Why**: Two distinct strategies:
- **Bare shell** (`--no-agent`): Use `new_default_prog()` for native login shell behavior
- **Agents** (`--agent claude`): Wrap in `$SHELL -lc 'exec <command>'` so profile files are sourced before the agent starts, providing full PATH and environment. The `exec` replaces the wrapper shell with the agent for clean PID tracking.

---

### Step 6: Update `build_daemon_create_request` call site

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 248-260
**Action**: UPDATE

Destructure the new 4-tuple and pass `use_login_shell` into `DaemonCreateRequest`.

Also update the `open_session` daemon path (around line 654-722 on main, varies on feat/tmux-shim) which calls the same function.

---

### Step 7: Add exit code logging

**File**: `crates/kild-daemon/src/pty/output.rs`
**Action**: UPDATE

After the PTY reader detects EOF, attempt to capture the child's exit code. Add it to `PtyExitEvent` so it can be logged when the session transitions to Stopped.

```rust
pub struct PtyExitEvent {
    pub session_id: String,
    pub exit_code: Option<i32>,
}
```

Log at the session manager level:
```rust
info!(
    event = "daemon.session.pty_exited",
    session_id = session_id,
    exit_code = ?event.exit_code,
);
```

**Why**: Without exit codes, debugging PTY failures is blind. This is critical for the "no silent failures" principle.

---

### Step 8: Update existing tests and add new ones

**File**: `crates/kild-daemon/src/pty/manager.rs` (tests section)
**Action**: UPDATE

Update existing tests to pass `use_login_shell: false` (preserving current behavior).

Add new test:
```rust
#[test]
fn test_create_with_login_shell_uses_default_prog() {
    let mut mgr = PtyManager::new();
    let tmpdir = tempfile::tempdir().unwrap();
    // use_login_shell=true should succeed (uses $SHELL)
    let result = mgr.create(
        "s1", "", &[], tmpdir.path(), 24, 80, &[], true,
    );
    assert!(result.is_ok());
    mgr.destroy("s1").unwrap();
}
```

**File**: `crates/kild-core/src/sessions/handler.rs` (tests section)
**Action**: UPDATE

Update `build_daemon_create_request` tests for the new return type.

Add tests:
- Bare shell (`agent_name == "shell"`) returns `use_login_shell: true`
- Agent returns `use_login_shell: false` with shell-wrapped command
- Agent command with args is properly escaped in the shell wrapper

---

## Patterns to Follow

**From codebase - IPC protocol pattern:**
```rust
// SOURCE: crates/kild-daemon/src/protocol/messages.rs
// Pattern for adding fields to CreateSession
#[derive(Debug, Deserialize)]
pub struct CreateSessionFields {
    pub session_id: String,
    pub working_directory: String,
    pub command: String,
    pub args: Vec<String>,
    pub env_vars: HashMap<String, String>,
    pub rows: u16,
    pub cols: u16,
    // Add: pub use_login_shell: bool,
}
```

**From codebase - PtyManager create pattern:**
```rust
// SOURCE: crates/kild-daemon/src/pty/manager.rs:112-195
// Pattern: create() takes all config as params, constructs CommandBuilder internally
pub fn create(
    &mut self,
    session_id: &str,
    command: &str,
    args: &[&str],
    working_dir: &std::path::Path,
    rows: u16,
    cols: u16,
    env_vars: &[(String, String)],
    // Add: use_login_shell: bool,
) -> Result<&ManagedPty, DaemonError>
```

**From portable-pty - new_default_prog usage:**
```rust
// SOURCE: portable-pty-0.8.1/src/cmdbuilder.rs:248-257
// new_default_prog() uses $SHELL with login prefix (argv[0] = "-zsh")
// IMPORTANT: panics if .arg() is called on it
let cmd = CommandBuilder::new_default_prog();
// Only .cwd() and .env() are safe to call
cmd.cwd(working_dir);
cmd.env("KEY", "VALUE");
```

---

## Edge Cases & Risks

| Risk/Edge Case                                    | Mitigation                                                                                       |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `new_default_prog()` panics if `.arg()` called    | Guard with `if use_login_shell` branch that skips `cmd.args()`                                   |
| `$SHELL` not set in daemon environment            | `new_default_prog()` falls back to `/bin/sh` internally (portable-pty handles this)              |
| Agent command with single quotes in args          | Escape with `replace('\'', "'\\''")` before wrapping in `sh -lc '...'`                          |
| `feat/tmux-shim` branch adds shim env vars        | This fix is additive; shim env vars are added to `env_vars` vec independently                    |
| `close_random_fds()` kills Node.js fds for agents | Shell wrapper (`sh -lc 'exec cmd'`) means `close_random_fds()` only affects `sh`, not the agent |
| Backward compat: daemon protocol change           | Add `#[serde(default)]` on `use_login_shell` so old clients default to `false`                   |
| `open_session` also creates daemon PTYs           | Must update the open path too (handler.rs ~line 654-722 on main)                                 |

---

## Validation

### Automated Checks

```bash
cargo fmt --check && cargo clippy --all -- -D warnings && cargo test --all && cargo build --all
```

### Manual Verification

1. `kild daemon start && kild create test-shell --no-agent --daemon` - shell should stay alive
2. `kild attach test-shell` - should get interactive shell prompt
3. `kild create test-claude --agent claude --daemon` - claude should stay alive
4. `kild attach test-claude` - should get claude interface
5. `kild create test-sleep --daemon` (with agent) - verify existing behavior preserved
6. `kild list` - all sessions should show Active, not Stopped

---

## Scope Boundaries

**IN SCOPE:**
- `PtyManager::create()` login shell support via `new_default_prog()`
- Agent command wrapping in login shell (`$SHELL -lc 'exec <cmd>'`)
- IPC protocol update for `use_login_shell`
- Exit code capture and logging
- Test updates

**OUT OF SCOPE (do not touch):**
- Terminal mode spawn (already works correctly)
- tmux shim integration (separate branch, additive)
- Forking or patching portable-pty
- PTY reader/output infrastructure
- Attach command logic
- Session state machine

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-10T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-302.md`
