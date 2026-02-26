# Investigation: honryu should not be able to stop/destroy itself via --all commands

**Issue**: #600 (https://github.com/Wirasm/kild/issues/600)
**Type**: BUG
**Investigated**: 2026-02-26T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                         |
| ---------- | ------ | ----------------------------------------------------------------------------------------------------------------- |
| Severity   | HIGH   | Fleet brain self-destructs mid-execution, leaving all workers unmanaged with no recovery path                     |
| Complexity | MEDIUM | 3 files changed (stop, destroy, helpers), self-detection helper is straightforward, but edge cases need care      |
| Confidence | HIGH   | Root cause is clear (no self-exclusion filter in --all loops), existing `find_session_by_worktree_path` is reusable |

---

## Problem Statement

When Honryū (the fleet brain) runs `kild stop --all`, `kild destroy --all`, or `kild cleanup`, it includes itself in the operation. The `--all` command loops iterate every session returned by `list_sessions()` without any self-detection or exclusion. This causes the brain to kill its own agent process mid-execution, leaving the fleet fully unmanaged.

---

## Analysis

### Root Cause

**WHY 1**: Why does honryu stop itself?
→ Because `handle_stop_all()` iterates ALL active sessions without exclusion.
→ Evidence: `crates/kild/src/commands/stop.rs:124` — `for session in active { session_ops::stop_session(&session.branch) }`

**WHY 2**: Why are all sessions included?
→ Because `list_sessions()` returns every session from `~/.kild/sessions/` with no caller-identity filter.
→ Evidence: `crates/kild-core/src/sessions/list.rs:7-27` — loads all session files, returns full vec

**WHY 3**: Why is there no self-detection?
→ Because `--all` commands were designed without considering they could be invoked from within a session. The existing `--self` detection mechanism (`find_session_by_worktree_path`) was built for `agent-status` only and was never wired into stop/destroy/cleanup.
→ Evidence: `crates/kild-core/src/sessions/agent_status.rs:81-90` — `find_session_by_worktree_path` is only called from `crates/kild/src/commands/agent_status.rs:22`

**ROOT CAUSE**: The `--all` iteration loops in `handle_stop_all()`, `handle_destroy_all()`, and `handle_open_all()` have no self-exclusion filter. Self-detection primitives exist (`$KILD_SESSION_BRANCH` env var, CWD-based worktree path match) but are not wired into these code paths.

### Evidence Chain

| Signal | How to Read | Availability |
|--------|-------------|-------------|
| `$KILD_SESSION_BRANCH` env var | `std::env::var("KILD_SESSION_BRANCH")` | claude & codex agents only (`daemon_request.rs:298-304`) |
| CWD worktree path match | `find_session_by_worktree_path(&cwd)` | All agents, requires CWD inside session worktree |

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `crates/kild/src/commands/helpers.rs` | NEW | UPDATE | Add `resolve_self_branch()` helper |
| `crates/kild/src/commands/stop.rs` | 100-140 | UPDATE | Filter self from `--all` loop, guard single-branch self-stop |
| `crates/kild/src/commands/destroy.rs` | 12-19, 108-162 | UPDATE | Filter self from `--all` loop, guard single-branch self-destroy |

### Integration Points

- `crates/kild-core/src/sessions/agent_status.rs:81` — `find_session_by_worktree_path()` is the existing self-resolution mechanism (reuse it)
- `crates/kild-core/src/sessions/fleet.rs:21` — `pub const BRAIN_BRANCH: &str = "honryu"` (not needed — fix is generic, not honryu-specific)
- `crates/kild-core/src/agents/resume.rs:54-78` — `KILD_SESSION_BRANCH` only injected for claude/codex (CWD fallback covers other agents)
- `crates/kild/src/commands/open.rs:84-175` — `open --all` only targets Stopped sessions; if caller is Active, self-exclusion is a no-op (no change needed)
- `crates/kild/src/commands/cleanup.rs` — cleanup already safe for active sessions: `detect_stale_sessions()` skips sessions with existing worktree paths, `detect_sessions_older_than()` only picks up stopped sessions (no change needed)

### Git History

- **stop.rs last modified**: `8b4d240` — "fix(cli): stop --all mentions already-stopped sessions (#558)"
- **destroy.rs last modified**: `19c0c7c` — "refactor: extract kild-config crate from kild-core (#434)"
- **Implication**: Long-standing gap — self-exclusion was never implemented since `--all` was added

---

## Implementation Plan

### Step 1: Add `resolve_self_branch()` helper

**File**: `crates/kild/src/commands/helpers.rs`
**Action**: UPDATE (add new function)

**Required change:**

```rust
use kild_core::session_ops;

/// Resolve the branch name of the calling session, if running inside one.
///
/// Tries `$KILD_SESSION_BRANCH` first (reliable for claude/codex agents),
/// then falls back to CWD-based worktree path matching (universal).
/// Returns `None` when called from outside any kild session.
pub(crate) fn resolve_self_branch() -> Option<String> {
    // Fast path: env var is set for claude and codex daemon sessions
    if let Ok(branch) = std::env::var("KILD_SESSION_BRANCH") {
        if !branch.is_empty() {
            return Some(branch);
        }
    }

    // Fallback: match CWD against session worktree paths
    let cwd = std::env::current_dir().ok()?;
    let session = session_ops::find_session_by_worktree_path(&cwd).ok()??;
    Some(session.branch.to_string())
}
```

**Why**: Centralizes self-detection in one place. Both env var and CWD approaches are tried so it works for all agent types. Returns `None` when called from the user's regular terminal (no false positives).

---

### Step 2: Add self-exclusion to `kild stop --all`

**File**: `crates/kild/src/commands/stop.rs`
**Lines**: 100-140
**Action**: UPDATE

**Current code (lines 100-113):**

```rust
fn handle_stop_all() -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.stop_all_started");

    let sessions = session_ops::list_sessions()?;
    let mut active = Vec::new();
    let mut already_stopped = Vec::new();

    for s in sessions {
        match s.status {
            SessionStatus::Active => active.push(s),
            SessionStatus::Stopped => already_stopped.push(s),
            _ => {}
        }
    }
```

**Required change:**

```rust
fn handle_stop_all() -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.stop_all_started");

    let self_branch = super::helpers::resolve_self_branch();

    let sessions = session_ops::list_sessions()?;
    let mut active = Vec::new();
    let mut already_stopped = Vec::new();
    let mut skipped_self = false;

    for s in sessions {
        // Skip the calling session to prevent self-destruction
        if let Some(ref self_br) = self_branch {
            if s.branch.as_ref() == self_br.as_str() {
                skipped_self = true;
                continue;
            }
        }
        match s.status {
            SessionStatus::Active => active.push(s),
            SessionStatus::Stopped => already_stopped.push(s),
            _ => {}
        }
    }

    if skipped_self {
        if let Some(ref self_br) = self_branch {
            info!(
                event = "cli.stop_all_self_skipped",
                branch = self_br.as_str()
            );
            eprintln!(
                "{} Skipping self ({}) — use `kild stop {}` explicitly.",
                color::warning("Note:"),
                color::ice(self_br),
                self_br,
            );
        }
    }
```

**Why**: Filters the calling session out of the `--all` set before iteration. The warning tells the user (or brain agent) what happened and how to explicitly stop if truly intended.

---

### Step 3: Add self-guard to single-branch `kild stop`

**File**: `crates/kild/src/commands/stop.rs`
**Lines**: 24-48
**Action**: UPDATE

**Current code (lines 24-31):**

```rust
    // Single branch operation
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;

    info!(event = "cli.stop_started", branch = branch);

    match session_ops::stop_session(branch) {
```

**Required change:**

```rust
    // Single branch operation
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;

    // Warn if stopping own session (prevents accidental self-destruction)
    if let Some(self_br) = super::helpers::resolve_self_branch() {
        if self_br == branch.as_str() {
            eprintln!(
                "{} You are about to stop your own session ({}).",
                color::warning("Warning:"),
                color::ice(branch),
            );
            eprintln!(
                "  {}",
                color::hint("This will kill the agent running this command."),
            );
            // Let it proceed — this is an explicit, intentional operation.
            // The warning ensures the user/agent is aware.
        }
    }

    info!(event = "cli.stop_started", branch = branch);

    match session_ops::stop_session(branch) {
```

**Why**: Explicit `kild stop honryu` is intentional — don't block it, just warn. The distinction is between accidental self-inclusion (--all) vs intentional self-targeting (explicit branch). The warning ensures awareness without breaking the user's ability to stop any session explicitly from the CLI.

---

### Step 4: Add self-exclusion to `kild destroy --all`

**File**: `crates/kild/src/commands/destroy.rs`
**Lines**: 108-162
**Action**: UPDATE

**Current code (lines 108-121):**

```rust
fn handle_destroy_all(force: bool) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.destroy_all_started", force = force);

    let sessions = session_ops::list_sessions()?;

    if sessions.is_empty() {
        println!("No kilds to destroy.");
        info!(
            event = "cli.destroy_all_completed",
            destroyed = 0,
            failed = 0
        );
        return Ok(());
    }
```

**Required change:**

```rust
fn handle_destroy_all(force: bool) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.destroy_all_started", force = force);

    let self_branch = super::helpers::resolve_self_branch();

    let mut sessions = session_ops::list_sessions()?;

    // Filter out the calling session to prevent self-destruction
    let skipped_self = if let Some(ref self_br) = self_branch {
        let before = sessions.len();
        sessions.retain(|s| s.branch.as_ref() != self_br.as_str());
        before > sessions.len()
    } else {
        false
    };

    if sessions.is_empty() {
        if skipped_self {
            println!("No other kilds to destroy (skipped self).");
        } else {
            println!("No kilds to destroy.");
        }
        info!(
            event = "cli.destroy_all_completed",
            destroyed = 0,
            failed = 0
        );
        return Ok(());
    }

    if skipped_self {
        if let Some(ref self_br) = self_branch {
            info!(
                event = "cli.destroy_all_self_skipped",
                branch = self_br.as_str()
            );
            eprintln!(
                "{} Skipping self ({}) — use `kild destroy {}` explicitly.",
                color::warning("Note:"),
                color::ice(self_br),
                self_br,
            );
        }
    }
```

**Why**: Same pattern as stop --all. Filters self before the confirmation prompt and destroy loop. The user sees the correct count of sessions to destroy (excluding self).

---

### Step 5: Add self-guard to single-branch `kild destroy`

**File**: `crates/kild/src/commands/destroy.rs`
**Lines**: 21-31
**Action**: UPDATE

**Current code (lines 21-31):**

```rust
    // Single branch operation
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;

    info!(
        event = "cli.destroy_started",
        branch = branch,
        force = force
    );
```

**Required change:**

```rust
    // Single branch operation
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;

    // Warn if destroying own session
    if let Some(self_br) = super::helpers::resolve_self_branch() {
        if self_br == branch.as_str() {
            eprintln!(
                "{} You are about to destroy your own session ({}).",
                color::warning("Warning:"),
                color::ice(branch),
            );
            eprintln!(
                "  {}",
                color::hint("This will kill the agent and remove the session."),
            );
        }
    }

    info!(
        event = "cli.destroy_started",
        branch = branch,
        force = force
    );
```

**Why**: Same rationale as stop — warn on explicit self-targeting, don't block.

---

### Step 6: Add tests

**File**: `crates/kild/src/commands/stop.rs` (or a new test module)
**Action**: UPDATE

**Test cases to add:**

Tests for `resolve_self_branch()`:
- `resolve_self_branch_from_env_var` — set `KILD_SESSION_BRANCH=honryu`, verify returns `Some("honryu")`
- `resolve_self_branch_empty_env_var` — set `KILD_SESSION_BRANCH=""`, verify falls through to CWD (returns None in test env)
- `resolve_self_branch_no_env_var` — unset env var, verify returns None when CWD is not a session worktree

Note: Integration tests for the full stop/destroy --all self-exclusion flow are harder to unit test because they require session fixtures and daemon state. The `resolve_self_branch()` helper is the testable unit. The filtering logic in `handle_stop_all`/`handle_destroy_all` is straightforward `retain`/`continue` — covered by manual verification.

---

## Patterns to Follow

**From codebase — self-detection pattern in agent_status.rs:**

```rust
// SOURCE: crates/kild/src/commands/agent_status.rs:22-29
// Pattern for resolving "current session" from CWD
(true, [status]) => {
    let cwd = std::env::current_dir()?;
    let session = session_ops::find_session_by_worktree_path(&cwd)?.ok_or_else(|| {
        format!(
            "No kild session found for current directory: {}",
            cwd.display()
        )
    })?;
    (session.branch, status.as_str())
}
```

**From codebase — warning message pattern in destroy.rs:**

```rust
// SOURCE: crates/kild/src/commands/destroy.rs:38-44
// Pattern for safety warnings
for warning in &warnings {
    if safety_info.should_block() {
        eprintln!("{} {}", color::warning("Warning:"), warning);
    } else {
        println!("{} {}", color::copper("Warning:"), warning);
    }
}
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| `$KILD_SESSION_BRANCH` not set for non-claude/codex agents | CWD fallback via `find_session_by_worktree_path` covers all agents |
| Multiple `--main` sessions share the same worktree_path (project root) | `$KILD_SESSION_BRANCH` is authoritative when set; CWD match returns first match which may be wrong — acceptable since the env var path handles the primary use case (brain = claude agent) |
| `resolve_self_branch()` adds a `list_sessions()` call on CWD fallback path | Only triggered when `$KILD_SESSION_BRANCH` is unset; acceptable overhead for a destructive operation |
| User running `kild stop --all` from project root outside any session | `resolve_self_branch()` returns None, no exclusion applied — correct behavior |
| Cleanup: `kild cleanup` could catch honryu session | Already safe: `detect_stale_sessions()` skips sessions with existing worktree paths (honryu uses project root), `detect_sessions_older_than()` only targets stopped sessions. No code change needed. |
| `open --all` self-exclusion | Not needed: `open --all` only targets Stopped sessions; if the caller is Active (running), it won't be in the list |

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

1. Create honryu session: `kild create honryu --daemon --main`
2. Create a worker: `kild create worker-1 --daemon`
3. From honryu's PTY, run `kild stop --all` → verify worker-1 stops, honryu stays active with "Skipping self" message
4. From honryu's PTY, run `kild stop honryu` → verify warning is shown, stop proceeds
5. From regular terminal, run `kild stop --all` → verify ALL sessions stop (no exclusion when not inside a session)
6. Repeat steps 1-5 with `kild destroy --all` and `kild destroy honryu`

---

## Scope Boundaries

**IN SCOPE:**

- Self-exclusion for `kild stop --all`
- Self-exclusion for `kild destroy --all`
- Self-warning for explicit `kild stop <self>` and `kild destroy <self>`
- `resolve_self_branch()` helper using `$KILD_SESSION_BRANCH` + CWD fallback

**OUT OF SCOPE (do not touch):**

- `kild cleanup` — already safe for active sessions (no code change needed)
- `kild open --all` — only targets Stopped sessions, self-exclusion is a no-op
- Making `$KILD_SESSION_BRANCH` available for all agent types (separate enhancement)
- Adding `--include-self` flag (overengineering — the explicit branch name path handles intentional self-targeting)
- Core-layer changes — self-detection is process-level (env vars, CWD), belongs in CLI layer

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-26T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-600.md`
