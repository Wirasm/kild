# Investigation: perf: tmux shim — file I/O on every command, parser allocations

**Issue**: #476 (https://github.com/Wirasm/kild/issues/476)
**Type**: ENHANCEMENT (performance)
**Investigated**: 2026-02-17T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                                                                        |
| ---------- | ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Priority   | HIGH   | Agent teams issue high-frequency shim commands; every `send-keys` does full file I/O + exclusive flock, creating a serial bottleneck across all concurrent panes. |
| Complexity | MEDIUM | 4 files in one crate (state.rs, parser.rs, commands.rs, ipc.rs), isolated changes with no cross-crate impact except kild-protocol's IpcConnection.               |
| Confidence | HIGH   | All hot paths traced end-to-end with file:line evidence; the issue is straightforward unnecessary I/O, not a subtle race condition or architectural flaw.         |

---

## Problem Statement

The tmux shim performs full file I/O (flock + JSON read + deserialize + validate) on every command invocation, including read-only commands like `send-keys` which only need a pane-to-session-id lookup. With agent teams running 5-10 panes, the exclusive flock on every `send-keys` serializes the most frequent operation. Additionally, the parser allocates owned `String` values for every flag when `&str` references would suffice, and `build_child_env()` rebuilds a HashMap from scratch on every pane creation.

---

## Analysis

### Root Cause / Change Rationale

This is not a regression — the shim was implemented with a simple load/save pattern that works correctly but doesn't differentiate between read-only and write operations.

### Evidence Chain

WHY: `send-keys` (the hottest command) is slow under concurrent agent teams
↓ BECAUSE: Every `send-keys` call acquires an exclusive flock and reads the full panes.json file
Evidence: `commands.rs:283` — `let registry = state::load(&sid)?;`

↓ BECAUSE: `state::load()` always acquires an exclusive flock, reads the entire file, deserializes, and validates
Evidence: `state.rs:128-144` — `acquire_lock(session_id)?` + `fs::read_to_string` + `serde_json::from_str` + `validate()`

↓ BECAUSE: There is no separate read-only code path — `load()` is the only way to access registry data
Evidence: `state.rs` has only `load()` and `save()`, no `load_shared()` or cached access

↓ ROOT CAUSE: All commands funnel through the same `state::load()` regardless of whether they modify state
Evidence: 7 read-only call sites (`commands.rs:283, 307, 421, 522, 634, 708`) all use the same exclusive-lock path as 8 write call sites

### Affected Files

| File                                      | Lines   | Action | Description                                                            |
| ----------------------------------------- | ------- | ------ | ---------------------------------------------------------------------- |
| `crates/kild-tmux-shim/src/state.rs`      | 97-161  | UPDATE | Add shared-lock `load_shared()` for read-only access                   |
| `crates/kild-tmux-shim/src/commands.rs`    | 279-737 | UPDATE | Switch read-only handlers to `state::load_shared()`                    |
| `crates/kild-tmux-shim/src/parser.rs`      | 26-150  | UPDATE | Change arg structs from `String` to lifetime-annotated `&str`          |
| `crates/kild-tmux-shim/src/parser.rs`      | 152-581 | UPDATE | Remove `.to_string()` calls, return borrowed references                |
| `crates/kild-tmux-shim/src/commands.rs`    | 72-114  | UPDATE | Cache `build_child_env()` result with `OnceLock`                       |
| `crates/kild-tmux-shim/src/ipc.rs`         | 65-88   | UPDATE | Pre-size `Vec<u8>` for base64, reuse write buffer via thread-local     |

### Integration Points

- `state::load()` is called from 15 sites in `commands.rs` — all must be audited for read-only vs write
- `parser::parse()` is called from `main.rs:28` — return type change cascades to `commands::execute()`
- `ipc::write_stdin()` is called from `commands.rs:293` — the `send-keys` hot path
- `kild-protocol::IpcConnection::send()` at `client.rs:96-129` — serialization overhead is downstream

### Git History

- **Introduced**: `71f3343a` — 2026-02-10 — "feat: add kild-tmux-shim crate for agent team support in daemon sessions (#301)"
- **Last significant change**: `b97d6ff` — "refactor: extract shared IPC client into kild-protocol (#461)"
- **Implication**: Original implementation, not a regression. The simple pattern was fine for initial implementation but doesn't scale to high-frequency agent team usage.

---

## Implementation Plan

### Step 1: Add shared-lock read-only `load_shared()` to state.rs

**File**: `crates/kild-tmux-shim/src/state.rs`
**Lines**: 97-144
**Action**: UPDATE

**Current code:**

```rust
// Line 97-126: acquire_lock always uses LockExclusive
fn acquire_lock(session_id: &str) -> Result<Flock<fs::File>, ShimError> {
    // ...
    Flock::lock(lock_file, FlockArg::LockExclusive).map_err(|(_, e)| ShimError::StateError {
        message: format!("failed to acquire lock: {}", e),
    })
}

// Line 128-144: load() always acquires exclusive lock
pub fn load(session_id: &str) -> Result<PaneRegistry, ShimError> {
    let data_path = panes_path(session_id)?;
    let _lock = acquire_lock(session_id)?;
    // ... read, parse, validate
}
```

**Required change:**

```rust
fn acquire_lock(session_id: &str, shared: bool) -> Result<Flock<fs::File>, ShimError> {
    let lock = lock_path(session_id)?;
    if let Some(parent) = lock.parent() {
        fs::create_dir_all(parent).map_err(|e| ShimError::StateError {
            message: format!(
                "failed to create state directory {}: {}",
                parent.display(),
                e
            ),
        })?;
    }
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock)
        .map_err(|e| ShimError::StateError {
            message: format!("failed to open lock file {}: {}", lock.display(), e),
        })?;

    let arg = if shared {
        FlockArg::LockShared
    } else {
        FlockArg::LockExclusive
    };
    Flock::lock(lock_file, arg).map_err(|(_, e)| ShimError::StateError {
        message: format!("failed to acquire lock: {}", e),
    })
}

/// Load the registry with a shared (read-only) lock.
/// Multiple readers can hold shared locks concurrently.
pub fn load_shared(session_id: &str) -> Result<PaneRegistry, ShimError> {
    let data_path = panes_path(session_id)?;
    let _lock = acquire_lock(session_id, true)?;

    let content = fs::read_to_string(&data_path).map_err(|e| ShimError::StateError {
        message: format!("failed to read {}: {}", data_path.display(), e),
    })?;

    let registry: PaneRegistry =
        serde_json::from_str(&content).map_err(|e| ShimError::StateError {
            message: format!("failed to parse pane registry: {}", e),
        })?;

    registry.validate()?;

    Ok(registry)
}

/// Load the registry with an exclusive (write) lock.
pub fn load(session_id: &str) -> Result<PaneRegistry, ShimError> {
    let data_path = panes_path(session_id)?;
    let _lock = acquire_lock(session_id, false)?;

    let content = fs::read_to_string(&data_path).map_err(|e| ShimError::StateError {
        message: format!("failed to read {}: {}", data_path.display(), e),
    })?;

    let registry: PaneRegistry =
        serde_json::from_str(&content).map_err(|e| ShimError::StateError {
            message: format!("failed to parse pane registry: {}", e),
        })?;

    registry.validate()?;

    Ok(registry)
}
```

**Why**: Shared locks allow concurrent readers. `send-keys`, `list-panes`, `display-message`, `has-session`, `list-windows`, and `capture-pane` only read state — they should not block each other. This is the single biggest performance win.

---

### Step 2: Switch read-only command handlers to `load_shared()`

**File**: `crates/kild-tmux-shim/src/commands.rs`
**Lines**: Multiple handlers
**Action**: UPDATE

**Commands to switch from `state::load()` to `state::load_shared()`:**

| Handler                | Line | Reason                                      |
| ---------------------- | ---- | ------------------------------------------- |
| `handle_send_keys`     | 283  | Only reads `pane.daemon_session_id`          |
| `handle_list_panes`    | 307  | Only iterates panes for display              |
| `handle_display_message` | 421 | Only reads session/window/pane names         |
| `handle_has_session`   | 522  | Only checks session existence                |
| `handle_list_windows`  | 634  | Only iterates windows for display            |
| `handle_capture_pane`  | 708  | Only reads `pane.daemon_session_id`          |

**Current code (send-keys example):**

```rust
fn handle_send_keys(args: SendKeysArgs) -> Result<i32, ShimError> {
    // ...
    let registry = state::load(&sid)?;
    // ... only reads, never calls state::save()
}
```

**Required change:**

```rust
fn handle_send_keys(args: SendKeysArgs) -> Result<i32, ShimError> {
    // ...
    let registry = state::load_shared(&sid)?;
    // ... only reads, never calls state::save()
}
```

**Why**: These 6 handlers never call `state::save()`. Using shared locks lets them run concurrently instead of serializing behind exclusive locks.

---

### Step 3: Make parser args zero-copy with lifetimes

**File**: `crates/kild-tmux-shim/src/parser.rs`
**Lines**: 26-150 (arg structs), 152-581 (parse functions)
**Action**: UPDATE

**Current code (example struct + parser):**

```rust
// Line 36-40
#[derive(Debug)]
pub struct SendKeysArgs {
    pub target: Option<String>,
    pub keys: Vec<String>,
}

// Line 237-257
fn parse_send_keys(args: &[&str]) -> Result<TmuxCommand, ShimError> {
    let mut target = None;
    let mut keys = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-t" => target = Some(take_value(args, &mut i)?.to_string()),
            _ => keys.push(args[i].to_string()),
        }
        i += 1;
    }
    Ok(TmuxCommand::SendKeys(SendKeysArgs { target, keys }))
}
```

**Required change:**

```rust
#[derive(Debug)]
pub struct SendKeysArgs<'a> {
    pub target: Option<&'a str>,
    pub keys: Vec<&'a str>,
}

fn parse_send_keys<'a>(args: &[&'a str]) -> Result<TmuxCommand<'a>, ShimError> {
    let mut target = None;
    let mut keys = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-t" => target = Some(take_value(args, &mut i)?),
            _ => keys.push(args[i]),
        }
        i += 1;
    }
    Ok(TmuxCommand::SendKeys(SendKeysArgs { target, keys }))
}
```

**Apply this pattern to all 16 arg structs and all 15 parse functions.**

The `TmuxCommand` enum and all `*Args` structs get a lifetime parameter `<'a>`:

```rust
pub enum TmuxCommand<'a> {
    Version,
    SplitWindow(SplitWindowArgs<'a>),
    SendKeys(SendKeysArgs<'a>),
    // ... etc
}
```

This cascades to `commands::execute()` signature:

```rust
pub fn execute(cmd: TmuxCommand<'_>) -> Result<i32, ShimError> {
```

And `main.rs::run()`:

```rust
fn run(args: &[String]) -> Result<i32, errors::ShimError> {
    let cmd = parser::parse(args)?;
    commands::execute(cmd)
}
```

The lifetime naturally works because `args` outlives `cmd` within `run()`.

**Structs where fields are consumed by move into owned state** (e.g., `select_pane` writes `title` into `PaneEntry`): these handlers convert at the call site with `.to_string()` only when actually writing to state.

**Why**: Eliminates ~35 `.to_string()` allocations per command. The parser output is consumed immediately within `run()` — no need for owned strings.

---

### Step 4: Cache `build_child_env()` with `OnceLock`

**File**: `crates/kild-tmux-shim/src/commands.rs`
**Lines**: 72-114
**Action**: UPDATE

**Current code:**

```rust
fn build_child_env() -> HashMap<String, String> {
    let copy_vars = ["PATH", "HOME", "SHELL", "USER", "LANG", "TERM"];
    let mut env_vars: HashMap<String, String> = copy_vars
        .iter()
        .filter_map(|&key| env::var(key).ok().map(|val| (key.to_string(), val)))
        .collect();
    // ... PATH prefix, TMUX propagation, KILD_SHIM_SESSION propagation
    env_vars
}
```

**Required change:**

```rust
use std::sync::OnceLock;

/// Base environment template — computed once, cloned per invocation.
/// Only dynamic part (TMUX_PANE) is added at the call site.
fn base_child_env() -> &'static HashMap<String, String> {
    static ENV: OnceLock<HashMap<String, String>> = OnceLock::new();
    ENV.get_or_init(|| {
        let copy_vars = ["PATH", "HOME", "SHELL", "USER", "LANG", "TERM"];
        let mut env_vars: HashMap<String, String> = copy_vars
            .iter()
            .filter_map(|&key| env::var(key).ok().map(|val| (key.to_string(), val)))
            .collect();

        // Ensure ~/.kild/bin is at the front of PATH
        match KildPaths::resolve() {
            Ok(paths) => {
                let kild_bin = paths.bin_dir();
                let current_path = env_vars.get("PATH").cloned().unwrap_or_default();
                let kild_bin_str = kild_bin.to_string_lossy();
                let already_present = current_path
                    .split(':')
                    .any(|component| component == kild_bin_str.as_ref());
                if !already_present {
                    env_vars.insert(
                        "PATH".to_string(),
                        format!("{}:{}", kild_bin_str, current_path),
                    );
                }
            }
            Err(e) => {
                error!(
                    event = "shim.split_window.path_resolution_failed",
                    error = %e,
                );
            }
        }

        if let Ok(tmux) = env::var("TMUX") {
            env_vars.insert("TMUX".to_string(), tmux);
        }
        if let Ok(sid) = env::var("KILD_SHIM_SESSION") {
            env_vars.insert("KILD_SHIM_SESSION".to_string(), sid);
        }

        env_vars
    })
}

fn build_child_env() -> HashMap<String, String> {
    base_child_env().clone()
}
```

**Why**: Environment variables don't change during the shim's lifetime (it's a short-lived process per command). However, `split-window` might be called multiple times if multiple panes are created in one session. The `OnceLock` ensures the HashMap is built once. The clone at the call site is still needed because `create_pty_pane` inserts `TMUX_PANE` per-pane. This is a minor optimization since `split-window` is infrequent compared to `send-keys`.

---

### Step 5: Pre-allocate IPC write buffer

**File**: `crates/kild-tmux-shim/src/ipc.rs`
**Lines**: 65-88
**Action**: UPDATE

**Current code:**

```rust
pub fn write_stdin(session_id: &str, data: &[u8]) -> Result<(), ShimError> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    let request = ClientMessage::WriteStdin {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: SessionId::new(session_id),
        data: encoded,
    };
    let mut conn = IpcConnection::connect(&socket_path()?)?;
    conn.send(&request)?;
    Ok(())
}
```

**Required change:**

```rust
pub fn write_stdin(session_id: &str, data: &[u8]) -> Result<(), ShimError> {
    // Pre-size the base64 buffer: base64 output is ceil(input_len / 3) * 4
    let encoded_len = (data.len() + 2) / 3 * 4;
    let mut encoded = String::with_capacity(encoded_len);
    base64::engine::general_purpose::STANDARD.encode_string(data, &mut encoded);

    let request = ClientMessage::WriteStdin {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: SessionId::new(session_id),
        data: encoded,
    };
    let mut conn = IpcConnection::connect(&socket_path()?)?;
    conn.send(&request)?;

    debug!(
        event = "shim.ipc.write_stdin_completed",
        session_id = session_id,
    );
    Ok(())
}
```

**Why**: `encode_string()` writes into a pre-sized buffer instead of allocating internally. Minor win but it's on the hot path.

---

### Step 6: Add/update tests

**File**: `crates/kild-tmux-shim/src/state.rs` (tests section)
**Action**: UPDATE

**Test cases to add:**

```rust
#[test]
fn test_load_shared_concurrent_reads() {
    let test_id = format!("test-{}", uuid::Uuid::new_v4());
    init_registry(&test_id, "daemon-abc-123").unwrap();

    // Two concurrent shared loads should not block each other
    let reg1 = load_shared(&test_id).unwrap();
    let reg2 = load_shared(&test_id).unwrap();
    assert_eq!(reg1.panes.len(), reg2.panes.len());

    let dir = state_dir(&test_id).unwrap();
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_load_shared_returns_valid_registry() {
    let test_id = format!("test-{}", uuid::Uuid::new_v4());
    init_registry(&test_id, "daemon-abc-123").unwrap();

    let registry = load_shared(&test_id).unwrap();
    assert_eq!(registry.next_pane_id, 1);
    assert_eq!(registry.panes.len(), 1);
    assert_eq!(registry.panes["%0"].daemon_session_id, "daemon-abc-123");

    let dir = state_dir(&test_id).unwrap();
    fs::remove_dir_all(&dir).ok();
}
```

**File**: `crates/kild-tmux-shim/src/parser.rs` (tests section)
**Action**: UPDATE — existing 71 parser tests should continue to pass after lifetime changes. The test input construction (`Vec<String>`) remains the same since `parse()` takes `&[String]`. Assertions using `.as_deref()` become direct `Option<&str>` comparisons.

---

## Patterns to Follow

**From codebase — shared vs exclusive lock pattern:**

```rust
// SOURCE: nix::fcntl already supports FlockArg::LockShared
// The existing acquire_lock() uses FlockArg::LockExclusive
// LockShared allows multiple concurrent readers
use nix::fcntl::FlockArg;
// FlockArg::LockShared — multiple processes can hold simultaneously
// FlockArg::LockExclusive — only one process at a time
```

**From codebase — OnceLock pattern (standard library):**

```rust
// std::sync::OnceLock is the standard approach for lazy static initialization
// Already used in Rust ecosystem for this exact pattern
use std::sync::OnceLock;
static CACHE: OnceLock<T> = OnceLock::new();
```

**From codebase — lifetime annotations in kild-protocol:**

```rust
// SOURCE: crates/kild-protocol/src/types.rs
// The newtype_string! macro shows the project's convention for owned vs borrowed:
// SessionId wraps String (owned) because it crosses IPC boundaries
// Parser args don't cross boundaries — &str is appropriate
```

---

## Edge Cases & Risks

| Risk/Edge Case                                 | Mitigation                                                                                         |
| ---------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| Shared lock doesn't prevent stale reads        | Acceptable: read-only commands see the state at time of read. A split-window during send-keys would show the new pane on next list-panes. No correctness issue. |
| Lifetime annotations cascade through entire API | The cascade is contained: `parse()` → `TmuxCommand` → `execute()`, all within `run()` scope. The `args: Vec<String>` in `main()` outlives everything. |
| `OnceLock` env cache stale if env changes       | The shim is a short-lived process (one command per invocation). Env doesn't change during execution. |
| `base64::encode_string` API availability        | Available since base64 0.21. Check Cargo.toml version. If not available, keep current `encode()`. |
| Parser test updates for lifetime changes        | All 71 parser tests already use `.as_deref()` for comparison — they naturally work with `Option<&str>` fields. |
| Write commands that consume parsed string args  | Handlers like `select_pane` that write `args.title` into `PaneEntry` call `.to_string()` at the write site only. |

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

1. Run `cargo test -p kild-tmux-shim` to verify all 110 existing tests pass
2. Create a daemon kild session with `cargo run -p kild -- create test-perf --daemon`
3. Verify `send-keys` still works: shim should use shared lock path
4. Verify `split-window` still works: should use exclusive lock path
5. Stop and destroy: `cargo run -p kild -- destroy test-perf --force`

---

## Scope Boundaries

**IN SCOPE:**

- Shared-lock read-only path for state access (Step 1-2)
- Zero-copy parser with lifetime annotations (Step 3)
- Cached environment template (Step 4)
- Pre-sized IPC buffer (Step 5)
- Tests for new `load_shared()` (Step 6)

**OUT OF SCOPE (do not touch):**

- In-memory registry cache with file watcher (mentioned in issue — future optimization, higher complexity)
- IPC connection pooling (would require architectural changes to IpcConnection)
- Thread-local reusable buffers for IPC serialization (marginal gain, adds complexity)
- Changes to kild-protocol's `IpcConnection::send()` (shared crate, different scope)
- UUID generation optimization (marginal, would need protocol changes)
- `serde_json::to_string_pretty` → `serde_json::to_string` for save (changes debug experience)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-17T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-476.md`
