# Feature: GUI Phase 9.1 - Theme Foundation

## Summary

Create a centralized theme module for kild-ui that defines all color, typography, and spacing constants from the KILD brand system. This provides the foundation for consistent styling across all UI components and replaces the 100+ hardcoded `rgb()` calls scattered throughout the codebase.

## User Story

As a developer working on kild-ui
I want centralized theme constants with proper GPUI types
So that I can apply consistent branding and easily update colors across the entire UI

## Problem Statement

The current kild-ui codebase has colors hardcoded directly in view files:
- Over 100+ direct `rgb(0xXXXXXX)` calls across 6 view files
- Duplicate values (e.g., `0x4a9eff` for primary buttons appears in 4+ files)
- No named colors - must guess meaning from hex values
- No single source of truth for the brand palette
- Changing a color requires editing multiple files

## Solution Statement

Create a `theme.rs` module that:
1. Defines all brand colors as `Hsla` constants (GPUI's native color type)
2. Provides typography scale constants
3. Provides spacing scale constants
4. Exports a helper function to convert hex to `Hsla`
5. Is importable from any view file

## Metadata

| Field | Value |
|-------|-------|
| Type | NEW_CAPABILITY |
| Complexity | LOW |
| Systems Affected | kild-ui |
| Dependencies | gpui 0.2 (already in use) |
| Estimated Tasks | 3 |

---

## UX Design

### Before State

```
┌─────────────────────────────────────────────────────────────────────┐
│                         CURRENT STATE                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  main_view.rs:830     .bg(rgb(0x1e1e1e))                           │
│  main_view.rs:847     .text_color(rgb(0xffffff))                   │
│  main_view.rs:901     .bg(rgb(0x4a9eff))                           │
│  kild_list.rs:146     rgb(0x00ff00) // Green                       │
│  kild_list.rs:147     rgb(0xff0000) // Red                         │
│  create_dialog.rs:47  .bg(rgb(0x2d2d2d))                           │
│  create_dialog.rs:96  rgb(0x4a9eff) // focus border                │
│                                                                      │
│  PROBLEM: 100+ hardcoded values, no naming, duplicates everywhere   │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### After State

```
┌─────────────────────────────────────────────────────────────────────┐
│                          AFTER STATE                                 │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  theme.rs (NEW):                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  // Base surfaces                                            │   │
│  │  pub const VOID: Hsla = hsla_const(0x08090A);               │   │
│  │  pub const OBSIDIAN: Hsla = hsla_const(0x0E1012);           │   │
│  │  pub const SURFACE: Hsla = hsla_const(0x151719);            │   │
│  │  pub const ELEVATED: Hsla = hsla_const(0x1C1F22);           │   │
│  │                                                              │   │
│  │  // Accents                                                  │   │
│  │  pub const ICE: Hsla = hsla_const(0x38BDF8);                │   │
│  │  pub const AURORA: Hsla = hsla_const(0x34D399);             │   │
│  │  pub const COPPER: Hsla = hsla_const(0xFBBF24);             │   │
│  │  pub const EMBER: Hsla = hsla_const(0xF87171);              │   │
│  │  pub const KIRI: Hsla = hsla_const(0xA78BFA);               │   │
│  │  ...                                                         │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                      │
│  Views can now use:                                                  │
│  .bg(theme::SURFACE)                                                │
│  .text_color(theme::TEXT_BRIGHT)                                    │
│  .border_color(theme::ICE)                                          │
│                                                                      │
│  VALUE: Single source of truth, named colors, easy to update        │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| All view files | `rgb(0x1e1e1e)` | `theme::SURFACE` | Readable, maintainable code |
| Color updates | Edit 6+ files | Edit theme.rs only | Single source of truth |
| New components | Guess hex values | Use named constants | Consistent branding |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-ui/src/main.rs` | all | Module structure, how mods are declared |
| P0 | `crates/kild-ui/src/views/main_view.rs` | 820-920 | Current color usage pattern to replace |
| P0 | `crates/kild-ui/src/views/kild_list.rs` | 140-220 | Status color pattern |
| P0 | `crates/kild-ui/src/views/create_dialog.rs` | 1-120 | Dialog color pattern |
| P1 | `.claude/PRPs/branding/mockup-dashboard.html` | 11-75 | Definitive color values (CSS variables) |
| P1 | `.claude/PRPs/prds/gpui-native-terminal-ui.prd.md` | 1078-1200 | Phase 9.1 spec |

**External Documentation:**

| Source | Section | Why Needed |
|--------|---------|------------|
| [GPUI docs.rs](https://docs.rs/gpui/latest/gpui/) | Color types | `Hsla`, `rgb()`, `hsla()` API |
| [Zed color.rs](https://github.com/zed-industries/zed/blob/main/crates/gpui/src/color.rs) | Implementation | How `rgb()` converts to `Hsla` |

---

## Patterns to Mirror

**MODULE_DECLARATION:**
```rust
// SOURCE: crates/kild-ui/src/main.rs:10-14
// COPY THIS PATTERN:
mod actions;
mod projects;
mod refresh;
mod state;
mod views;
// ADD: mod theme;
```

**COLOR_USAGE:**
```rust
// SOURCE: crates/kild-ui/src/views/main_view.rs:830
// CURRENT PATTERN (to replace later):
.bg(rgb(0x1e1e1e))

// SOURCE: crates/kild-ui/src/views/main_view.rs:6
// GPUI IMPORTS:
use gpui::{Context, IntoElement, div, prelude::*, rgb};
```

**CONSTANT_DEFINITION:**
```rust
// GPUI provides rgb() which returns Hsla
// From docs.rs/gpui: pub fn rgb(hex: u32) -> Hsla

// For const definitions, we need a const fn approach
// Since rgb() is not const, we'll use a helper pattern
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/kild-ui/src/theme.rs` | CREATE | All theme constants |
| `crates/kild-ui/src/main.rs` | UPDATE | Add `mod theme;` and `pub use theme;` |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Light theme** - Dark theme only for MVP (per PRD)
- **Theme switching UI** - No runtime theme changes
- **Replacing existing colors in views** - That's Phase 9.6
- **Components (Button, TextInput, etc.)** - That's Phases 9.2-9.5

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: CREATE `crates/kild-ui/src/theme.rs`

- **ACTION**: Create the theme module with all color, typography, and spacing constants
- **IMPLEMENT**:
  - Color constants using `gpui::rgb()` function (returns Hsla)
  - Typography scale constants (f32 values)
  - Spacing scale constants (f32 values)
  - Border radius constants (f32 values)
  - Helper function for alpha variants

**Color Palette** (from mockup-dashboard.html CSS variables):

```rust
//! Theme constants for KILD UI.
//!
//! Color palette based on the "Tallinn Night" brand system.
//! All colors are GPUI Hsla values for direct use in styling.

use gpui::Hsla;

// =============================================================================
// COLOR PALETTE - Tallinn Night (Dark Theme)
// =============================================================================

// Base surfaces (darkest to lightest)
pub fn void() -> Hsla { gpui::rgb(0x08090A) }
pub fn obsidian() -> Hsla { gpui::rgb(0x0E1012) }
pub fn surface() -> Hsla { gpui::rgb(0x151719) }
pub fn elevated() -> Hsla { gpui::rgb(0x1C1F22) }

// Borders (subtle to strong)
pub fn border_subtle() -> Hsla { gpui::rgb(0x1F2328) }
pub fn border() -> Hsla { gpui::rgb(0x2D3139) }
pub fn border_strong() -> Hsla { gpui::rgb(0x3D434D) }

// Text (muted to brightest)
pub fn text_muted() -> Hsla { gpui::rgb(0x5C6370) }
pub fn text_subtle() -> Hsla { gpui::rgb(0x848D9C) }
pub fn text() -> Hsla { gpui::rgb(0xB8C0CC) }
pub fn text_bright() -> Hsla { gpui::rgb(0xE8ECF0) }
pub fn text_white() -> Hsla { gpui::rgb(0xF8FAFC) }

// Primary accent - Ice (for primary actions, focus states)
pub fn ice() -> Hsla { gpui::rgb(0x38BDF8) }
pub fn ice_dim() -> Hsla { gpui::rgb(0x0EA5E9) }
pub fn ice_bright() -> Hsla { gpui::rgb(0x7DD3FC) }

// Status - Aurora (active/running/success)
pub fn aurora() -> Hsla { gpui::rgb(0x34D399) }
pub fn aurora_dim() -> Hsla { gpui::rgb(0x10B981) }

// Status - Copper (stopped/warning/idle)
pub fn copper() -> Hsla { gpui::rgb(0xFBBF24) }
pub fn copper_dim() -> Hsla { gpui::rgb(0xD97706) }

// Status - Ember (error/crashed/danger)
pub fn ember() -> Hsla { gpui::rgb(0xF87171) }

// Agent indicator - Kiri (purple, for AI activity)
pub fn kiri() -> Hsla { gpui::rgb(0xA78BFA) }

// Secondary accent - Blade (for secondary actions)
pub fn blade() -> Hsla { gpui::rgb(0x64748B) }
pub fn blade_bright() -> Hsla { gpui::rgb(0x94A3B8) }

// =============================================================================
// GLOW EFFECTS (colors with alpha for shadows/glows)
// =============================================================================

/// Create a color with alpha for glow effects
pub fn with_alpha(color: Hsla, alpha: f32) -> Hsla {
    Hsla { a: alpha, ..color }
}

pub fn ice_glow() -> Hsla { with_alpha(ice(), 0.15) }
pub fn aurora_glow() -> Hsla { with_alpha(aurora(), 0.15) }
pub fn copper_glow() -> Hsla { with_alpha(copper(), 0.15) }
pub fn ember_glow() -> Hsla { with_alpha(ember(), 0.15) }
pub fn kiri_glow() -> Hsla { with_alpha(kiri(), 0.15) }

// =============================================================================
// TYPOGRAPHY SCALE
// =============================================================================

pub const TEXT_XS: f32 = 11.0;
pub const TEXT_SM: f32 = 12.0;
pub const TEXT_BASE: f32 = 13.0;
pub const TEXT_MD: f32 = 14.0;
pub const TEXT_LG: f32 = 16.0;
pub const TEXT_XL: f32 = 18.0;

// Font families (for reference - actual fonts set at app level)
pub const FONT_UI: &str = "Inter";
pub const FONT_MONO: &str = "JetBrains Mono";

// =============================================================================
// SPACING SCALE
// =============================================================================

pub const SPACE_1: f32 = 4.0;
pub const SPACE_2: f32 = 8.0;
pub const SPACE_3: f32 = 12.0;
pub const SPACE_4: f32 = 16.0;
pub const SPACE_5: f32 = 20.0;
pub const SPACE_6: f32 = 24.0;

// =============================================================================
// BORDER RADII
// =============================================================================

pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 6.0;
pub const RADIUS_LG: f32 = 8.0;

// =============================================================================
// OVERLAY
// =============================================================================

/// Semi-transparent overlay for modals (Void at 80% opacity)
pub fn overlay() -> Hsla {
    gpui::Hsla { h: 0.0, s: 0.0, l: 0.03, a: 0.8 }
}
```

- **GOTCHA**: GPUI's `rgb()` function is NOT const, so we use regular functions not `const` values. This is the standard pattern in GPUI codebases.
- **GOTCHA**: Use `gpui::rgb()` not `rgb()` to avoid import conflicts in view files that already import `rgb` from gpui.
- **VALIDATE**: `cargo build -p kild-ui`

### Task 2: UPDATE `crates/kild-ui/src/main.rs`

- **ACTION**: Add the theme module declaration
- **IMPLEMENT**: Add `mod theme;` after other mod declarations, add `pub use theme;` for external access

```rust
// Add after line 14 (after mod views;):
mod theme;

// For public access (optional, views can use crate::theme):
pub use theme;
```

- **MIRROR**: `crates/kild-ui/src/main.rs:10-14` - follow existing mod declaration pattern
- **VALIDATE**: `cargo build -p kild-ui`

### Task 3: VERIFY theme constants are accessible from views

- **ACTION**: Verify the theme module compiles and exports are accessible
- **IMPLEMENT**: Temporarily add a test import in main_view.rs (remove after verification)

```rust
// Temporary verification (add at top of main_view.rs, then remove):
use crate::theme;
// In render(), try: let _test = theme::ice();
```

- **VALIDATE**:
  ```bash
  cargo build -p kild-ui
  cargo clippy -p kild-ui -- -D warnings
  ```

---

## Testing Strategy

### Unit Tests to Write

No unit tests needed for this phase - it's pure constant definitions. Validation is done through compilation and type checking.

### Edge Cases Checklist

- [x] All colors from mockup-dashboard.html CSS included
- [x] Function-based colors (not const) since `rgb()` is not const
- [x] Alpha helper works with any color
- [x] No naming conflicts with GPUI's built-in color functions

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt -p kild-ui --check && cargo clippy -p kild-ui -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: BUILD

```bash
cargo build -p kild-ui
```

**EXPECT**: Compiles successfully, theme module exports accessible

### Level 3: FULL_SUITE

```bash
cargo build --all && cargo clippy --all -- -D warnings
```

**EXPECT**: All crates build, no warnings

---

## Acceptance Criteria

- [x] `theme.rs` created with all color constants from brand mockup
- [x] Typography scale constants defined (TEXT_XS through TEXT_XL)
- [x] Spacing scale constants defined (SPACE_1 through SPACE_6)
- [x] Border radius constants defined (RADIUS_SM, RADIUS_MD, RADIUS_LG)
- [x] Glow effect helpers (with_alpha, ice_glow, etc.)
- [x] Module exported from main.rs
- [x] `cargo build -p kild-ui` succeeds
- [x] `cargo clippy -p kild-ui -- -D warnings` passes

---

## Completion Checklist

- [ ] Task 1: theme.rs created with all constants
- [ ] Task 2: main.rs updated with mod declaration
- [ ] Task 3: Verified accessibility from views
- [ ] Level 1: `cargo fmt` and `cargo clippy` pass
- [ ] Level 2: `cargo build -p kild-ui` succeeds
- [ ] Level 3: Full workspace builds

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| `rgb()` not being const | HIGH | LOW | Use functions instead of const values (standard GPUI pattern) |
| Color values don't match mockup | LOW | MED | Cross-reference hex values directly from mockup-dashboard.html CSS |
| Import conflicts with gpui::rgb | MED | LOW | Use fully qualified `gpui::rgb()` in theme.rs |

---

## Notes

**Design Decision: Functions vs Constants**

GPUI's `rgb()` function is not const, so we cannot use `const` for color values. This is the same pattern used throughout the Zed codebase. Functions like `void()`, `ice()`, etc. are idiomatic.

**Color Source of Truth**

All hex values come directly from `.claude/PRPs/branding/mockup-dashboard.html` lines 11-75 (CSS custom properties). This is the definitive brand specification.

**Future Phases**

This theme module will be consumed by:
- Phase 9.2: Button component
- Phase 9.3: StatusIndicator component
- Phase 9.4: TextInput component
- Phase 9.5: Modal component
- Phase 9.6: Theme integration (replacing all hardcoded colors)

---

## Sources

- [GPUI docs.rs - Color types](https://docs.rs/gpui/latest/gpui/)
- [Zed GPUI color.rs](https://github.com/zed-industries/zed/blob/main/crates/gpui/src/color.rs)
- [GPUI Framework Overview](https://www.gpui.rs/)
- [Zed Themes Documentation](https://zed.dev/docs/themes)
