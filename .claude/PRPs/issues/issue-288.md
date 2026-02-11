# Investigation: open --no-agent inherits session agent name instead of 'shell' in spawn record

**Issue**: #288 (https://github.com/Wirasm/kild/issues/288)
**Type**: BUG
**Investigated**: 2026-02-11

### Assessment

| Metric     | Value  | Reasoning                                                                                  |
| ---------- | ------ | ------------------------------------------------------------------------------------------ |
| Severity   | LOW    | Cosmetic bug: only affects display/JSON output, actual shell command runs correctly         |
| Complexity | LOW    | Single line change in one file, isolated to BareShell match arm in open_session             |
| Confidence | HIGH   | Root cause identified exactly at handler.rs:760, fix is identical to existing create_session pattern |

---

## Problem Statement

When `kild open --no-agent` is used on a kild originally created with an agent (e.g., `claude`), the spawn record's `agent` field shows the session's original agent name (`"claude"`) instead of `"shell"`. This causes incorrect display in `kild list`, `kild status`, and JSON output, though the actual command (`/bin/zsh`) runs correctly.

---

## Analysis

### Root Cause

The `BareShell` match arm in `open_session` uses `session.agent.clone()` (the session's original agent) instead of the literal `"shell"` that `create_session` correctly uses.

### Evidence Chain

WHY: Spawn record shows `"agent": "claude"` for a bare shell open
↓ BECAUSE: The `agent` variable is set to `session.agent.clone()` at handler.rs:760
Evidence: `crates/kild-core/src/sessions/handler.rs:760` - `(session.agent.clone(), shell)`

↓ BECAUSE: The loaded session has `session.agent = "claude"` from original creation
Evidence: `crates/kild-core/src/sessions/handler.rs:724-729` - session loaded from persistence

↓ ROOT CAUSE: BareShell arm in open_session doesn't set agent to `"shell"` like create_session does
Evidence: `crates/kild-core/src/sessions/handler.rs:75` - `("shell".to_string(), shell)` (correct pattern in create_session)

### Affected Files

| File                                           | Lines   | Action | Description                                        |
| ---------------------------------------------- | ------- | ------ | -------------------------------------------------- |
| `crates/kild-core/src/sessions/handler.rs`     | 759-760 | UPDATE | Fix BareShell arm to use `"shell"` instead of `session.agent` |

### Integration Points

- `crates/kild-core/src/sessions/handler.rs:930-941` - Daemon path AgentProcess::new receives the `agent` variable
- `crates/kild-core/src/sessions/handler.rs:977-988` - Terminal path AgentProcess::new receives the `agent` variable
- `crates/kild-core/src/sessions/info.rs:82` - Display logic reads `agent_proc.agent()`
- `crates/kild/src/commands/open.rs:33` - CLI calls `open_session()` with OpenMode

### Git History

- **Introduced**: `4ec14627` - 2026-02-05 - Renamed crate from shards-core to kild-core (code carried over from earlier)
- **Implication**: Original bug, present since BareShell support was added to open_session

---

## Implementation Plan

### Step 1: Fix BareShell agent name in open_session

**File**: `crates/kild-core/src/sessions/handler.rs`
**Lines**: 759-760
**Action**: UPDATE

**Current code:**

```rust
// Line 759-760
// Keep the session's original agent — no agent is actually running
(session.agent.clone(), shell)
```

**Required change:**

```rust
("shell".to_string(), shell)
```

**Why**: Match the same pattern used in `create_session` at line 75. The spawn record's agent field should reflect what's actually running (a bare shell), not the session's default agent.

---

## Patterns to Follow

**From codebase - mirror this exactly:**

```rust
// SOURCE: crates/kild-core/src/sessions/handler.rs:74-75
// Pattern for BareShell agent name in create_session
info!(event = "core.session.create_shell_selected", shell = %shell);
("shell".to_string(), shell)
```

---

## Edge Cases & Risks

| Risk/Edge Case                        | Mitigation                                                              |
| ------------------------------------- | ----------------------------------------------------------------------- |
| Session.agent field updated after open | open_session updates `session.agent` at line 993-994 - now correctly set to "shell" for this spawn |
| Existing sessions with wrong data     | Only affects new opens going forward; existing persisted data unchanged  |

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

1. Create a kild with agent: `kild create test-branch --agent claude`
2. Open bare shell: `kild open test-branch --no-agent`
3. Check status: `kild status test-branch --json` - verify second spawn shows `"agent": "shell"`

---

## Scope Boundaries

**IN SCOPE:**
- Fix the BareShell arm in `open_session` to use `"shell"` agent name
- Remove misleading comment

**OUT OF SCOPE (do not touch):**
- Session.agent field update logic (line 993-994) - works correctly once agent name is fixed
- create_session BareShell handling - already correct
- Existing persisted session data migration

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-11
- **Artifact**: `.claude/PRPs/issues/issue-288.md`
