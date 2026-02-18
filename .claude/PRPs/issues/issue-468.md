# Investigation: feat: support terminal cursor shapes (Block, Beam, Underline, Hidden)

**Issue**: #468 (https://github.com/Wirasm/kild/issues/468)
**Type**: ENHANCEMENT
**Investigated**: 2026-02-18T00:00:00Z

### Assessment

| Metric     | Value  | Reasoning                                                                                                              |
| ---------- | ------ | ---------------------------------------------------------------------------------------------------------------------- |
| Priority   | MEDIUM | P2 label ("when time permits") — improves developer UX but no functional blocking, confirmed by issue labels           |
| Complexity | LOW    | Exactly 3 files change, all within `terminal_element/`; isolated to prepaint/paint/types with no external integration |
| Confidence | HIGH   | `content.cursor.shape` is already present in `RenderableCursor`, never read; all GPUI primitives exist (`fill`, `outline`) |

---

## Problem Statement

The terminal renderer reads `content.cursor.point` from alacritty_terminal but ignores `content.cursor.shape`. This means all cursor shape escape sequences (e.g., vim's block↔beam switching, shell vi-mode) are silently dropped, and the terminal always shows a filled block (focused) or 2px beam (unfocused) regardless of what the running program requests.

---

## Analysis

### Root Cause / Change Rationale

The `RenderableCursor` struct returned by `term.renderable_content()` has had a `shape: CursorShape` field all along. The prepaint code reads `.point` from it but never `.shape`. The fix is to read the shape, branch on it to compute appropriate bounds, and propagate it to the paint phase for shape-specific rendering (fill vs outline).

### Evidence Chain

WHY: Cursor shape doesn't change when vim switches modes
↓ BECAUSE: `PreparedCursor` has no shape field, so the painter always calls `fill()`
Evidence: `crates/kild-ui/src/terminal/terminal_element/types.rs:31-34`
```rust
pub(super) struct PreparedCursor {
    pub(super) bounds: Bounds<Pixels>,
    pub(super) color: Hsla,
    // no shape field
}
```

↓ BECAUSE: The prepaint code never reads `content.cursor.shape`
Evidence: `crates/kild-ui/src/terminal/terminal_element/prepaint.rs:403-431`
```rust
let cursor_point = content.cursor.point;  // shape is NEVER read
// ...
cursor = Some(PreparedCursor {
    bounds: if self.has_focus {
        // always full block
        Bounds::new(point(cx_pos, cy_pos), size(cursor_w, cell_height))
    } else {
        // always 2px beam
        Bounds::new(point(cx_pos, cy_pos), size(px(2.0), cell_height))
    },
    color: ...
});
```

↓ ROOT CAUSE: `content.cursor.shape` (type `alacritty_terminal::vte::ansi::CursorShape`) is available but unused
Evidence: `alacritty_terminal::term::mod.rs` — `RenderableCursor` struct:
```rust
pub struct RenderableCursor {
    pub shape: CursorShape,  // <-- exists, always populated
    pub point: Point,
}
```

### Affected Files

| File                                                               | Lines   | Action | Description                                          |
| ------------------------------------------------------------------ | ------- | ------ | ---------------------------------------------------- |
| `crates/kild-ui/src/terminal/terminal_element/types.rs`           | 1, 31-34 | UPDATE | Add `CursorShape` import and `shape` field to `PreparedCursor` |
| `crates/kild-ui/src/terminal/terminal_element/prepaint.rs`        | 1, 397-432 | UPDATE | Read `content.cursor.shape`, compute shape-specific bounds, handle Hidden |
| `crates/kild-ui/src/terminal/terminal_element/paint.rs`           | 1-5, 124-127 | UPDATE | Import `outline`/`BorderStyle`, match on shape for fill vs outline render |

### Integration Points

- `crates/kild-ui/src/terminal/terminal_view.rs:268` — constructs `has_focus`, passes to `TerminalElement::new()`
- `alacritty_terminal::term::renderable_content()` — returns `content.cursor: RenderableCursor { shape, point }`
- `gpui::fill()` — already imported in `paint.rs`, used for solid shapes
- `gpui::outline()` — needs to be added to imports in `paint.rs`, used for HollowBlock

### Git History

- No relevant history to inspect — the cursor rendering code has been in place since initial terminal implementation. This is a missing feature, not a regression.

---

## Implementation Plan

### Step 1: Add `shape` field to `PreparedCursor` in `types.rs`

**File**: `crates/kild-ui/src/terminal/terminal_element/types.rs`
**Lines**: 1 (add import), 31-34 (update struct)
**Action**: UPDATE

**Current code:**
```rust
// Line 1
use std::sync::LazyLock;

use gpui::{Bounds, Font, FontWeight, Hitbox, Hsla, Pixels, font};
// ...

// Lines 31-34
pub(super) struct PreparedCursor {
    pub(super) bounds: Bounds<Pixels>,
    pub(super) color: Hsla,
}
```

**Required change:**
```rust
// Line 1 — add import
use std::sync::LazyLock;

use alacritty_terminal::vte::ansi::CursorShape;
use gpui::{Bounds, Font, FontWeight, Hitbox, Hsla, Pixels, font};
// ...

// Lines 31-34 — add shape field
pub(super) struct PreparedCursor {
    pub(super) bounds: Bounds<Pixels>,
    pub(super) color: Hsla,
    pub(super) shape: CursorShape,
}
```

**Why**: The shape must flow from prepaint to paint so the painter can choose fill vs outline rendering.

---

### Step 2: Read and apply cursor shape in `prepaint.rs`

**File**: `crates/kild-ui/src/terminal/terminal_element/prepaint.rs`
**Lines**: 1-2 (add import), 397-432 (cursor block)
**Action**: UPDATE

**Current code (lines 1-2):**
```rust
use alacritty_terminal::term::cell::Flags as CellFlags;
use gpui::{App, Bounds, HitboxBehavior, Hsla, Pixels, Window, point, px, size};
```

**Required change (line 1 area):**
```rust
use alacritty_terminal::term::cell::Flags as CellFlags;
use alacritty_terminal::vte::ansi::CursorShape;
use gpui::{App, Bounds, HitboxBehavior, Hsla, Pixels, Window, point, px, size};
```

**Current code (lines 397-432):**
```rust
// Cursor (only when visible and terminal has cursor enabled via DECTCEM)
if self.cursor_visible
    && content
        .mode
        .contains(alacritty_terminal::term::TermMode::SHOW_CURSOR)
{
    let cursor_point = content.cursor.point;
    let cursor_line = cursor_point.line.0;
    let cursor_col = cursor_point.column.0;
    if cursor_line >= 0 && (cursor_line as usize) < rows && cursor_col < cols {
        let cx_pos = bounds.origin.x + cursor_col as f32 * cell_width;
        let cy_pos = bounds.origin.y + cursor_line as f32 * cell_height;
        let cursor_color = Hsla::from(theme::terminal_cursor());

        cursor = Some(PreparedCursor {
            bounds: if self.has_focus {
                let cursor_w = if cursor_is_wide {
                    cell_width + cell_width
                } else {
                    cell_width
                };
                Bounds::new(point(cx_pos, cy_pos), size(cursor_w, cell_height))
            } else {
                Bounds::new(point(cx_pos, cy_pos), size(px(2.0), cell_height))
            },
            color: if self.has_focus {
                cursor_color
            } else {
                Hsla {
                    a: 0.5,
                    ..cursor_color
                }
            },
        });
    }
}
```

**Required change:**
```rust
// Cursor (only when visible and terminal has cursor enabled via DECTCEM)
if self.cursor_visible
    && content
        .mode
        .contains(alacritty_terminal::term::TermMode::SHOW_CURSOR)
{
    let cursor_shape = content.cursor.shape;
    // Hidden cursor — nothing to render
    if cursor_shape != CursorShape::Hidden {
        let cursor_point = content.cursor.point;
        let cursor_line = cursor_point.line.0;
        let cursor_col = cursor_point.column.0;
        if cursor_line >= 0 && (cursor_line as usize) < rows && cursor_col < cols {
            let cx_pos = bounds.origin.x + cursor_col as f32 * cell_width;
            let cy_pos = bounds.origin.y + cursor_line as f32 * cell_height;
            let cursor_color = Hsla::from(theme::terminal_cursor());

            // When unfocused, always use HollowBlock at half opacity regardless of terminal shape.
            let (effective_shape, effective_color) = if self.has_focus {
                (cursor_shape, cursor_color)
            } else {
                (CursorShape::HollowBlock, Hsla { a: 0.5, ..cursor_color })
            };

            // Cell width for block-style shapes (respects wide characters).
            let cursor_w = if cursor_is_wide {
                cell_width + cell_width
            } else {
                cell_width
            };

            let cursor_bounds = match effective_shape {
                CursorShape::Block | CursorShape::HollowBlock => {
                    Bounds::new(point(cx_pos, cy_pos), size(cursor_w, cell_height))
                }
                CursorShape::Beam => {
                    Bounds::new(point(cx_pos, cy_pos), size(px(2.0), cell_height))
                }
                CursorShape::Underline => {
                    Bounds::new(
                        point(cx_pos, cy_pos + cell_height - px(2.0)),
                        size(cursor_w, px(2.0)),
                    )
                }
                // Hidden is already handled above; unreachable here.
                CursorShape::Hidden => unreachable!(),
            };

            cursor = Some(PreparedCursor {
                bounds: cursor_bounds,
                color: effective_color,
                shape: effective_shape,
            });
        }
    }
}
```

**Why**: Reads the shape from the terminal, computes appropriate pixel bounds for each shape, and stores the shape for the painter to dispatch on. When unfocused, overrides to HollowBlock (matching Zed's convention and the issue specification).

---

### Step 3: Render shape-specific cursor in `paint.rs`

**File**: `crates/kild-ui/src/terminal/terminal_element/paint.rs`
**Lines**: 2-5 (update imports), 124-127 (cursor layer)
**Action**: UPDATE

**Current code (lines 1-5):**
```rust
use alacritty_terminal::selection::{Selection, SelectionType};
use gpui::{
    App, Bounds, CursorStyle, DispatchPhase, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent,
    Pixels, SharedString, TextRun, Window, fill, point, px, size,
};
```

**Required change:**
```rust
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::vte::ansi::CursorShape;
use gpui::{
    App, BorderStyle, Bounds, CursorStyle, DispatchPhase, Hsla, MouseButton, MouseDownEvent,
    MouseMoveEvent, Pixels, SharedString, TextRun, Window, fill, outline, point, px, size,
};
```

**Current code (lines 124-127):**
```rust
// Layer 4: Cursor (topmost, always visible over text)
if let Some(cursor) = &prepaint.cursor {
    window.paint_quad(fill(cursor.bounds, cursor.color));
}
```

**Required change:**
```rust
// Layer 4: Cursor (topmost, always visible over text)
if let Some(cursor) = &prepaint.cursor {
    match cursor.shape {
        CursorShape::HollowBlock => {
            window.paint_quad(outline(cursor.bounds, cursor.color, BorderStyle::Solid));
        }
        _ => {
            window.paint_quad(fill(cursor.bounds, cursor.color));
        }
    }
}
```

**Why**: HollowBlock needs a rectangle outline with transparent fill; all other shapes (Block, Beam, Underline) use a solid fill at their pre-computed bounds.

---

## Patterns to Follow

**From codebase — mirror these exactly:**

```rust
// SOURCE: crates/kild-ui/src/terminal/colors.rs:3
// Pattern for importing from alacritty_terminal::vte::ansi
use alacritty_terminal::vte::ansi::{Color, NamedColor};
// → CursorShape follows same path: use alacritty_terminal::vte::ansi::CursorShape;
```

```rust
// SOURCE: crates/kild-ui/src/terminal/terminal_element/paint.rs:4
// Pattern for importing gpui drawing helpers
use gpui::{..., fill, point, px, size};
// → Add: outline, BorderStyle
```

---

## Edge Cases & Risks

| Risk/Edge Case | Mitigation |
| --- | --- |
| `CursorShape::Hidden` from SHOW_CURSOR=off (alacritty sets shape=Hidden when mode is off) | The existing `SHOW_CURSOR` gate already prevents entering the cursor block. The Hidden check inside is a belt-and-suspenders safety, since `content.cursor.shape` may be Hidden even when `SHOW_CURSOR` is set by some terminals. |
| Wide character + Beam/Underline cursor | Beam uses fixed `px(2.0)` width (no wide-char issue); Underline uses `cursor_w` which already accounts for wide chars — correct behavior |
| `unreachable!()` for Hidden in match | Safe: guarded by `cursor_shape != CursorShape::Hidden` check immediately above. Clippy will also catch this if the outer guard is ever removed. |
| `outline()` border width — 1px fixed | `gpui::outline()` hardcodes 1px. This matches Zed's HollowBlock rendering. If thicker borders are wanted later, switch to `quad()` with `border_widths`. |
| `BorderStyle` import availability | `BorderStyle` is a public type in `gpui`. Confirmed via `gpui::scene::BorderStyle` re-exported at crate root. |

---

## Validation

### Automated Checks

```bash
# From repo root
cargo fmt --check
cargo clippy -p kild-ui -- -D warnings
cargo build -p kild-ui
cargo test -p kild-ui
```

### Manual Verification

1. Open vim in a kild terminal — cursor should change from block (normal mode) to beam (insert mode)
2. Open fish/zsh with vi mode enabled — verify cursor changes on mode switch
3. Focus another window — terminal cursor should become a hollow block outline
4. Run a program that sets underline cursor (`printf '\e[3 q'`) — verify horizontal underline at cell bottom
5. Run `printf '\e[?25l'` (DECTCEM hide) — cursor should disappear

---

## Scope Boundaries

**IN SCOPE:**

- Read `content.cursor.shape` in prepaint
- Compute shape-specific pixel bounds (Block, Beam, Underline, HollowBlock, Hidden)
- Propagate shape through `PreparedCursor.shape`
- Match on shape in paint to choose fill vs outline

**OUT OF SCOPE (do not touch):**

- Cursor color — keep using `theme::terminal_cursor()` (not the terminal's color palette)
- Cursor blink — separate concern, no blink state currently exists
- `cursor_visible` field logic — already handled correctly
- `SHOW_CURSOR` (DECTCEM) mode gating — already handled correctly; don't remove or modify
- Any files outside `crates/kild-ui/src/terminal/terminal_element/`

---

## Metadata

- **Investigated by**: Claude
- **Timestamp**: 2026-02-18T00:00:00Z
- **Artifact**: `.claude/PRPs/issues/issue-468.md`
