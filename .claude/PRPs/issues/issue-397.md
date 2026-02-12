# Investigation: --json commands return non-JSON for nonexistent sessions

**Issue**: #397 (https://github.com/Wirasm/kild/issues/397)
**Type**: BUG
**Investigated**: 2026-02-12T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                     |
| ---------- | ------ | ------------------------------------------------------------------------------------------------------------- |
| Severity   | MEDIUM | JSON contract violation breaks automation/scripting but stderr+empty stdout means piping won't crash consumers |
| Complexity | LOW    | All changes are in the CLI layer's error branches; pattern is identical across 6 commands                      |
| Confidence | HIGH   | Clear root cause with exact file:line references; pattern is consistent and well-understood                    |

---

## Problem Statement

When `--json` commands fail (e.g., session not found), they output plain text error messages to stderr via `eprintln!` and produce empty stdout. The contract established in #369 (PR #382) requires that `--json` always returns valid JSON. Automation consumers that parse stdout get no output on failure, and the error message is unstructured text instead of a machine-readable JSON error object.

---

## Analysis

### Root Cause

Every command with `--json` support extracts the `json_output` flag early but **never checks it in error branches**. Error paths always use `eprintln!` with plain text, regardless of the flag value.

### Evidence Chain

WHY: `kild status nonexistent --json` outputs plain text to stderr instead of JSON to stdout
- BECAUSE: The error branch at `status.rs:236-247` uses `eprintln!` unconditionally
- Evidence: `crates/kild/src/commands/status.rs:237` - `eprintln!("❌ Failed to get status for kild '{}': {}", branch, e);`

WHY: The error branch doesn't check `json_output`
- BECAUSE: The `json_output` flag is only checked in the success path (line 40: `if json_output {`)
- Evidence: `crates/kild/src/commands/status.rs:19` - flag is extracted but never referenced in `Err(e)` branch at line 236

ROOT CAUSE: All 6 commands with `--json` follow the same pattern - `json_output` flag is parsed but only used in success paths. No JSON error output helper exists in the CLI layer.

### Affected Files

| File                                      | Lines   | Action | Description                                          |
| ----------------------------------------- | ------- | ------ | ---------------------------------------------------- |
| `crates/kild/src/commands/json_types.rs`  | NEW     | UPDATE | Add `JsonError` serializable struct                  |
| `crates/kild/src/commands/helpers.rs`     | NEW     | UPDATE | Add `print_json_error` helper function               |
| `crates/kild/src/commands/status.rs`      | 236-247 | UPDATE | Check `json_output` in error branch                  |
| `crates/kild/src/commands/list.rs`        | 156-166 | UPDATE | Check `json_output` in error branch                  |
| `crates/kild/src/commands/pr.rs`          | 16-20, 29-38 | UPDATE | Check `json_output` in error branches           |
| `crates/kild/src/commands/stats.rs`       | 35-39, 62-69, 103-111 | UPDATE | Check `json_output` in error branches  |
| `crates/kild/src/commands/health.rs`      | 84-88, 102-107, 126-131 | UPDATE | Check `json_output` in error branches |
| `crates/kild/tests/cli_json_output.rs`    | NEW     | UPDATE | Add tests for JSON error output                      |

### Integration Points

- `main.rs:15-21` - Catches errors from command handlers, drops them, exits with code 1. Comment says "Error already printed to user via eprintln! in command handlers." This behavior stays unchanged; JSON errors are printed by command handlers before returning.
- `KildError` trait (`kild-core/src/errors/mod.rs:4-12`) - Provides `error_code()` returning `&'static str` like `"SESSION_NOT_FOUND"`. Available on concrete error types (SessionError, ConfigError, etc.) but lost after `.into()` conversion to `Box<dyn std::error::Error>`.
- `SessionError::NotFound` (`kild-core/src/sessions/errors.rs:8-9`) - Error code `"SESSION_NOT_FOUND"`, display `"Session '{name}' not found"`.

### Git History

- **PR #382** introduced JSON output support for success cases (follow-up to #369)
- **bf29d51** (recent) refactored FleetSummary constructors and tests
- Error branches were never updated to handle JSON output

---

## Implementation Plan

### Step 1: Add `JsonError` struct to `json_types.rs`

**File**: `crates/kild/src/commands/json_types.rs`
**Action**: UPDATE (append)

**Required change:**

```rust
/// JSON error response for --json commands.
#[derive(Serialize)]
pub struct JsonError {
    pub error: String,
    pub code: String,
}
```

**Why**: Provides a serializable type for consistent JSON error output across all commands. Uses `String` for both fields since error codes come from `KildError::error_code()` as `&'static str` and need to be owned for the struct.

---

### Step 2: Add `print_json_error` helper to `helpers.rs`

**File**: `crates/kild/src/commands/helpers.rs`
**Action**: UPDATE (add function)

**Required change:**

```rust
use super::json_types::JsonError;

/// Print a JSON error object to stdout for --json mode.
/// Returns the error wrapped in Box for chaining with `?` or `return Err(...)`.
pub fn print_json_error(error: &dyn std::fmt::Display, code: &str) -> Box<dyn std::error::Error> {
    let json_err = JsonError {
        error: error.to_string(),
        code: code.to_string(),
    };
    // Unwrap is safe: JsonError serialization cannot fail (no maps with non-string keys)
    println!("{}", serde_json::to_string_pretty(&json_err).unwrap());
    error.to_string().into()
}
```

**Why**: Centralizes JSON error output so each command handler doesn't duplicate serialization logic. Takes `&dyn Display` + `&str` code to work with any error type. Returns `Box<dyn std::error::Error>` so callers can write `return Err(print_json_error(&e, e.error_code()))`.

**Note**: The function needs `serde_json` in scope. Check if `helpers.rs` already imports it; if not, add `use serde_json;` or inline the usage.

---

### Step 3: Update `status.rs` error branch

**File**: `crates/kild/src/commands/status.rs`
**Lines**: 236-247
**Action**: UPDATE

**Current code:**

```rust
Err(e) => {
    eprintln!("❌ Failed to get status for kild '{}': {}", branch, e);

    error!(
        event = "cli.status_failed",
        branch = branch,
        error = %e
    );

    events::log_app_error(&e);
    Err(e.into())
}
```

**Required change:**

```rust
Err(e) => {
    if json_output {
        let boxed = super::helpers::print_json_error(&e, e.error_code());
        error!(
            event = "cli.status_failed",
            branch = branch,
            error = %e
        );
        events::log_app_error(&e);
        return Err(boxed);
    }

    eprintln!("❌ Failed to get status for kild '{}': {}", branch, e);

    error!(
        event = "cli.status_failed",
        branch = branch,
        error = %e
    );

    events::log_app_error(&e);
    Err(e.into())
}
```

**Why**: When `json_output` is true, output structured JSON error to stdout. Keep plain text output for non-JSON mode. Logging and error propagation are preserved in both paths. Need to call `print_json_error` before converting `e` because `error_code()` is only available on the concrete `SessionError` type.

**Important**: The `use kild_core::errors::KildError;` import is needed at the top of `status.rs` to access `error_code()` method on `SessionError`.

---

### Step 4: Update `list.rs` error branch

**File**: `crates/kild/src/commands/list.rs`
**Lines**: 156-166
**Action**: UPDATE

**Current code:**

```rust
Err(e) => {
    eprintln!("❌ Failed to list kilds: {}", e);

    error!(
        event = "cli.list_failed",
        error = %e
    );

    events::log_app_error(&e);
    Err(e.into())
}
```

**Required change:**

```rust
Err(e) => {
    if json_output {
        let boxed = super::helpers::print_json_error(&e, e.error_code());
        error!(event = "cli.list_failed", error = %e);
        events::log_app_error(&e);
        return Err(boxed);
    }

    eprintln!("❌ Failed to list kilds: {}", e);

    error!(
        event = "cli.list_failed",
        error = %e
    );

    events::log_app_error(&e);
    Err(e.into())
}
```

**Why**: Same pattern as status. `list_sessions()` returns `SessionError` which implements `KildError`.

**Import needed**: `use kild_core::errors::KildError;`

---

### Step 5: Update `pr.rs` error branches

**File**: `crates/kild/src/commands/pr.rs`
**Lines**: 16-20, 29-38
**Action**: UPDATE

**Error branch 1 - invalid branch name (lines 16-20):**

**Current:**
```rust
if !is_valid_branch_name(branch) {
    eprintln!("Invalid branch name: {}", branch);
    error!(event = "cli.pr_invalid_branch", branch = branch);
    return Err("Invalid branch name".into());
}
```

**Required change:**
```rust
if !is_valid_branch_name(branch) {
    if json_output {
        let err_msg = format!("Invalid branch name: {}", branch);
        let boxed = super::helpers::print_json_error(&err_msg, "INVALID_BRANCH_NAME");
        error!(event = "cli.pr_invalid_branch", branch = branch);
        return Err(boxed);
    }
    eprintln!("Invalid branch name: {}", branch);
    error!(event = "cli.pr_invalid_branch", branch = branch);
    return Err("Invalid branch name".into());
}
```

**Error branch 2 - session not found (lines 29-38):**

**Current:**
```rust
let session = match session_ops::get_session(branch) {
    Ok(s) => s,
    Err(e) => {
        eprintln!("❌ Failed to find kild '{}': {}", branch, e);
        error!(event = "cli.pr_failed", branch = branch, error = %e);
        events::log_app_error(&e);
        return Err(e.into());
    }
};
```

**Required change:**
```rust
let session = match session_ops::get_session(branch) {
    Ok(s) => s,
    Err(e) => {
        if json_output {
            let boxed = super::helpers::print_json_error(&e, e.error_code());
            error!(event = "cli.pr_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            return Err(boxed);
        }
        eprintln!("❌ Failed to find kild '{}': {}", branch, e);
        error!(event = "cli.pr_failed", branch = branch, error = %e);
        events::log_app_error(&e);
        return Err(e.into());
    }
};
```

**Import needed**: `use kild_core::errors::KildError;`

---

### Step 6: Update `stats.rs` error branches

**File**: `crates/kild/src/commands/stats.rs`
**Lines**: 35-39, 62-69, 103-111
**Action**: UPDATE

**Error branch 1 - invalid branch (lines 35-39):**

```rust
if !is_valid_branch_name(branch) {
    if json_output {
        let err_msg = format!("Invalid branch name: {}", branch);
        let boxed = super::helpers::print_json_error(&err_msg, "INVALID_BRANCH_NAME");
        error!(event = "cli.stats_invalid_branch", branch = branch);
        return Err(boxed);
    }
    eprintln!("Invalid branch name: {}", branch);
    error!(event = "cli.stats_invalid_branch", branch = branch);
    return Err("Invalid branch name".into());
}
```

**Error branch 2 - session not found (lines 62-69):**

```rust
Err(e) => {
    if json_output {
        let boxed = super::helpers::print_json_error(&e, e.error_code());
        error!(event = "cli.stats_failed", branch = branch, error = %e);
        events::log_app_error(&e);
        return Err(boxed);
    }
    eprintln!("Failed to find kild '{}': {}", branch, e);
    error!(event = "cli.stats_failed", branch = branch, error = %e);
    events::log_app_error(&e);
    return Err(e.into());
}
```

**Error branch 3 - health unavailable (lines 103-111):**

```rust
Err(msg) => {
    if json_output {
        let err_msg = format!("Could not compute branch health for '{}': {}", branch, msg);
        let boxed = super::helpers::print_json_error(&err_msg, "HEALTH_UNAVAILABLE");
        error!(event = "cli.stats_failed", branch = branch, reason = "health_unavailable");
        return Err(boxed);
    }
    eprintln!("Could not compute branch health for '{}': {}", branch, msg);
    error!(
        event = "cli.stats_failed",
        branch = branch,
        reason = "health_unavailable"
    );
    Err(format!("Branch health unavailable for '{}'", branch).into())
}
```

**Import needed**: `use kild_core::errors::KildError;`

---

### Step 7: Update `health.rs` error branches

**File**: `crates/kild/src/commands/health.rs`
**Lines**: 84-88, 102-107, 126-131
**Action**: UPDATE

**Error branch 1 - invalid branch (lines 84-88):**

```rust
if !is_valid_branch_name(branch_name) {
    if json_output {
        let err_msg = format!("Invalid branch name: {}", branch_name);
        let boxed = super::helpers::print_json_error(&err_msg, "INVALID_BRANCH_NAME");
        error!(event = "cli.health_invalid_branch", branch = branch_name);
        return Err(boxed);
    }
    eprintln!("❌ Invalid branch name: {}", branch_name);
    error!(event = "cli.health_invalid_branch", branch = branch_name);
    return Err("Invalid branch name".into());
}
```

**Error branch 2 - single session health failed (lines 102-107):**

```rust
Err(e) => {
    if json_output {
        let boxed = super::helpers::print_json_error(&e, e.error_code());
        error!(event = "cli.health_failed", branch = branch_name, error = %e);
        events::log_app_error(&e);
        return Err(boxed);
    }
    eprintln!("❌ Failed to get health for kild '{}': {}", branch_name, e);
    error!(event = "cli.health_failed", branch = branch_name, error = %e);
    events::log_app_error(&e);
    Err(e.into())
}
```

**Error branch 3 - all sessions health failed (lines 126-131):**

```rust
Err(e) => {
    if json_output {
        let boxed = super::helpers::print_json_error(&e, e.error_code());
        error!(event = "cli.health_failed", error = %e);
        events::log_app_error(&e);
        return Err(boxed);
    }
    eprintln!("❌ Failed to get health status: {}", e);
    error!(event = "cli.health_failed", error = %e);
    events::log_app_error(&e);
    Err(e.into())
}
```

**Note**: `health.rs` error branches involve `health::get_health_single_session` and `health::get_health_all_sessions`. Need to verify these return types implement `KildError`. If they return `SessionError` (which implements `KildError`), the pattern works. If they return a different error type, may need to use a generic code like `"HEALTH_ERROR"`.

**Import needed**: `use kild_core::errors::KildError;`

---

### Step 8: Add tests for JSON error output

**File**: `crates/kild/tests/cli_json_output.rs`
**Action**: UPDATE (append tests)

**Test cases to add:**

```rust
/// Verify that 'kild status nonexistent --json' returns a JSON error object
#[test]
fn test_status_json_nonexistent_returns_json_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["status", "nonexistent-branch-xyz-12345", "--json"])
        .output()
        .expect("Failed to execute 'kild status --json'");

    // Command should fail (non-zero exit code)
    assert!(
        !output.status.success(),
        "kild status nonexistent --json should fail"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // stdout should contain valid JSON error object
    let error_obj: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON even on error");

    assert!(
        error_obj.is_object(),
        "JSON error should be an object, got: {}",
        stdout
    );

    // Must have "error" and "code" fields
    assert!(
        error_obj.get("error").and_then(|v| v.as_str()).is_some(),
        "JSON error should have 'error' string field"
    );
    assert!(
        error_obj.get("code").and_then(|v| v.as_str()).is_some(),
        "JSON error should have 'code' string field"
    );

    // Error code should be session_not_found
    assert_eq!(
        error_obj.get("code").and_then(|v| v.as_str()),
        Some("SESSION_NOT_FOUND"),
        "Error code should be SESSION_NOT_FOUND"
    );
}

/// Verify that 'kild pr nonexistent --json' returns a JSON error object
#[test]
fn test_pr_json_nonexistent_returns_json_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["pr", "nonexistent-branch-xyz-12345", "--json"])
        .output()
        .expect("Failed to execute 'kild pr --json'");

    assert!(
        !output.status.success(),
        "kild pr nonexistent --json should fail"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let error_obj: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON even on error");

    assert!(error_obj.get("error").is_some(), "Should have 'error' field");
    assert!(error_obj.get("code").is_some(), "Should have 'code' field");
}

/// Verify that 'kild stats nonexistent --json' returns a JSON error object
#[test]
fn test_stats_json_nonexistent_returns_json_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["stats", "nonexistent-branch-xyz-12345", "--json"])
        .output()
        .expect("Failed to execute 'kild stats --json'");

    assert!(
        !output.status.success(),
        "kild stats nonexistent --json should fail"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let error_obj: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON even on error");

    assert!(error_obj.get("error").is_some(), "Should have 'error' field");
    assert!(error_obj.get("code").is_some(), "Should have 'code' field");
}

/// Verify that non-JSON error mode still works (plain text to stderr)
#[test]
fn test_status_nonexistent_without_json_uses_stderr() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["status", "nonexistent-branch-xyz-12345"])
        .output()
        .expect("Failed to execute 'kild status'");

    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // stdout should be empty (no JSON)
    assert!(
        stdout.trim().is_empty(),
        "Without --json, stdout should be empty on error. Got: {}",
        stdout
    );

    // stderr should contain the error message
    assert!(
        stderr.contains("Failed to get status"),
        "Without --json, error should go to stderr. Got: {}",
        stderr
    );
}
```

---

## Patterns to Follow

**From codebase - JSON success output pattern:**

```rust
// SOURCE: crates/kild/src/commands/status.rs:40-84
// Pattern: check json_output flag, serialize with serde_json::to_string_pretty, println!
if json_output {
    // ... build enriched struct ...
    println!("{}", serde_json::to_string_pretty(&enriched)?);
    // ... log completion ...
    return Ok(());
}
```

**From codebase - PR command's existing JSON "error" handling (soft errors):**

```rust
// SOURCE: crates/kild/src/commands/pr.rs:42-50
// Pattern: JSON object with null + reason field for non-error-but-no-data cases
if json_output {
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "pr": null,
            "branch": format!("kild/{}", branch),
            "reason": "no_remote_configured"
        }))?
    );
}
```

**From codebase - KildError error_code pattern:**

```rust
// SOURCE: crates/kild-core/src/sessions/errors.rs:106-134
// Pattern: each error variant maps to a SCREAMING_SNAKE_CASE code string
impl KildError for SessionError {
    fn error_code(&self) -> &'static str {
        match self {
            SessionError::NotFound { .. } => "SESSION_NOT_FOUND",
            SessionError::AlreadyExists { .. } => "SESSION_ALREADY_EXISTS",
            // ...
        }
    }
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                              | Mitigation                                                                       |
| ------------------------------------------- | -------------------------------------------------------------------------------- |
| Error type doesn't implement KildError      | Use a generic code string (e.g., `"UNKNOWN_ERROR"`) for `Box<dyn Error>` types   |
| `health.rs` error types may not be SessionError | Verify return types; use generic code if needed                              |
| `serde_json` serialization fails            | `JsonError` has only String fields - serialization cannot fail; use `unwrap()`   |
| Existing consumers expect empty stdout on error | This is a **fix** not a break - consumers currently get nothing, now they get JSON |
| `overlaps.rs` partial failure errors        | These go to stderr as warnings; only the final `Err(...)` matters and `overlaps` already handles empty/insufficient kilds as JSON |
| Duplicate logging in json vs non-json paths | Extract logging to happen after the if/else or accept minor duplication for clarity |

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

1. `cargo run -p kild -- status nonexistent --json` - should output `{"error": "Session 'nonexistent' not found", "code": "SESSION_NOT_FOUND"}` to stdout and exit 1
2. `cargo run -p kild -- status nonexistent` - should still output plain text error to stderr (no regression)
3. `cargo run -p kild -- pr nonexistent --json` - should output JSON error
4. `cargo run -p kild -- stats nonexistent --json` - should output JSON error
5. Verify output is parseable: `cargo run -p kild -- status nonexistent --json | jq .code` should output `"SESSION_NOT_FOUND"`

---

## Scope Boundaries

**IN SCOPE:**

- Adding `JsonError` struct and `print_json_error` helper
- Updating error branches in all 6 commands with `--json`: status, list, pr, stats, health, overlaps
- Adding integration tests for JSON error output

**OUT OF SCOPE (do not touch):**

- Success path JSON output (already works correctly)
- `main.rs` error handling (stays as-is; handlers print before returning)
- `daemon.rs` status command (always succeeds; no error path to fix)
- `overlaps.rs` partial failure warnings (go to stderr as warnings, separate from JSON data on stdout)
- Error types in `kild-core` (error codes already exist and are correct)
- Changing exit codes (still exit 1 on error)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-12
- **Artifact**: `.claude/PRPs/issues/issue-397.md`
