# Investigation: kild attach shows no error message on failure

**Issue**: #298 (https://github.com/Wirasm/kild/issues/298)
**Type**: BUG
**Investigated**: 2026-02-10T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                     |
| ---------- | ------ | --------------------------------------------------------------------------------------------- |
| Severity   | HIGH   | Every attach failure is completely silent - user gets exit code 1 with zero guidance           |
| Complexity | LOW    | Single file change (attach.rs), well-established pattern to mirror from other command handlers |
| Confidence | HIGH   | Root cause is unambiguous: `?` propagation without `eprintln!`, verified in source code        |

---

## Problem Statement

When `kild attach` fails for any reason (session not found, not daemon-managed, daemon not running, daemon rejects), the CLI exits with code 1 but prints **nothing** to stderr. The user has no idea what went wrong. This violates the established pattern where all other command handlers print user-facing error messages via `eprintln!` before returning errors.

---

## Analysis

### Root Cause

**WHY 1**: Why does `kild attach nonexistent` show no error?
- Because `main.rs:15-21` drops the error with `drop(e)`, trusting that "Error already printed to user via eprintln! in command handlers."
- Evidence: `crates/kild/src/main.rs:15-21`

**WHY 2**: Why doesn't main.rs print the error itself?
- Because the convention is that each command handler prints errors before returning them (see stop.rs, create.rs, focus.rs, destroy.rs, open.rs).
- Evidence: `crates/kild/src/commands/stop.rs:30-35` - uses `eprintln!` then returns `Err`

**WHY 3**: Why doesn't attach.rs print errors?
- Because `handle_attach_command` uses bare `?` operator on all three error points without any `eprintln!` calls.
- Evidence: `crates/kild/src/commands/attach.rs:18` (`get_session(branch)?`), line 28 (`.ok_or_else(...)?`), line 38 (`attach_to_daemon_session(...)?`)

**ROOT CAUSE**: `attach.rs` propagates errors via `?` without printing them, violating the established `eprintln!` + `error!` + `events::log_app_error` pattern used by every other command handler.

### Evidence Chain

```
User: kild attach nonexistent
  -> main.rs:15 calls run_command
  -> commands/mod.rs:63 routes to attach::handle_attach_command
  -> attach.rs:18 calls get_session("nonexistent") -> Err(SessionError::NotFound)
  -> ? propagates to main.rs:15
  -> main.rs:19 drop(e) -- error silently discarded
  -> main.rs:20 exit(1) -- user sees nothing
```

### Affected Files

| File                                       | Lines  | Action | Description                                              |
| ------------------------------------------ | ------ | ------ | -------------------------------------------------------- |
| `crates/kild/src/commands/attach.rs`       | 8-42   | UPDATE | Add error printing to `handle_attach_command`            |
| `crates/kild/src/commands/attach.rs`       | 44-107 | UPDATE | Add error printing to `attach_to_daemon_session`         |

### Integration Points

- `crates/kild/src/main.rs:15-21` - Relies on command handlers printing errors before returning
- `crates/kild/src/commands/mod.rs:63` - Routes to attach handler
- `crates/kild-core/src/sessions/handler.rs:401` - `get_session()` returns `SessionError`
- `crates/kild-core/src/daemon/client.rs:21-38` - `DaemonClientError` types

### Git History

- **Introduced**: `6f1cfa7` - "feat: add kild-daemon crate with PTY ownership, IPC server, and session persistence (#294)"
- **main.rs pattern**: `e9d07bc` - 2026-02-09 - established the `drop(e)` convention
- **Implication**: Original bug - attach was added in the daemon feature without the error printing convention

---

## Implementation Plan

### Step 1: Refactor `handle_attach_command` to use match pattern for session lookup

**File**: `crates/kild/src/commands/attach.rs`
**Lines**: 1-42
**Action**: UPDATE

**Current code:**

```rust
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use clap::ArgMatches;
use nix::sys::termios;
use tracing::{error, info, warn};

pub(crate) fn handle_attach_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    info!(event = "cli.attach_started", branch = branch);

    // 1. Look up session to get daemon_session_id
    let session = kild_core::session_ops::get_session(branch)?;

    let daemon_session_id = session
        .latest_agent()
        .and_then(|a| a.daemon_session_id())
        .ok_or_else(|| {
            format!(
                "Session '{}' is not daemon-managed. Use 'kild focus {}' for terminal sessions.",
                branch, branch
            )
        })?
        .to_string();

    info!(
        event = "cli.attach_connecting",
        branch = branch,
        daemon_session_id = daemon_session_id.as_str()
    );

    // 2. Connect to daemon and attach
    attach_to_daemon_session(&daemon_session_id)?;

    info!(event = "cli.attach_completed", branch = branch);
    Ok(())
}
```

**Required change:**

```rust
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use clap::ArgMatches;
use nix::sys::termios;
use tracing::{error, info, warn};

use kild_core::events;

pub(crate) fn handle_attach_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    info!(event = "cli.attach_started", branch = branch);

    // 1. Look up session to get daemon_session_id
    let session = match kild_core::session_ops::get_session(branch) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Session '{}' not found.", branch);
            eprintln!("Tip: Use 'kild list' to see active sessions.");
            error!(event = "cli.attach_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    let daemon_session_id = match session
        .latest_agent()
        .and_then(|a| a.daemon_session_id())
    {
        Some(id) => id.to_string(),
        None => {
            let msg = format!(
                "Session '{}' is not daemon-managed. Use 'kild focus {}' for terminal sessions.",
                branch, branch
            );
            eprintln!("Error: {}", msg);
            error!(event = "cli.attach_failed", branch = branch, error = msg.as_str());
            return Err(msg.into());
        }
    };

    info!(
        event = "cli.attach_connecting",
        branch = branch,
        daemon_session_id = daemon_session_id.as_str()
    );

    // 2. Connect to daemon and attach
    if let Err(e) = attach_to_daemon_session(&daemon_session_id) {
        eprintln!("Error: {}", e);
        error!(event = "cli.attach_failed", branch = branch, error = %e);
        return Err(e);
    }

    info!(event = "cli.attach_completed", branch = branch);
    Ok(())
}
```

**Why**: Follows the established pattern from stop.rs, focus.rs, destroy.rs, open.rs - every error path gets an `eprintln!` before returning. Adds contextual tips for each failure mode as specified in the issue.

### Step 2: Improve daemon connection error message in `attach_to_daemon_session`

**File**: `crates/kild/src/commands/attach.rs`
**Lines**: 46-52
**Action**: UPDATE

**Current code:**

```rust
let mut stream = UnixStream::connect(&socket_path).map_err(|e| {
    format!(
        "Cannot connect to daemon at {}: {}",
        socket_path.display(),
        e
    )
})?;
```

**Required change:**

```rust
let mut stream = UnixStream::connect(&socket_path).map_err(|e| {
    format!(
        "Cannot connect to daemon at {}: {}\nTip: Start the daemon with 'kild daemon start'.",
        socket_path.display(),
        e
    )
})?;
```

**Why**: The daemon connection error gets printed by the `if let Err(e)` block in Step 1. Including the tip in the error message itself keeps the logic simple.

---

## Patterns to Follow

**From codebase - mirror the stop.rs pattern exactly:**

```rust
// SOURCE: crates/kild/src/commands/stop.rs:23-36
match session_ops::stop_session(branch) {
    Ok(()) => {
        println!("Stopped kild '{}'", branch);
        println!("   KILD preserved. Use 'kild open {}' to restart.", branch);
        info!(event = "cli.stop_completed", branch = branch);
        Ok(())
    }
    Err(e) => {
        eprintln!("Failed to stop kild '{}': {}", branch, e);
        error!(event = "cli.stop_failed", branch = branch, error = %e);
        events::log_app_error(&e);
        Err(e.into())
    }
}
```

**From codebase - focus.rs session lookup pattern:**

```rust
// SOURCE: crates/kild/src/commands/focus.rs:15-23
let session = match session_ops::get_session(branch) {
    Ok(s) => s,
    Err(e) => {
        eprintln!("Failed to find kild '{}': {}", branch, e);
        error!(event = "cli.focus_failed", branch = branch, error = %e);
        events::log_app_error(&e);
        return Err(e.into());
    }
};
```

---

## Edge Cases & Risks

| Risk/Edge Case                        | Mitigation                                                      |
| ------------------------------------- | --------------------------------------------------------------- |
| No double-printing from other layers  | Verified: main.rs drops errors, no other layer prints            |
| events::log_app_error type mismatch   | For non-SessionError paths, skip log_app_error (only for String) |
| Runtime errors during active session   | Already handled correctly (lines 165, 181, 187, 192) - no change needed |

---

## Validation

### Automated Checks

```bash
cargo fmt --check && cargo clippy --all -- -D warnings && cargo test --all && cargo build --all
```

### Manual Verification

1. `kild attach nonexistent-branch` - should print "Error: Session 'nonexistent-branch' not found." + tip
2. `kild attach terminal-session` (non-daemon session) - should print "Error: Session 'X' is not daemon-managed." + tip
3. `kild attach my-session` (daemon not running) - should print "Cannot connect to daemon" + tip
4. Verify normal attach flow still works correctly

---

## Scope Boundaries

**IN SCOPE:**

- Add `eprintln!` error messages to all error paths in `handle_attach_command`
- Add `error!` structured logging to all error paths
- Add daemon connection tip to `attach_to_daemon_session` error
- Add `use kild_core::events;` import

**OUT OF SCOPE (do not touch):**

- Runtime error handling inside active session (lines 153-273) - already correct
- Other commands that may have similar issues - separate audit issue
- Changing main.rs `drop(e)` pattern (Option B from issue) - deferred
- Adding tests for CLI error output

---

## Related Finding: Daemon has no log output in background mode

**Verified on running daemon PID 30391**: `lsof` confirms fd 2 (stderr) → `/dev/null`.

The daemon produces extensive tracing output (`info!`, `error!` in `server/mod.rs`, `session/manager.rs`, `pty/mod.rs`), but when started in background mode (the default), `daemon.rs:47-48` pipes both stdout and stderr to `/dev/null`:

```rust
.stdout(std::process::Stdio::null())
.stderr(std::process::Stdio::null())
```

Since `init_logging()` writes to stderr (`logging/mod.rs:14`), all daemon logs are lost. This makes it impossible to debug daemon issues (like the attach race condition where PTY exits before attach connects). This should be tracked as a separate issue - the daemon needs file-based logging (e.g., `~/.kild/daemon.log`).

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-10T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-298.md`
