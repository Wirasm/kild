# Investigation: DaemonClientError doesn't implement KildError trait

**Issue**: #332 (https://github.com/Wirasm/kild/issues/332)
**Type**: BUG
**Investigated**: 2026-02-11

### Assessment

| Metric     | Value  | Reasoning                                                                                              |
| ---------- | ------ | ------------------------------------------------------------------------------------------------------ |
| Severity   | MEDIUM | Error handling still works via manual `.to_string()` conversion; no runtime crash, but breaks documented pattern |
| Complexity | LOW    | Single file change + tests; isolated to `daemon/client.rs` with no cascading structural changes        |
| Confidence | HIGH   | Clear root cause, exact code location known, well-established pattern to mirror from 14 other impls    |

---

## Problem Statement

`DaemonClientError` at `crates/kild-core/src/daemon/client.rs:23` is the only error type in kild-core that does NOT implement the `KildError` trait. Every other domain error type (14 total) implements `KildError` with `error_code()` and `is_user_error()` methods. This breaks the documented error handling contract from CLAUDE.md.

---

## Analysis

### Root Cause

The `DaemonClientError` type was introduced in commit `6f1cfa7` (feat: add kild-daemon crate) and the `KildError` implementation was simply never added.

### Evidence Chain

WHY: `DaemonClientError` doesn't implement `KildError`
↓ BECAUSE: The type was added without the trait impl in the initial daemon crate commit
Evidence: `crates/kild-core/src/daemon/client.rs:22-38` — only derives `Debug` and `thiserror::Error`

↓ ROOT CAUSE: Missing `impl KildError for DaemonClientError` block
Evidence: Every other error type in the codebase has this impl (14 implementations across kild-core + kild-daemon)

### Affected Files

| File                                        | Lines  | Action | Description                            |
| ------------------------------------------- | ------ | ------ | -------------------------------------- |
| `crates/kild-core/src/daemon/client.rs`     | 38+    | UPDATE | Add `KildError` impl after error enum  |

### Integration Points

- `crates/kild-core/src/sessions/handler.rs:340,893,1098` — converts `DaemonClientError` to `SessionError::DaemonError` via `.to_string()` (loses error code info)
- `crates/kild/src/commands/daemon.rs:110` — pattern matches on `DaemonClientError::NotRunning`
- `crates/kild-core/src/daemon/autostart.rs:20` — logs error but doesn't propagate

### Git History

- **Introduced**: `6f1cfa7` — "feat: add kild-daemon crate with PTY ownership, IPC server, and session persistence"
- **Last modified**: `dee5ff2` — "fix: detect early PTY exit in daemon sessions"
- **Implication**: Original omission, not a regression

---

## Implementation Plan

### Step 1: Add KildError import and implementation

**File**: `crates/kild-core/src/daemon/client.rs`
**Lines**: After line 38 (after the enum definition)
**Action**: UPDATE

**Current code:**
```rust
// Line 22-38
/// Error communicating with the daemon.
#[derive(Debug, thiserror::Error)]
pub enum DaemonClientError {
    #[error("Daemon is not running (socket not found at {path})")]
    NotRunning { path: String },

    #[error("Connection failed: {message}")]
    ConnectionFailed { message: String },

    #[error("Daemon returned error: {message}")]
    DaemonError { message: String },

    #[error("IPC protocol error: {message}")]
    ProtocolError { message: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

**Required change:**

Add import for `KildError` at the top of the file and add the trait impl block after the enum:

```rust
use crate::errors::KildError;
```

```rust
impl KildError for DaemonClientError {
    fn error_code(&self) -> &'static str {
        match self {
            DaemonClientError::NotRunning { .. } => "DAEMON_NOT_RUNNING",
            DaemonClientError::ConnectionFailed { .. } => "DAEMON_CONNECTION_FAILED",
            DaemonClientError::DaemonError { .. } => "DAEMON_ERROR",
            DaemonClientError::ProtocolError { .. } => "DAEMON_PROTOCOL_ERROR",
            DaemonClientError::Io(_) => "DAEMON_IO_ERROR",
        }
    }

    fn is_user_error(&self) -> bool {
        // NotRunning is user-actionable: user needs to start the daemon
        matches!(self, DaemonClientError::NotRunning { .. })
    }
}
```

**Why**: Follows the established pattern. Error codes use `DAEMON_*` prefix consistent with `DaemonAutoStartError`. `NotRunning` is the only user error — the user can fix it by starting the daemon. `ConnectionFailed`, `DaemonError`, `ProtocolError`, and `Io` are system/infrastructure errors.

Note: `DAEMON_ERROR` is already used by `SessionError::DaemonError` in `sessions/errors.rs:123`. This is acceptable — `SessionError::DaemonError` wraps `DaemonClientError` with `.to_string()`, so they represent the same underlying issue at different abstraction levels.

### Step 2: Add tests

**File**: `crates/kild-core/src/daemon/client.rs`
**Action**: UPDATE — add test module at end of file

**Test cases to add:**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        assert_eq!(
            DaemonClientError::NotRunning {
                path: "/tmp/test.sock".to_string()
            }
            .error_code(),
            "DAEMON_NOT_RUNNING"
        );
        assert_eq!(
            DaemonClientError::ConnectionFailed {
                message: "refused".to_string()
            }
            .error_code(),
            "DAEMON_CONNECTION_FAILED"
        );
        assert_eq!(
            DaemonClientError::DaemonError {
                message: "internal".to_string()
            }
            .error_code(),
            "DAEMON_ERROR"
        );
        assert_eq!(
            DaemonClientError::ProtocolError {
                message: "bad json".to_string()
            }
            .error_code(),
            "DAEMON_PROTOCOL_ERROR"
        );
        assert_eq!(
            DaemonClientError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test"
            ))
            .error_code(),
            "DAEMON_IO_ERROR"
        );
    }

    #[test]
    fn test_is_user_error() {
        assert!(DaemonClientError::NotRunning {
            path: "/tmp/test.sock".to_string()
        }
        .is_user_error());

        assert!(!DaemonClientError::ConnectionFailed {
            message: "refused".to_string()
        }
        .is_user_error());
        assert!(!DaemonClientError::DaemonError {
            message: "internal".to_string()
        }
        .is_user_error());
        assert!(!DaemonClientError::ProtocolError {
            message: "bad json".to_string()
        }
        .is_user_error());
        assert!(!DaemonClientError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "test"
        ))
        .is_user_error());
    }
}
```

**Why**: Mirrors test patterns from `daemon/errors.rs:39-94` and `errors/mod.rs:91-136`. Tests every variant for both `error_code()` and `is_user_error()`.

---

## Patterns to Follow

**From codebase — mirror these exactly:**

```rust
// SOURCE: crates/kild-core/src/daemon/errors.rs:24-37
// Pattern for KildError impl in daemon domain
impl KildError for DaemonAutoStartError {
    fn error_code(&self) -> &'static str {
        match self {
            DaemonAutoStartError::Disabled => "DAEMON_AUTO_START_DISABLED",
            DaemonAutoStartError::SpawnFailed { .. } => "DAEMON_SPAWN_FAILED",
            DaemonAutoStartError::Timeout { .. } => "DAEMON_AUTO_START_TIMEOUT",
            DaemonAutoStartError::BinaryNotFound { .. } => "DAEMON_BINARY_NOT_FOUND",
        }
    }

    fn is_user_error(&self) -> bool {
        matches!(self, DaemonAutoStartError::Disabled)
    }
}
```

```rust
// SOURCE: crates/kild-core/src/daemon/errors.rs:39-94
// Test pattern for KildError in daemon domain
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kild_error_codes() {
        assert_eq!(
            DaemonAutoStartError::Disabled.error_code(),
            "DAEMON_AUTO_START_DISABLED"
        );
        // ... each variant tested
    }

    #[test]
    fn test_is_user_error() {
        assert!(DaemonAutoStartError::Disabled.is_user_error());
        assert!(!DaemonAutoStartError::SpawnFailed { message: "test".to_string() }.is_user_error());
        // ... each variant tested
    }
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                                     | Mitigation                                                                      |
| -------------------------------------------------- | ------------------------------------------------------------------------------- |
| `DAEMON_ERROR` code collision with SessionError    | Acceptable — different abstraction levels, same underlying issue                |
| Test file already has tests at bottom              | Check for existing `#[cfg(test)]` module before adding                          |

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

1. Verify `DaemonClientError` now implements `KildError` by checking compile
2. Verify error codes match `DAEMON_*` prefix convention
3. Verify tests cover all 5 variants for both methods

---

## Scope Boundaries

**IN SCOPE:**
- Adding `KildError` impl for `DaemonClientError`
- Adding tests for the new impl

**OUT OF SCOPE (do not touch):**
- Refactoring `SessionError::DaemonError` to use `DaemonClientError` directly (separate improvement)
- Adding `From<DaemonClientError>` for `SessionError` (separate improvement)
- Moving `DaemonClientError` to `daemon/errors.rs` (unnecessary churn)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-11
- **Artifact**: `.claude/PRPs/issues/issue-332.md`
