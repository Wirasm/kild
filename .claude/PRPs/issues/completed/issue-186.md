# Investigation: kild complete bypasses git safety checks for uncommitted changes

**Issue**: #186 (https://github.com/Wirasm/kild/issues/186)
**Type**: BUG
**Investigated**: 2026-01-30

### Assessment

| Metric     | Value    | Reasoning                                                                                      |
| ---------- | -------- | ---------------------------------------------------------------------------------------------- |
| Severity   | CRITICAL | Causes silent data loss - uncommitted work destroyed without warning or ability to recover      |
| Complexity | LOW      | 2 files need changes, safety infrastructure already exists, just needs wiring up                |
| Confidence | HIGH     | Clear root cause with exact line references, fix mirrors existing working pattern in `destroy`  |

---

## Problem Statement

`kild complete` does not check for uncommitted changes before destroying the worktree. An agent can be actively writing code in a kild, and `kild complete` (without `--force`) will destroy the worktree and all uncommitted work without warning. `kild destroy` correctly blocks on uncommitted changes, but `kild complete` bypasses this check entirely.

---

## Analysis

### Root Cause

The safety check infrastructure exists and works correctly for `kild destroy`. The `kild complete` command was implemented to delegate to `destroy_session()` but the safety check was never wired up.

### Evidence Chain

WHY: `kild complete` destroys worktrees with uncommitted changes
↓ BECAUSE: `handle_complete_command` does not call `get_destroy_safety_info()` before calling `complete_session()`
Evidence: `crates/kild/src/commands.rs:301-358` - No safety check before line 320

↓ BECAUSE: `complete_session()` delegates directly to `destroy_session()` without any pre-flight check
Evidence: `crates/kild-core/src/sessions/handler.rs:448` - `destroy_session(name, force)?;`

↓ ROOT CAUSE: The safety check (`get_destroy_safety_info` → `should_block()`) is only wired up in `handle_destroy_command`, not in `handle_complete_command`
Evidence: `crates/kild/src/commands.rs:249-276` (destroy has it) vs `crates/kild/src/commands.rs:301-358` (complete lacks it)

### Affected Files

| File                                           | Lines   | Action | Description                                       |
| ---------------------------------------------- | ------- | ------ | ------------------------------------------------- |
| `crates/kild/src/commands.rs`                  | 301-358 | UPDATE | Add safety check to `handle_complete_command`      |
| `crates/kild-core/src/sessions/handler.rs`     | 399-457 | UPDATE | Add safety check inside `complete_session()`       |
| `crates/kild-core/src/sessions/handler.rs`     | ~1166   | UPDATE | Add tests for complete with uncommitted changes    |

### Integration Points

- `commands.rs:320` calls `complete_session()` which calls `destroy_session()` at `handler.rs:448`
- `get_destroy_safety_info()` at `handler.rs:619-699` already exists and works correctly
- `DestroySafetyInfo::should_block()` at `sessions/types.rs:68-70` checks `has_uncommitted_changes`
- `get_worktree_status()` at `git/operations.rs:161-185` detects staged, modified, and untracked files

### Git History

- `complete_session` was introduced without safety checks from the start
- `destroy_session` safety checks were added in the CLI layer only

---

## Implementation Plan

### Approach: `complete` always blocks on uncommitted changes — no `--force` bypass

`kild complete` is a clean-workflow command: the work is done, the PR is merged, clean up. It should **always** refuse if there are uncommitted changes. There is no `--force` escape hatch on complete — if you need to force-destroy a dirty worktree, use `kild destroy --force` explicitly. This makes the commands semantically distinct:

- `kild complete` = clean finish (blocks on dirty state, no override)
- `kild destroy --force` = deliberate destructive action (user accepts data loss)

The fix goes in the **core layer** (`complete_session()`) so all callers (CLI, UI, scripts) are protected. The CLI layer also adds the check for better UX (showing warnings before the core even runs).

### Step 1: Add `UncommittedChanges` error variant

**File**: `crates/kild-core/src/sessions/errors.rs`
**Action**: UPDATE

**Required change:**
Add a new error variant to `SessionError`:

```rust
#[error("Cannot complete '{name}' with uncommitted changes. Use 'kild destroy --force' to remove.")]
UncommittedChanges { name: String },
```

Implement `KildError` trait for this variant:
- `error_code()`: `"SESSION_UNCOMMITTED_CHANGES"`
- `is_user_error()`: `true`

**Why**: Distinct error type that directs users to the correct command.

### Step 2: Add safety check to `complete_session()` in core — always enforced

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 399-457
**Action**: UPDATE

**Current code (line ~448):**
```rust
// 4. Destroy the session (reuse existing logic)
destroy_session(name, force)?;
```

**Required change:**
```rust
// 4. Safety check: always block on uncommitted changes (no --force bypass for complete)
let safety_info = get_destroy_safety_info(name)?;
if safety_info.should_block() {
    error!(
        event = "core.session.complete_blocked",
        name = name,
        reason = "uncommitted_changes"
    );
    return Err(SessionError::UncommittedChanges {
        name: name.to_string(),
    });
}

// 5. Destroy the session
destroy_session(name, force)?;
```

**Why**: Enforces safety unconditionally at the core layer. The `force` flag is irrelevant for the uncommitted check — `complete` always refuses dirty worktrees. `force` is still passed through to `destroy_session` for other git safety checks (unpushed commits, etc.).

### Step 3: Add safety check to CLI `handle_complete_command`

**File**: `crates/kild/src/commands.rs`
**Lines**: 301-358
**Action**: UPDATE

**Current code (around line 320):**
```rust
let force = matches.get_flag("force");

info!(
    event = "cli.complete_started",
    branch = branch,
    force = force
);

match session_handler::complete_session(branch, force) {
```

**Required change:**
```rust
let force = matches.get_flag("force");

info!(
    event = "cli.complete_started",
    branch = branch,
    force = force
);

// Pre-complete safety check (always — complete never bypasses uncommitted check)
if let Ok(safety_info) = session_handler::get_destroy_safety_info(branch) {
    if safety_info.has_warnings() {
        let warnings = safety_info.warning_messages();
        for warning in &warnings {
            if safety_info.should_block() {
                eprintln!("⚠️  {}", warning);
            } else {
                println!("⚠️  {}", warning);
            }
        }
    }

    if safety_info.should_block() {
        eprintln!();
        eprintln!("❌ Cannot complete '{}' with uncommitted changes.", branch);
        eprintln!("   Use 'kild destroy --force {}' to remove anyway.", branch);

        error!(
            event = "cli.complete_blocked",
            branch = branch,
            reason = "uncommitted_changes"
        );

        return Err("Uncommitted changes detected. Use 'kild destroy --force' to override.".into());
    }
}

match session_handler::complete_session(branch, force) {
```

**Why**: CLI provides user-facing warnings and directs to `destroy --force` as the explicit workaround.

### Step 4: Add tests

**File**: `crates/kild-core/src/sessions/handler.rs`
**Action**: UPDATE (add tests near existing complete tests at ~line 1166)

**Test cases to add:**

```rust
#[test]
fn test_complete_session_blocks_on_uncommitted_changes() {
    // Setup: create a session with uncommitted changes in worktree
    // Act: call complete_session(name, false)
    // Assert: returns SessionError::UncommittedChanges
}

#[test]
fn test_complete_session_force_still_blocks_on_uncommitted_changes() {
    // Setup: create a session with uncommitted changes in worktree
    // Act: call complete_session(name, true)
    // Assert: STILL returns SessionError::UncommittedChanges
    // (complete never allows dirty worktrees, even with force)
}
```

Note: These tests may need filesystem setup similar to existing tests in the file. Follow existing test patterns for session creation and worktree setup.

---

## Patterns to Follow

**From codebase - reference the destroy CLI handler pattern:**

```rust
// SOURCE: crates/kild/src/commands.rs:249-276
// Destroy uses `if !force` gate — complete removes that gate entirely.
// Complete always checks, always blocks on uncommitted changes.
// The error message directs to `kild destroy --force` instead of `kild complete --force`.
```

---

## Edge Cases & Risks

| Risk/Edge Case                            | Mitigation                                                              |
| ----------------------------------------- | ----------------------------------------------------------------------- |
| Agent writing files during safety check   | Conservative fallback in `get_worktree_status` assumes dirty on failure |
| Git status check fails (repo corruption)  | Existing fallback at `handler.rs:647-660` assumes dirty                 |
| Double safety check (CLI + core)          | Redundant but safe — core is authoritative, CLI provides UX             |
| `destroy --all` calls complete internally | Verify `destroy --all` path also gets safety checks                     |

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

1. Create a kild, make uncommitted changes, run `kild complete` — should block with error
2. Create a kild, make uncommitted changes, run `kild complete --force` — should STILL block (no force bypass)
3. Create a kild, make uncommitted changes, run `kild destroy --force` — should succeed (correct workaround)
4. Create a kild, commit all changes, run `kild complete` — should succeed
5. Create a kild (clean), run `kild complete` — should succeed

---

## Scope Boundaries

**IN SCOPE:**
- Add safety check to `complete_session()` in core
- Add safety check to `handle_complete_command` in CLI
- Add `UncommittedChanges` error variant
- Add tests for the new behavior

**OUT OF SCOPE (do not touch):**
- Changing `destroy_session()` behavior
- Changing `get_destroy_safety_info()` implementation
- Adding new git status checks
- Modifying the `--force` flag behavior for destroy

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-30
- **Artifact**: `.claude/PRPs/issues/issue-186.md`
