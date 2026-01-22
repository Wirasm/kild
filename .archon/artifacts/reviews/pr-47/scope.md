# PR Review Scope: #47

**Title**: feat: Close terminal windows when killing processes (#43)
**URL**: https://github.com/Wirasm/shards/pull/47
**Branch**: issue-43 → main
**Author**: Wirasm
**Date**: 2026-01-21

---

## Pre-Review Status

| Check | Status | Notes |
|-------|--------|-------|
| Merge Conflicts | ✅ None | MERGEABLE, CLEAN |
| CI Status | ⚠️ No checks | No CI checks configured |
| Behind Main | ✅ Up to date | 0 commits behind |
| Draft | ✅ Ready | Not a draft PR |
| Size | ✅ Normal | 8 files, +830 -3 lines |

---

## Changed Files

| File | Type | Additions | Deletions |
|------|------|-----------|-----------|
| `.archon/artifacts/issues/completed/issue-43.md` | artifact | +618 | -0 |
| `src/sessions/handler.rs` | source | +11 | -1 |
| `src/sessions/operations.rs` | test | +13 | -0 |
| `src/sessions/types.rs` | source | +61 | -1 |
| `src/terminal/errors.rs` | source | +4 | -0 |
| `src/terminal/handler.rs` | source | +24 | -0 |
| `src/terminal/operations.rs` | source | +97 | -0 |
| `src/terminal/types.rs` | source | +2 | -1 |

**Total**: 8 files, +830 -3

---

## File Categories

### Source Files (6)
- `src/sessions/handler.rs` - Session create/destroy logic
- `src/sessions/types.rs` - Session struct with new terminal_type field
- `src/terminal/errors.rs` - New TerminalCloseFailed error variant
- `src/terminal/handler.rs` - New close_terminal handler function
- `src/terminal/operations.rs` - AppleScript close templates and close_terminal_window function
- `src/terminal/types.rs` - Serde derives added to TerminalType

### Test Files (1)
- `src/sessions/operations.rs` - Updated test fixtures with terminal_type field

### Artifacts (1)
- `.archon/artifacts/issues/completed/issue-43.md` - Investigation documentation

---

## Review Focus Areas

Based on changes, reviewers should focus on:

1. **Code Quality**: Core logic in `operations.rs:close_terminal_window`, `handler.rs:close_terminal`, `sessions/handler.rs:destroy_session`
2. **Error Handling**: AppleScript failures, graceful fallback behavior, non-fatal close failures
3. **Test Coverage**: New backward compatibility tests, close script validation tests
4. **Type Safety**: TerminalType serialization, Option<TerminalType> handling
5. **Backward Compatibility**: Sessions without terminal_type field, serde defaults

---

## Key Implementation Details

### Changes Summary
1. Added `Serialize, Deserialize` derives to `TerminalType` enum
2. Added `terminal_type: Option<TerminalType>` field to `Session` struct
3. Added AppleScript close templates for iTerm, Terminal.app, Ghostty
4. Added `close_terminal_window()` function in operations
5. Added `close_terminal()` handler function
6. Added `TerminalCloseFailed` error variant
7. Updated `destroy_session()` to close terminal before killing process
8. Updated `restart_session()` to preserve terminal_type
9. Updated all test fixtures with terminal_type field

### Design Decisions
- Terminal close is best-effort (non-fatal)
- Close terminal BEFORE killing process
- Uses AppleScript (macOS only, non-macOS returns Ok immediately)
- Graceful handling of already-closed windows

---

## Metadata

- **Scope created**: 2026-01-21
- **Artifact path**: `.archon/artifacts/reviews/pr-47/`
