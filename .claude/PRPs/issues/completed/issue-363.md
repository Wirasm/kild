# Investigation: kild list should not truncate column values

**Issue**: #363 (https://github.com/Wirasm/kild/issues/363)
**Type**: BUG
**Investigated**: 2026-02-11

### Assessment

| Metric     | Value  | Reasoning                                                                                       |
| ---------- | ------ | ----------------------------------------------------------------------------------------------- |
| Severity   | HIGH   | Core data display is broken for the primary CLI consumers (agents). Branch names, notes, and commands are silently cut off, making the output unreliable. |
| Complexity | LOW    | 2 files changed (table.rs + helpers.rs tests). Isolated to table formatting — no business logic or integration changes. |
| Confidence | HIGH   | Root cause is obvious: hardcoded column widths + truncate() calls. Clear evidence chain, no unknowns. |

---

## Problem Statement

`kild list` truncates branch names, notes, commands, timestamps, and other column values to fit hardcoded column widths (e.g., note: 30 chars, command: 20 chars, branch max: 50 chars). This discards information that agents and humans need. The fix: make each column width dynamic based on actual content, and stop truncating values.

---

## Analysis

### Root Cause

WHY: Column values like notes and commands appear as `"P2: Mouse selection and cop..."` instead of the full text
↓ BECAUSE: `truncate()` is called on every column value with fixed max widths
Evidence: `crates/kild/src/table.rs:116-141` - every column passed through `truncate(value, self.X_width)`

↓ BECAUSE: Column widths are hardcoded constants in `TableFormatter::new()`
Evidence: `crates/kild/src/table.rs:29-37`:
```rust
agent_width: 7,
status_width: 7,
activity_width: 8,
created_width: 19,
port_width: 11,
process_width: 11,
command_width: 20,
pr_width: 8,
note_width: 30,
```

↓ ROOT CAUSE: The `TableFormatter` uses fixed column widths and applies `truncate()` instead of sizing columns to fit actual data.
Evidence: Only `branch_width` is content-aware (lines 20-25), but even it is clamped to max 50 chars.

### Affected Files

| File                                   | Lines   | Action | Description                                                  |
| -------------------------------------- | ------- | ------ | ------------------------------------------------------------ |
| `crates/kild/src/table.rs`             | 1-243   | UPDATE | Make all column widths dynamic based on content, remove truncation |
| `crates/kild/src/commands/helpers.rs`  | 139-200 | UPDATE | Update truncation tests to reflect new behavior              |
| `crates/kild/src/commands/status.rs`   | 57-171  | UPDATE | Remove fixed-width box, use dynamic widths                   |

### Integration Points

- `crates/kild/src/commands/list.rs:60-61` creates `TableFormatter` and calls `print_table()`
- `crates/kild/src/commands/status.rs:8,70,96,133,135,138` imports and uses `truncate()`
- `crates/kild/src/commands/helpers.rs:142` imports `truncate` for tests
- `truncate()` is a public function used by both `list` and `status` commands

### Git History

- **Introduced**: `160314d` - Rebrand Shards to KILD (#110) — original table formatting
- **Last modified**: `1ec7713` - refactor: introduce ForgeBackend trait (#277) — added PR column
- **Implication**: Hardcoded widths are an original design choice, not a regression

---

## Implementation Plan

### Step 1: Make all column widths dynamic in `TableFormatter::new()`

**File**: `crates/kild/src/table.rs`
**Lines**: 5-39
**Action**: UPDATE

**Current code:**
```rust
pub struct TableFormatter {
    branch_width: usize,
    agent_width: usize,
    status_width: usize,
    activity_width: usize,
    created_width: usize,
    port_width: usize,
    process_width: usize,
    command_width: usize,
    pr_width: usize,
    note_width: usize,
}

impl TableFormatter {
    pub fn new(sessions: &[Session]) -> Self {
        let branch_width = sessions
            .iter()
            .map(|s| s.branch.len())
            .max()
            .unwrap_or(16)
            .clamp(6, 50);

        Self {
            branch_width,
            agent_width: 7,
            status_width: 7,
            activity_width: 8,
            created_width: 19,
            port_width: 11,
            process_width: 11,
            command_width: 20,
            pr_width: 8,
            note_width: 30,
        }
    }
```

**Required change:**

The constructor needs to accept pre-computed display values so it can measure them. However, the current design computes display values inside `print_row()` at render time. We need to restructure so that `TableFormatter::new()` receives enough info to compute widths.

The simplest approach: change `new()` to accept sessions + statuses + pr_infos (the same data `print_table` already receives), compute display strings, measure max widths, then store widths. The display strings will also be stored to avoid recomputing in `print_row()`.

Alternative simpler approach: compute column widths from the raw session data in `new()`, using the same display logic as `print_row()`. Store widths only (not display strings). `print_row()` still computes display strings but uses dynamic widths with no truncation.

**Go with the simpler approach** — compute max widths in `new()` by iterating sessions with the same formatting logic, then use those widths (with minimum = header length) in rendering. Remove `truncate()` calls from `print_row()`, replace with simple `{:<width$}` formatting.

```rust
pub struct TableFormatter {
    branch_width: usize,
    agent_width: usize,
    status_width: usize,
    activity_width: usize,
    created_width: usize,
    port_width: usize,
    process_width: usize,
    command_width: usize,
    pr_width: usize,
    note_width: usize,
}

impl TableFormatter {
    pub fn new(
        sessions: &[Session],
        statuses: &[Option<AgentStatusInfo>],
        pr_infos: &[Option<PrInfo>],
    ) -> Self {
        // Minimum widths = header label lengths
        let mut branch_width = "Branch".len();
        let mut agent_width = "Agent".len();
        let mut status_width = "Status".len();
        let mut activity_width = "Activity".len();
        let mut created_width = "Created".len();
        let mut port_width = "Port Range".len();
        let mut process_width = "Process".len();
        let mut command_width = "Command".len();
        let mut pr_width = "PR".len();
        let mut note_width = "Note".len();

        for (i, session) in sessions.iter().enumerate() {
            branch_width = branch_width.max(display_width(&session.branch));

            let agent_display = if session.agent_count() > 1 {
                format!(
                    "{} (+{})",
                    session.latest_agent().map_or(session.agent.as_str(), |a| a.agent()),
                    session.agent_count() - 1
                )
            } else {
                session.agent.clone()
            };
            agent_width = agent_width.max(display_width(&agent_display));

            let status_str = format!("{:?}", session.status).to_lowercase();
            status_width = status_width.max(display_width(&status_str));

            let activity = statuses.get(i)
                .and_then(|s| s.as_ref())
                .map_or("-".to_string(), |info| info.status.to_string());
            activity_width = activity_width.max(display_width(&activity));

            created_width = created_width.max(display_width(&session.created_at));

            let port_range = format!("{}-{}", session.port_range_start, session.port_range_end);
            port_width = port_width.max(display_width(&port_range));

            // Process status (simplified measurement — exact rendering happens in print_row)
            let process_str = Self::format_process_status(session);
            process_width = process_width.max(display_width(&process_str));

            let command = session.latest_agent().map_or("", |a| a.command());
            command_width = command_width.max(display_width(command));

            let pr_display = pr_infos.get(i)
                .and_then(|p| p.as_ref())
                .map_or("-".to_string(), |pr| match pr.state {
                    kild_core::PrState::Merged => "Merged".to_string(),
                    _ => format!("PR #{}", pr.number),
                });
            pr_width = pr_width.max(display_width(&pr_display));

            let note = session.note.as_deref().unwrap_or("");
            note_width = note_width.max(display_width(note));
        }

        Self {
            branch_width,
            agent_width,
            status_width,
            activity_width,
            created_width,
            port_width,
            process_width,
            command_width,
            pr_width,
            note_width,
        }
    }
```

Where `display_width()` counts character display width (accounting for wide CJK/emoji chars that take 2 cells — matching the recent `cd2b6d1` commit for wide character rendering).

**Why**: Dynamic widths ensure no data is truncated. Each column sizes to its widest value.

---

### Step 2: Remove `truncate()` calls from `print_row()`

**File**: `crates/kild/src/table.rs`
**Lines**: 114-152
**Action**: UPDATE

Replace all `truncate(value, self.X_width)` calls with `pad(value, self.X_width)` — a simple left-pad-to-width function that never truncates:

```rust
/// Pad a string to a minimum display width without truncating.
///
/// Uses Unicode display width to handle wide characters (CJK, emoji).
fn pad(s: &str, min_width: usize) -> String {
    let width = display_width(s);
    if width >= min_width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(min_width - width))
    }
}
```

In `print_row()`, replace every `truncate(&value, self.X_width)` with `pad(&value, self.X_width)`.

**Why**: The column width is already sized to fit the widest value, so no truncation is needed. Padding ensures shorter values align properly in the table.

---

### Step 3: Update `print_table()` signature and caller

**File**: `crates/kild/src/table.rs`
**Lines**: 41-54
**Action**: UPDATE

The `print_table()` method already receives `statuses` and `pr_infos`. No signature change needed — but since `new()` now takes these, the caller in `list.rs` must pass them to `new()`.

**File**: `crates/kild/src/commands/list.rs`
**Lines**: 60-61
**Action**: UPDATE

**Current code:**
```rust
let formatter = crate::table::TableFormatter::new(&sessions);
formatter.print_table(&sessions, &statuses, &pr_infos);
```

**Required change:**
```rust
let formatter = crate::table::TableFormatter::new(&sessions, &statuses, &pr_infos);
formatter.print_table(&sessions, &statuses, &pr_infos);
```

**Why**: `new()` needs status and PR data to compute accurate column widths.

---

### Step 4: Extract `format_process_status()` as a static method

**File**: `crates/kild/src/table.rs`
**Action**: UPDATE

Move the process status formatting logic out of `print_row()` into a `fn format_process_status(session: &Session) -> String` method so it can be called from both `new()` (for width measurement) and `print_row()` (for rendering). This avoids duplicating the process-check logic.

```rust
fn format_process_status(session: &Session) -> String {
    let mut running = 0;
    let mut errored = 0;
    for agent_proc in session.agents() {
        if let Some(pid) = agent_proc.process_id() {
            match kild_core::process::is_process_running(pid) {
                Ok(true) => running += 1,
                Ok(false) => {}
                Err(_) => errored += 1,
            }
        }
    }
    let total = session.agent_count();
    if total == 0 {
        "No PID".to_string()
    } else if errored > 0 {
        format!("{}run,{}err/{}", running, errored, total)
    } else {
        format!("Run({}/{})", running, total)
    }
}
```

**Why**: Avoids computing process status twice (once for width, once for display) and keeps the logic in one place.

---

### Step 5: Add `display_width()` utility function

**File**: `crates/kild/src/table.rs`
**Action**: UPDATE

Add a function that computes the terminal display width of a string, accounting for wide characters (CJK, emoji). Use the `unicode-width` crate which is already a transitive dependency (used by `kild-ui`).

```rust
use unicode_width::UnicodeWidthStr;

/// Compute the terminal display width of a string.
///
/// Wide characters (CJK, emoji) count as 2 columns.
fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}
```

Add `unicode-width` to `crates/kild/Cargo.toml` dependencies.

**Why**: The recent CJK rendering fix (`cd2b6d1`) established that wide characters take 2 cells. Column width calculations must account for this to keep table alignment correct.

---

### Step 6: Remove or repurpose `truncate()` function

**File**: `crates/kild/src/table.rs`
**Lines**: 230-243
**Action**: UPDATE

Remove the `truncate()` function. It's no longer used by `print_row()`.

Check if `status.rs` still needs it — if so, keep it but update `status.rs` to also use dynamic widths (see Step 7).

---

### Step 7: Update `kild status` to remove truncation

**File**: `crates/kild/src/commands/status.rs`
**Lines**: 57-171
**Action**: UPDATE

The `kild status` command uses a fixed 47-char value width with `truncate()` calls at lines 70, 96, 133, 135, 138. Apply the same principle: remove truncation, let the box expand to fit content.

Compute the max value width across all fields, then use that as the box width. Replace:
```rust
println!("│ Note:        {} │", truncate(note, 47));
```
with:
```rust
println!("│ Note:        {:<width$} │", note, width = value_width);
```

And compute the top/bottom borders dynamically:
```rust
let border = "─".repeat(label_width + value_width + 4); // +4 for "│ " and " │"
println!("┌{}┐", border);
```

**Why**: Same principle as `kild list` — status output also truncates notes, changes, PR info.

---

### Step 8: Update tests

**File**: `crates/kild/src/commands/helpers.rs`
**Lines**: 139-200
**Action**: UPDATE

The truncation tests in `helpers.rs` test the `truncate()` function. After changes:

1. If `truncate()` is removed entirely: remove the truncation tests (lines 144-200) and the `use crate::table::truncate;` import (line 142).
2. If `truncate()` is kept for status.rs: keep tests as-is but they test a function with narrowed scope.

Add new tests for `pad()` and `display_width()`:

```rust
#[test]
fn test_pad_shorter_than_width() {
    assert_eq!(pad("hi", 5), "hi   ");
}

#[test]
fn test_pad_exact_width() {
    assert_eq!(pad("hello", 5), "hello");
}

#[test]
fn test_pad_longer_than_width() {
    // Never truncates
    assert_eq!(pad("hello world", 5), "hello world");
}

#[test]
fn test_display_width_ascii() {
    assert_eq!(display_width("hello"), 5);
}

#[test]
fn test_display_width_cjk() {
    // CJK characters are 2 cells wide
    assert_eq!(display_width("日本"), 4);
}

#[test]
fn test_display_width_emoji() {
    // Emoji are typically 2 cells wide
    assert_eq!(display_width("🚀"), 2);
}
```

---

## Patterns to Follow

**From codebase — branch_width dynamic calculation pattern:**

```rust
// SOURCE: crates/kild/src/table.rs:20-25
// Pattern for computing column width from data
let branch_width = sessions
    .iter()
    .map(|s| s.branch.len())
    .max()
    .unwrap_or(16)
    .clamp(6, 50);
```

Extend this pattern to all columns but without the `.clamp()` upper bound. Use header label length as the minimum instead of a hardcoded number.

**From codebase — wide character handling (recent):**

```
// SOURCE: cd2b6d1 - fix: render wide characters (CJK/emoji) at double cell width
// The codebase already accounts for wide characters in terminal rendering.
// Use unicode-width crate for consistent behavior.
```

---

## Edge Cases & Risks

| Risk/Edge Case                        | Mitigation                                                                       |
| ------------------------------------- | -------------------------------------------------------------------------------- |
| Very long notes produce very wide table | This is the intended behavior per issue: "Let the table be as wide as it needs to be. If it wraps in a narrow terminal, that's fine." |
| Wide CJK/emoji characters misalign columns | Use `unicode-width` crate for display width calculations instead of `.len()` or `.chars().count()` |
| Empty session list (no data) | Minimum widths = header label lengths, handled by initializing with header widths |
| Process status computed twice (new + print_row) | Extract to `format_process_status()` static method, call from both places |
| `truncate()` removal breaks `status.rs` | Update status.rs in the same PR to use dynamic widths |

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

1. Create a kild with a long note: `kild create test-branch --note "This is a very long note that should not be truncated"`
2. Run `kild list` — verify note is fully visible, table is wider
3. Run `kild status test-branch` — verify note is fully visible
4. Run `kild list --json` — verify no change (already untruncated)
5. Verify table alignment is correct with sessions of varying data lengths

---

## Scope Boundaries

**IN SCOPE:**
- `crates/kild/src/table.rs` — dynamic column widths, remove truncation
- `crates/kild/src/commands/list.rs` — pass statuses/PR info to `TableFormatter::new()`
- `crates/kild/src/commands/status.rs` — remove fixed-width box, use dynamic widths
- `crates/kild/src/commands/helpers.rs` — update tests

**OUT OF SCOPE (do not touch):**
- `crates/kild/src/commands/stats.rs` — separate `truncate_str()` in fleet table, different issue
- `crates/kild-peek/src/table.rs` — independent table implementation for different tool
- JSON output (`--json`) — already untruncated, no changes needed
- Any kild-core business logic — this is purely a CLI display change

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-11
- **Artifact**: `.claude/PRPs/issues/issue-363.md`
