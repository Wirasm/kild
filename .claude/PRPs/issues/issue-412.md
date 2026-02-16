# Investigation: Tab rename input: text not visible when editing

**Issue**: #412 (https://github.com/Wirasm/kild/issues/412)
**Type**: BUG
**Investigated**: 2026-02-16T12:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                                                                  |
| ---------- | ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Severity   | MEDIUM | Feature partially broken — rename works (Enter commits the typed text), but text is invisible during editing. Workaround: type blind and press Enter.       |
| Complexity | LOW    | Single file change (`terminal_tabs.rs`), isolated to one div container. No integration points affected.                                                    |
| Confidence | HIGH   | Root cause is clear from code analysis: Input uses `.size_full()` but parent div has no width constraint in a flex row, causing width collapse to near-zero. |

---

## Problem Statement

When clicking the active tab to rename it, the Input widget appears with a visible focus ring/border but typed text is not visible. The text only appears after pressing Enter to commit the rename. The root cause is that the rename container div has no width constraint, and the gpui-component Input calls `.size_full()` (width: 100%) which collapses to near-zero width when the parent has no defined width in a flex row.

---

## Analysis

### Root Cause

WHY 1: Why is typed text not visible in the rename input?
→ Because the Input widget renders at near-zero width — the border is visible but there's no room for text characters.
→ Evidence: `terminal_tabs.rs:265-276` — container div has `.flex()` but no width constraint.

WHY 2: Why does the Input have near-zero width?
→ Because gpui-component's `Input::render()` calls `.size_full()` (which is `w_full().h_full()`, i.e. `width: 100%`), and 100% of an unconstrained flex item is effectively zero.
→ Evidence: `gpui-component-0.5.1/src/input/input.rs:355` — `.size_full()`

WHY 3: Why doesn't the parent div have a width?
→ Because the rename container was styled to match the normal tab (padding, background, border) but missed a width constraint. Normal tabs don't need explicit width because they contain static text children that provide intrinsic sizing. The Input widget doesn't provide intrinsic sizing — it depends on its parent.
→ Evidence: Compare `terminal_tabs.rs:265-276` (rename, no width) with `terminal_tabs.rs:281-300` (normal tab, sizes to text content).

ROOT CAUSE: The rename container div at `terminal_tabs.rs:265` needs a minimum width so the Input (which uses `width: 100%`) has space to render text.

### Evidence Chain

WHY: Text invisible during tab rename
↓ BECAUSE: Input widget collapses to near-zero width
Evidence: `terminal_tabs.rs:265` — `div().flex().items_center()...` has no `.w()` or `.min_w()`

↓ BECAUSE: Input uses `.size_full()` (100% of parent) and parent has no width
Evidence: `gpui-component-0.5.1/src/input/input.rs:355` — `.size_full()`

↓ ROOT CAUSE: Missing width constraint on rename container div
Evidence: `terminal_tabs.rs:265-276` — no `.min_w()`, `.w()`, or `.flex_1()`

### Affected Files

| File                                          | Lines   | Action | Description                               |
| --------------------------------------------- | ------- | ------ | ----------------------------------------- |
| `crates/kild-ui/src/views/terminal_tabs.rs`   | 265-276 | UPDATE | Add min width to rename input container    |

### Integration Points

- `main_view.rs:1588-1607` — `start_rename()` creates InputState and stores in `renaming_tab`
- `main_view.rs:1655-1677` — `render_tab_bar()` passes `renaming_tab` to `TabBarContext`
- `main_view.rs:1778-1784` — Key handler routes Enter/Escape during rename
- No other callers or consumers are affected by this change.

### Git History

- **Introduced**: `30f5c68` - 2026-02-13 - "feat(ui): extract terminal tabs + keyboard navigation (Phase 2.5) (#414)"
- **Original feature**: `03d127c` - "feat(ui): Phase 2 — terminal multiplexer UX (#411)"
- **Implication**: Original bug — the rename container never had a width constraint.

---

## Implementation Plan

### Step 1: Add minimum width to rename input container

**File**: `crates/kild-ui/src/views/terminal_tabs.rs`
**Lines**: 265-276
**Action**: UPDATE

**Current code:**

```rust
// Lines 265-276
return div()
    .flex()
    .items_center()
    .px(px(theme::SPACE_2))
    .py(px(2.0))
    .rounded(px(theme::RADIUS_SM))
    .bg(theme::elevated())
    .border_b_2()
    .border_color(theme::ice())
    .text_size(px(theme::TEXT_SM))
    .child(Input::new(&input_state).cleanable(false))
    .into_any_element();
```

**Required change:**

```rust
// Lines 265-276
return div()
    .flex()
    .items_center()
    .min_w(px(120.0))
    .px(px(theme::SPACE_2))
    .py(px(2.0))
    .rounded(px(theme::RADIUS_SM))
    .bg(theme::elevated())
    .border_b_2()
    .border_color(theme::ice())
    .text_size(px(theme::TEXT_SM))
    .child(Input::new(&input_state).cleanable(false))
    .into_any_element();
```

**Why**: The Input widget uses `.size_full()` (width: 100%) internally. Without a defined parent width, it collapses to zero. Adding `.min_w(px(120.0))` gives the Input enough room to display text while keeping the tab compact. 120px accommodates ~15-20 characters at TEXT_SM (12px) size, sufficient for most tab names.

---

## Patterns to Follow

**From codebase — normal tab rendering for comparison:**

```rust
// SOURCE: terminal_tabs.rs:281-300
// Normal tab div — sizes to text content, no explicit width needed
div()
    .flex()
    .items_center()
    .gap(px(theme::SPACE_1))
    .px(px(theme::SPACE_2))
    .py(px(2.0))
    .rounded(px(theme::RADIUS_SM))
    .cursor_pointer()
    .when(is_active, |d| {
        d.bg(theme::elevated())
            .text_color(theme::text_bright())
            .border_b_2()
            .border_color(theme::ice())
    })
    // ... text child provides intrinsic width
    .text_size(px(theme::TEXT_SM))
    .child(label)
```

---

## Edge Cases & Risks

| Risk/Edge Case                           | Mitigation                                                                                     |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Very long tab names overflow 120px       | Input handles horizontal scrolling internally; user can still type and see cursor-local text    |
| Min width pushes other tabs off-screen   | 120px is smaller than most tab names; tab bar already handles overflow via flex wrap             |
| Input appearance doesn't match tab style | Input's `.appearance` is true by default, adding its own border/bg — consider `.appearance(false)` if needed |

---

## Validation

### Automated Checks

```bash
cargo fmt --check
cargo clippy --all -- -D warnings
cargo build -p kild-ui
```

### Manual Verification

1. Run `cargo run -p kild-ui`, select a kild with multiple terminals
2. Click the active tab — rename Input should appear with the current name visible
3. Type new characters — text should be visible as you type
4. Press Enter — rename commits with the new name
5. Press Escape — rename cancels, original name restored
6. Verify dialog inputs (Create, Add Project) still work correctly

---

## Scope Boundaries

**IN SCOPE:**

- Adding `min_w` to the rename input container div in `terminal_tabs.rs`

**OUT OF SCOPE (do not touch):**

- Dialog input styling (working correctly)
- Theme bridge configuration (working correctly)
- InputState initialization (`default_value` works correctly)
- Other tab bar styling

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-16T12:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-412.md`
