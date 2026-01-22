# Documentation Impact Findings: PR #47

**Reviewer**: docs-impact-agent
**Date**: 2026-01-21T14:45:00Z
**Docs Checked**: CLAUDE.md, docs/, .claude/agents/, .archon/commands/, README.md

---

## Summary

This PR adds terminal window auto-close functionality when destroying shards. The changes add a new `terminal_type` field to sessions and AppleScript-based terminal closing for macOS. The README.md has a minor documentation gap where the destroy behavior is not mentioned, but no critical updates are required since the feature is automatic and backward-compatible.

**Verdict**: UPDATES_REQUIRED

---

## Impact Assessment

| Document | Impact | Required Update |
|----------|--------|-----------------|
| CLAUDE.md | N/A | File does not exist in this project |
| README.md | LOW | Could mention terminal auto-close in "How It Works" section |
| docs/PROMPT_PIPING_PLAN.md | NONE | Unrelated to this feature |
| .claude/agents/*.md | NONE | No agents affected by this change |
| .archon/commands/*.md | NONE | No commands need updating - destroy behavior is enhanced but CLI unchanged |

---

## Findings

### Finding 1: README "How It Works" Section Missing Terminal Close Behavior

**Severity**: LOW
**Category**: incomplete-docs
**Document**: `README.md`
**PR Change**: `src/sessions/handler.rs:183-191` - Terminal close added before process kill

**Issue**:
The "How It Works" section describes the lifecycle but doesn't mention that terminal windows are closed during destruction. This is a minor omission since the feature is automatic and doesn't require user action.

**Current Documentation**:
```markdown
## How It Works

1. **Worktree Creation**: Creates a new Git worktree in `.shards/<name>` with a unique branch
2. **Agent Launch**: Launches the specified agent command in a native terminal window
3. **Session Tracking**: Records session metadata in `~/.shards/registry.json`
4. **Lifecycle Management**: Provides commands to monitor, stop, and clean up sessions
```

**Code Change**:
```rust
// 2. Close terminal window first (before killing process)
if let Some(ref terminal_type) = session.terminal_type {
    info!(event = "session.destroy_close_terminal", terminal_type = %terminal_type);
    // Best-effort - don't fail destroy if terminal close fails
    let _ = terminal::handler::close_terminal(terminal_type);
}
```

**Impact if Not Updated**:
Users won't know about the terminal auto-close feature, though they'll discover it naturally when using `shards destroy`. This is a quality-of-life improvement that works automatically.

---

#### Update Suggestions

| Option | Approach | Scope | Effort |
|--------|----------|-------|--------|
| A | Add brief mention in "How It Works" | Minimal change | LOW |
| B | No update needed | Feature is automatic | NONE |

**Recommended**: Option B (No Update)

**Reasoning**:
- The terminal close feature is automatic and doesn't require user knowledge to work
- The existing documentation accurately describes the commands; the internal behavior improvement doesn't change the user-facing API
- Adding too much implementation detail to README can make it harder to read
- Users will naturally observe terminal windows closing - no learning curve

**Optional Documentation Update** (if Option A preferred):
```markdown
## How It Works

1. **Worktree Creation**: Creates a new Git worktree in `.shards/<name>` with a unique branch
2. **Agent Launch**: Launches the specified agent command in a native terminal window
3. **Session Tracking**: Records session metadata in `~/.shards/registry.json`
4. **Lifecycle Management**: Provides commands to monitor, stop, and clean up sessions (terminal windows are automatically closed on destroy)
```

---

### Finding 2: Session Registry Schema Change Not Documented

**Severity**: LOW
**Category**: missing-docs
**Document**: `README.md`
**PR Change**: `src/sessions/types.rs:46-52` - Added `terminal_type` field to Session struct

**Issue**:
The Session struct now includes a `terminal_type` field stored in `~/.shards/registry.json`. This is a schema change, but it's backward-compatible (uses `#[serde(default)]`).

**Impact if Not Updated**:
Minimal impact. The registry.json format is an internal implementation detail not documented in README. Users don't manually edit this file.

**Verdict**: No documentation update needed - internal implementation detail.

---

## CLAUDE.md Sections to Update

| Section | Current | Needed Update |
|---------|---------|---------------|
| N/A | File does not exist | No CLAUDE.md in this project |

---

## Statistics

| Severity | Count | Documents Affected |
|----------|-------|-------------------|
| CRITICAL | 0 | - |
| HIGH | 0 | - |
| MEDIUM | 0 | - |
| LOW | 2 | README.md |

---

## New Documentation Needed

| Topic | Suggested Location | Priority |
|-------|-------------------|----------|
| None | - | - |

The feature is automatic and transparent to users. No new documentation sections needed.

---

## Positive Observations

1. **Investigation artifact included**: The PR includes comprehensive investigation documentation in `.archon/artifacts/issues/completed/issue-43.md` with full implementation details
2. **Backward compatibility ensured**: The `#[serde(default)]` attribute on `terminal_type` ensures existing sessions deserialize correctly
3. **Inline code documentation**: New functions have proper rustdoc comments explaining their purpose and behavior
4. **Test coverage**: New tests verify serialization and backward compatibility of the Session struct

---

## Metadata

- **Agent**: docs-impact-agent
- **Timestamp**: 2026-01-21T14:45:00Z
- **Artifact**: `.archon/artifacts/reviews/pr-47/docs-impact-findings.md`
