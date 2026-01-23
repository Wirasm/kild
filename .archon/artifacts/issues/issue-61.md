# Investigation: UX: Logs pollute stdout, breaking piping and readability

**Issue**: #61 (https://github.com/Wirasm/shards/issues/61)
**Type**: BUG
**Investigated**: 2026-01-23T10:30:00Z

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Severity | HIGH | Every CLI command is affected; breaks Unix piping which is fundamental for scripting and composability |
| Complexity | LOW | Single-file fix in logging module; clear solution using tracing-subscriber's stderr writer |
| Confidence | HIGH | Root cause identified in logging/mod.rs:6; tracing-subscriber docs confirm `.with_writer(std::io::stderr)` is the fix |

---

## Problem Statement

Every CLI command outputs JSON structured logs to stdout mixed with user-facing output. This breaks Unix piping (`shards list | grep foo` includes log lines), creates noisy output, and violates Unix conventions (logs belong on stderr, not stdout).

---

## Analysis

### Root Cause

WHY: JSON logs appear mixed with user output
↓ BECAUSE: Both logs and user output go to stdout
  Evidence: `crates/shards-core/src/logging/mod.rs:6` - `tracing_subscriber::fmt::layer()` uses default writer

↓ BECAUSE: `fmt::layer()` defaults to stdout when no writer is specified
  Evidence: tracing-subscriber docs confirm default is `std::io::stdout()`

↓ ROOT CAUSE: Missing `.with_writer(std::io::stderr)` on the fmt layer
  Evidence: `crates/shards-core/src/logging/mod.rs:6-9`:
  ```rust
  tracing_subscriber::fmt::layer()
      .json()
      .with_current_span(false)
      .with_span_list(false),
  ```

### Evidence Chain

Current `init_logging()` code (`crates/shards-core/src/logging/mod.rs:3-16`):
```rust
pub fn init_logging() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(false)
                .with_span_list(false),
        )
        .with(
            EnvFilter::from_default_env()
                .add_directive("shards=info".parse().expect("Invalid log directive")),
        )
        .init();
}
```

User output uses `println!()` across all commands (stdout):
- `crates/shards/src/commands.rs:79-87` (create command)
- `crates/shards/src/commands.rs:118-120` (list command)
- etc.

Error messages correctly use `eprintln!()` (stderr):
- `crates/shards/src/commands.rs:98` (create error)
- `crates/shards/src/commands.rs:130` (list error)

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `crates/shards-core/src/logging/mod.rs` | 6 | UPDATE | Add `.with_writer(std::io::stderr)` to fmt layer |

### Integration Points

- `crates/shards/src/main.rs:8` calls `init_logging()`
- All CLI commands emit `info!()` and `error!()` events via tracing
- No changes needed to CLI commands - they already correctly use println/eprintln

### Git History

- **Introduced**: 3f23e66 - "refactor: Restructure project as Cargo workspace (#55)"
- **Implication**: Original implementation, not a regression

---

## Implementation Plan

### Step 1: Redirect logs to stderr

**File**: `crates/shards-core/src/logging/mod.rs`
**Lines**: 6
**Action**: UPDATE

**Current code:**
```rust
// Lines 3-16
pub fn init_logging() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(false)
                .with_span_list(false),
        )
        .with(
            EnvFilter::from_default_env()
                .add_directive("shards=info".parse().expect("Invalid log directive")),
        )
        .init();
}
```

**Required change:**
```rust
pub fn init_logging() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_writer(std::io::stderr)
                .with_current_span(false)
                .with_span_list(false),
        )
        .with(
            EnvFilter::from_default_env()
                .add_directive("shards=info".parse().expect("Invalid log directive")),
        )
        .init();
}
```

**Why**: Adding `.with_writer(std::io::stderr)` redirects all tracing output to stderr, keeping stdout clean for user-facing output. This is the standard Unix convention and enables proper piping.

---

### Step 2: Update CLAUDE.md documentation

**File**: `CLAUDE.md`
**Lines**: ~83-92 (Logging Setup section)
**Action**: UPDATE

**Current code:**
```rust
// Located in crates/shards-core/src/logging/mod.rs
tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer()
        .json()
        .with_current_span(false)
        .with_span_list(false))
```

**Required change:**
```rust
// Located in crates/shards-core/src/logging/mod.rs
tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer()
        .json()
        .with_writer(std::io::stderr)  // Logs to stderr, stdout for user output
        .with_current_span(false)
        .with_span_list(false))
```

**Why**: Documentation should match implementation and explain the stderr choice.

---

### Step 3: Add integration test for stdout cleanliness

**File**: `crates/shards/tests/cli_output.rs`
**Action**: CREATE

**Test case to add:**
```rust
//! Integration tests for CLI output behavior

use std::process::Command;

/// Verify that stdout contains only user-facing output (no JSON logs)
#[test]
fn test_list_stdout_is_clean() {
    let output = Command::new(env!("CARGO_BIN_EXE_shards"))
        .arg("list")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // stdout should not contain JSON log lines
    assert!(
        !stdout.contains(r#""event":"#),
        "stdout should not contain JSON logs, got: {}",
        stdout
    );

    // stderr should contain JSON logs (if any logging occurred)
    // Note: logs go to stderr now
    if !stderr.is_empty() {
        // If there's output on stderr, it should be JSON logs
        assert!(
            stderr.contains(r#""timestamp""#) || stderr.contains(r#""level""#),
            "stderr should contain structured logs, got: {}",
            stderr
        );
    }
}

/// Verify piping works correctly
#[test]
fn test_output_is_pipeable() {
    let output = Command::new(env!("CARGO_BIN_EXE_shards"))
        .arg("list")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // stdout should be clean enough to pipe through grep
    // Every line should either be empty, the "No active shards" message,
    // "Active shards:" header, or table content (starts with special chars or |)
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Should not be JSON
        assert!(
            !trimmed.starts_with('{'),
            "stdout contains JSON line: {}",
            line
        );
    }
}
```

**Why**: Ensures the fix works and prevents regression.

---

## Patterns to Follow

**From codebase - error handling pattern already uses stderr correctly:**

```rust
// SOURCE: crates/shards/src/commands.rs:98
// Pattern for error output - already goes to stderr
eprintln!("Failed to create shard: {}", e);
```

**From tracing-subscriber docs - writer configuration:**

```rust
// Pattern for configuring stderr writer
tracing_subscriber::fmt::layer()
    .with_writer(std::io::stderr)
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| Users relying on stdout logs | Low risk - no documented use case; users can still access logs via `2>&1` |
| Test output capture | Tests may need `2>&1` if they're checking for log content |
| Performance of stderr writes | Negligible - same syscall overhead as stdout |

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

1. Run `shards list` and verify only table output appears (no JSON)
2. Run `shards list 2>/dev/null` and verify output is unchanged (logs silenced)
3. Run `shards list 2>&1 | grep event` and verify logs are captured from stderr
4. Run `shards list | grep -v Active` and verify no JSON lines leak through

---

## Scope Boundaries

**IN SCOPE:**
- Redirect tracing output from stdout to stderr
- Update documentation to reflect the change
- Add integration test for stdout cleanliness

**OUT OF SCOPE (do not touch):**
- Adding verbosity flags (`-v`, `--verbose`) - future enhancement
- Adding config option for log level - already exists via `RUST_LOG` env var
- Adding file logging - future enhancement if needed
- Changing log format (JSON) - works fine, just needs to go to stderr

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-23T10:30:00Z
- **Artifact**: `.archon/artifacts/issues/issue-61.md`
