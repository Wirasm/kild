# Investigation: --json commands must always return valid JSON

**Issue**: #369 (https://github.com/Wirasm/kild/issues/369)
**Type**: BUG
**Investigated**: 2026-02-11

### Assessment

| Metric     | Value  | Reasoning                                                                                              |
| ---------- | ------ | ------------------------------------------------------------------------------------------------------ |
| Severity   | HIGH   | Breaks the programmatic API surface for agent automation; `--json` is a contract for scripting callers  |
| Complexity | LOW    | All fixes are isolated to CLI command handlers; same pattern repeated across 3 files                    |
| Confidence | HIGH   | Root cause is obvious: conditional branches that print plain text without checking the `json_output` flag |

---

## Problem Statement

Several CLI commands with `--json` flag violate the JSON contract by printing plain text for "empty" or "error" states. The primary offender is `kild pr --json` which prints `"No PR found for branch 'kild/my-branch'"` as plain text instead of JSON. Two other commands (`stats --all --json`, `overlaps --json`) have the same pattern of printing plain text for empty states before reaching the JSON output path.

---

## Analysis

### Root Cause

WHY: `kild pr my-branch --json` outputs plain text instead of JSON
↓ BECAUSE: The `None` branch in the `match pr_info` block doesn't check `json_output`
Evidence: `crates/kild/src/commands/pr.rs:103-110`:
```rust
None => {
    println!("No PR found for branch 'kild/{}'", branch);
}
```

WHY: The `None` branch doesn't check `json_output`
↓ BECAUSE: Early exit paths (no remote, no PR) were added without considering JSON mode
Evidence: `pr.rs:41-48` also prints plain text regardless of `json_output`:
```rust
if !session_ops::has_remote_configured(&session.worktree_path) {
    println!("No remote configured — PR tracking unavailable.");
    return Ok(());
}
```

ROOT CAUSE: Command handlers have multiple exit paths that bypass the `json_output` check. The `json_output` flag is read but only used in the "happy path" where data exists. Empty/error states print plain text unconditionally.

### Evidence Chain

The `daemon status` command is the **only** command that correctly handles all states in JSON mode:

```rust
// crates/kild/src/commands/daemon.rs:157-174
if json {
    let status = if running {
        serde_json::json!({ "running": true, "pid": pid, "socket": ... })
    } else {
        serde_json::json!({ "running": false })
    };
    println!("{}", serde_json::to_string_pretty(&status)?);
}
```

All other commands with empty-state violations follow this anti-pattern:
```rust
// Anti-pattern: early return with plain text BEFORE json check
if some_empty_condition {
    println!("Plain text message");  // BUG: ignores json_output
    return Ok(());
}
// ... later ...
if json_output {
    println!("{}", serde_json::to_string_pretty(&data)?);
}
```

### Affected Files

| File                                     | Lines    | Action | Description                                             |
| ---------------------------------------- | -------- | ------ | ------------------------------------------------------- |
| `crates/kild/src/commands/pr.rs`         | 41-48    | UPDATE | No-remote path: add JSON output                         |
| `crates/kild/src/commands/pr.rs`         | 103-110  | UPDATE | No-PR-found path: add JSON output                       |
| `crates/kild/src/commands/stats.rs`      | 129-133  | UPDATE | Empty sessions (--all): add JSON output                 |
| `crates/kild/src/commands/overlaps.rs`   | 26-30    | UPDATE | Empty sessions: add JSON output                         |
| `crates/kild/src/commands/overlaps.rs`   | 32-36    | UPDATE | Only 1 kild: add JSON output                            |
| `crates/kild/tests/cli_json_output.rs`   | EOF      | UPDATE | Add contract tests for empty/error JSON states          |

### Integration Points

- `kild pr` calls `session_ops::get_session`, `session_ops::has_remote_configured`, `session_ops::fetch_pr_info` — all in kild-core
- `kild stats --all` calls `session_ops::list_sessions` — returns empty Vec
- `kild overlaps` calls `session_ops::list_sessions` — returns empty Vec
- No core library changes needed; all fixes are in CLI command handlers

### Git History

- **pr.rs introduced**: `1027b21` - "refactor: split commands.rs into per-command module directory (#280)"
- **stats.rs introduced**: `809da75` - "feat: add `kild stats` command (#291)"
- **overlaps.rs introduced**: `519c6ef` - "feat: add `kild overlaps` command (#292)"
- **Implication**: Original bugs from initial implementation — JSON empty states were never handled

---

## Implementation Plan

### Step 1: Fix `pr.rs` — No-remote path

**File**: `crates/kild/src/commands/pr.rs`
**Lines**: 40-49
**Action**: UPDATE

**Current code:**
```rust
// 2. Check for remote
if !session_ops::has_remote_configured(&session.worktree_path) {
    println!("No remote configured — PR tracking unavailable.");
    info!(
        event = "cli.pr_completed",
        branch = branch,
        result = "no_remote"
    );
    return Ok(());
}
```

**Required change:**
```rust
// 2. Check for remote
if !session_ops::has_remote_configured(&session.worktree_path) {
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "pr": null,
                "branch": format!("kild/{}", branch),
                "reason": "no_remote_configured"
            }))?
        );
    } else {
        println!("No remote configured — PR tracking unavailable.");
    }
    info!(
        event = "cli.pr_completed",
        branch = branch,
        result = "no_remote"
    );
    return Ok(());
}
```

**Why**: The `--json` flag must produce valid JSON for all states, including "no remote configured".

---

### Step 2: Fix `pr.rs` — No PR found path

**File**: `crates/kild/src/commands/pr.rs`
**Lines**: 103-110
**Action**: UPDATE

**Current code:**
```rust
None => {
    println!("No PR found for branch 'kild/{}'", branch);
    info!(
        event = "cli.pr_completed",
        branch = branch,
        result = "no_pr"
    );
}
```

**Required change:**
```rust
None => {
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "pr": null,
                "branch": format!("kild/{}", branch)
            }))?
        );
    } else {
        println!("No PR found for branch 'kild/{}'", branch);
    }
    info!(
        event = "cli.pr_completed",
        branch = branch,
        result = "no_pr"
    );
}
```

**Why**: This is the primary bug from the issue. Scripts parsing `kild pr --json` get plain text.

---

### Step 3: Fix `stats.rs` — Empty sessions with --all

**File**: `crates/kild/src/commands/stats.rs`
**Lines**: 129-133
**Action**: UPDATE

**Current code:**
```rust
if sessions.is_empty() {
    println!("No kilds found.");
    info!(event = "cli.stats_all_completed", count = 0);
    return Ok(());
}
```

**Required change:**
```rust
if sessions.is_empty() {
    if json_output {
        println!("[]");
    } else {
        println!("No kilds found.");
    }
    info!(event = "cli.stats_all_completed", count = 0);
    return Ok(());
}
```

**Why**: Matches the pattern from `list --json` which returns `[]` for empty results.

---

### Step 4: Fix `overlaps.rs` — Empty sessions

**File**: `crates/kild/src/commands/overlaps.rs`
**Lines**: 26-36
**Action**: UPDATE

**Current code:**
```rust
if sessions.is_empty() {
    println!("No kilds found.");
    info!(event = "cli.overlaps_completed", overlap_count = 0);
    return Ok(());
}

if sessions.len() < 2 {
    println!("Only 1 kild active. Overlaps require at least 2 kilds.");
    info!(event = "cli.overlaps_completed", overlap_count = 0);
    return Ok(());
}
```

**Required change:**
```rust
if sessions.is_empty() {
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "overlapping_files": [],
                "clean_kilds": [],
                "reason": "no_kilds_found"
            }))?
        );
    } else {
        println!("No kilds found.");
    }
    info!(event = "cli.overlaps_completed", overlap_count = 0);
    return Ok(());
}

if sessions.len() < 2 {
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "overlapping_files": [],
                "clean_kilds": [],
                "reason": "insufficient_kilds"
            }))?
        );
    } else {
        println!("Only 1 kild active. Overlaps require at least 2 kilds.");
    }
    info!(event = "cli.overlaps_completed", overlap_count = 0);
    return Ok(());
}
```

**Why**: Returns a JSON object with the same shape as normal output (with `overlapping_files` and `clean_kilds` arrays) plus a `reason` field explaining why it's empty. This makes parsing predictable.

---

### Step 5: Add/Update Tests

**File**: `crates/kild/tests/cli_json_output.rs`
**Action**: UPDATE

**Test cases to add:**

```rust
/// Verify that 'kild stats --all --json' always returns valid JSON (even empty)
#[test]
fn test_stats_all_json_outputs_valid_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["stats", "--all", "--json"])
        .output()
        .expect("Failed to execute 'kild stats --all --json'");

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let _value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stats --all --json stdout should be valid JSON");
}

/// Verify that 'kild overlaps --json' always returns valid JSON (even empty)
#[test]
fn test_overlaps_json_outputs_valid_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["overlaps", "--json"])
        .output()
        .expect("Failed to execute 'kild overlaps --json'");

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let _value: serde_json::Value =
        serde_json::from_str(&stdout).expect("overlaps --json stdout should be valid JSON");
}
```

**Note**: `kild pr --json` requires a live session to test, so it can't easily be integration-tested in CI without setup. The `stats --all` and `overlaps` commands are testable because they handle the empty-list case locally.

---

## Patterns to Follow

**From codebase — the daemon status command is the reference:**

```rust
// SOURCE: crates/kild/src/commands/daemon.rs:157-175
// Pattern: Always produce JSON when flag is set, regardless of state
if json {
    let status = if running {
        serde_json::json!({ "running": true, "pid": pid, "socket": socket_path })
    } else {
        serde_json::json!({ "running": false })
    };
    println!("{}", serde_json::to_string_pretty(&status)?);
} else if running {
    // human output...
} else {
    println!("Daemon: stopped");
}
```

**From codebase — the list command handles empty arrays correctly:**

```rust
// SOURCE: crates/kild/src/commands/list.rs:23-48
// Pattern: JSON branch first, plain text after
if json_output {
    // Always outputs JSON — empty list becomes []
    println!("{}", serde_json::to_string_pretty(&enriched)?);
} else if sessions.is_empty() {
    println!("No active kilds found.");
} else {
    // table output...
}
```

---

## Edge Cases & Risks

| Risk/Edge Case                      | Mitigation                                                                           |
| ----------------------------------- | ------------------------------------------------------------------------------------ |
| JSON shape changes break consumers  | Use consistent shapes: `{"pr": null, ...}` mirrors `{"pr": {...}, ...}` from normal output |
| `eprintln!` errors in JSON mode     | Out of scope for this issue. Errors go to stderr which doesn't break JSON on stdout. Tracked separately if needed |
| `serde_json::to_string_pretty` fail | Already uses `?` operator — errors propagate. Extremely unlikely for static JSON values |

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

1. `cargo run -p kild -- pr nonexistent-branch --json` — should output JSON with `"pr": null`
2. `cargo run -p kild -- stats --all --json` — with no kilds, should output `[]`
3. `cargo run -p kild -- overlaps --json` — with no kilds, should output JSON object with empty arrays

---

## Scope Boundaries

**IN SCOPE:**
- Fix `pr.rs` no-remote and no-PR paths to output JSON
- Fix `stats.rs` empty --all path to output JSON
- Fix `overlaps.rs` empty/insufficient paths to output JSON
- Add integration tests for these empty-state JSON paths

**OUT OF SCOPE (do not touch):**
- `eprintln!` error messages (they go to stderr, not stdout — separate concern)
- `list`, `status`, `health`, `daemon status` commands (already correct)
- Core library changes (all fixes are CLI-layer only)
- Refactoring the overall error handling pattern (future improvement)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-11
- **Artifact**: `.claude/PRPs/issues/issue-369.md`
