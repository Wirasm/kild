# Investigation: Fix session name matching and display issues in list command

**Issue**: #6 (https://github.com/Wirasm/shards/issues/6)
**Type**: BUG
**Investigated**: 2026-01-20T15:14:47.566+02:00

### Assessment

| Metric | Value | Reasoning |
|--------|-------|-----------|
| Severity | MEDIUM | Feature partially broken - sessions work but regression tests fail due to display truncation causing confusion about which names to use |
| Complexity | LOW | Single file change to adjust display formatting, no architectural changes needed |
| Confidence | HIGH | Clear root cause identified in display truncation logic with exact file and line numbers |

---

## Problem Statement

Sessions created with long names like `regression-test-claude-native-20260113-153448` are displayed as truncated `regression-te...` in `shards list`, causing the regression test to fail when trying to match created sessions in the list output.

---

## Analysis

### Root Cause / Change Rationale

The issue is purely cosmetic display truncation that doesn't affect the underlying functionality but breaks automated testing.

### Evidence Chain

WHY: Regression test fails to find created sessions in list output
↓ BECAUSE: Session names are truncated in display
  Evidence: `src/cli/commands.rs:140` - `truncate(&session.branch, 16)`

↓ BECAUSE: Table formatting limits branch names to 16 characters
  Evidence: `src/cli/commands.rs:225-230` - truncate function adds "..." when length > 16

↓ ROOT CAUSE: Hard-coded 16-character limit is too short for descriptive session names
  Evidence: `src/cli/commands.rs:140` - `truncate(&session.branch, 16)` truncates "regression-test-claude-native-20260113-153448" to "regression-te..."

### Affected Files

| File | Lines | Action | Description |
|------|-------|--------|-------------|
| `src/cli/commands.rs` | 140 | UPDATE | Increase branch name column width from 16 to 32 characters |
| `src/cli/commands.rs` | 120,150 | UPDATE | Adjust table header and footer to match new column width |

### Integration Points

- Session creation stores full names correctly in `src/sessions/handler.rs:95`
- Session persistence maintains full names in `src/sessions/operations.rs:155`
- Session lookup uses exact matching and works correctly in `src/sessions/operations.rs:295`
- Only the display layer truncates names for table formatting

### Git History

- **Introduced**: a19478fe - 2026-01-09 - "Initial table formatting with truncation"
- **Last modified**: c2e245a2 - 2026-01-19
- **Implication**: Original design decision to keep table compact, but real-world session names are longer

---

## Implementation Plan

### Step 1: Increase branch name column width

**File**: `src/cli/commands.rs`
**Lines**: 140
**Action**: UPDATE

**Current code:**
```rust
// Line 140
truncate(&session.branch, 16),
```

**Required change:**
```rust
truncate(&session.branch, 32),
```

**Why**: Allow longer session names to display without truncation

---

### Step 2: Update table header

**File**: `src/cli/commands.rs`
**Lines**: ~120 (table header)
**Action**: UPDATE

**Current code:**
```rust
const TABLE_HEADER: &str = "┌──────────────────┬─────────┬─────────┬─────────────────────┬─────────────┬─────────────┬──────────────────────┐";
const TABLE_SEPARATOR: &str = "│ Branch           │ Agent   │ Status  │ Created             │ Port Range  │ Process     │ Command              │";
```

**Required change:**
```rust
const TABLE_HEADER: &str = "┌────────────────────────────────────┬─────────┬─────────┬─────────────────────┬─────────────┬─────────────┬──────────────────────┐";
const TABLE_SEPARATOR: &str = "│ Branch                             │ Agent   │ Status  │ Created             │ Port Range  │ Process     │ Command              │";
```

**Why**: Expand branch column from 16 to 32 characters

---

### Step 3: Update table footer

**File**: `src/cli/commands.rs`
**Lines**: 150
**Action**: UPDATE

**Current code:**
```rust
const TABLE_BOTTOM: &str = "└──────────────────┴─────────┴─────────┴─────────────────────┴─────────────┴─────────────┴──────────────────────┘";
```

**Required change:**
```rust
const TABLE_BOTTOM: &str = "└────────────────────────────────────┴─────────┴─────────┴─────────────────────┴─────────────┴─────────────┴──────────────────────┘";
```

**Why**: Match the expanded branch column width

---

## Patterns to Follow

**From codebase - mirror these exactly:**

```rust
// SOURCE: src/cli/commands.rs:225-230
// Pattern for truncation logic - keep this unchanged
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        format!("{:<width$}", s, width = max_len)
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
|----------------|------------|
| Very long session names (>32 chars) | Still truncate with "..." - acceptable for extreme cases |
| Table alignment issues | Carefully count characters in header/footer borders |
| Terminal width constraints | 32 chars is reasonable for most terminals |

---

## Validation

### Automated Checks

```bash
cargo build
cargo test
scripts/regression-test.sh
```

### Manual Verification

1. Create session with long name: `cargo run -- create regression-test-claude-native-20260113-153448 --agent claude`
2. Run `cargo run -- list` and verify full name is visible (not truncated)
3. Verify regression test passes: `scripts/regression-test.sh`

---

## Scope Boundaries

**IN SCOPE:**
- Adjusting display column width for branch names
- Updating table formatting to accommodate longer names

**OUT OF SCOPE (do not touch):**
- Session storage/persistence logic (works correctly)
- Session matching logic (works correctly)
- Truncation function logic (works correctly)
- Other column widths (agent, status, etc.)

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-01-20T15:14:47.566+02:00
- **Artifact**: `.archon/artifacts/issues/issue-6.md`
