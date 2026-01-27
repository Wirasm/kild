# Feature: GUI Phase 9.2 - Button Component

## Summary

Create a reusable Button component for kild-ui that encapsulates all button variants (Primary, Secondary, Ghost, Success, Warning, Danger) with consistent styling from the brand system. This replaces 30+ inline button implementations scattered across view files with a single, type-safe component.

## User Story

As a developer working on kild-ui
I want a reusable Button component with typed variants
So that I can create consistent, branded buttons without duplicating styling code

## Problem Statement

The current kild-ui codebase has button styling duplicated across all view files:
- 30+ inline button implementations with slightly varying styles
- Inconsistent padding (some `px_2 py_1`, others `px_4 py_2`)
- Colors hardcoded directly in each button div
- No type safety for variants - easy to use wrong colors
- Disabled state logic repeated in multiple places
- Click handler wiring verbose and repetitive

## Solution Statement

Create a `Button` struct that implements GPUI's `RenderOnce` trait with:
1. A `ButtonVariant` enum for type-safe styling
2. Builder pattern for fluent configuration
3. Theme-based colors (depends on Phase 9.1)
4. Consistent padding/radius across all variants
5. Built-in disabled state support

## Metadata

| Field | Value |
|-------|-------|
| Type | NEW_CAPABILITY |
| Complexity | MEDIUM |
| Systems Affected | kild-ui |
| Dependencies | gpui 0.2, theme module (Phase 9.1) |
| Estimated Tasks | 4 |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  main_view.rs:                                                                ║
║  ┌────────────────────────────────────────────────────────────────────────┐  ║
║  │ div()                                                                   │  ║
║  │     .id("create-btn")                                                   │  ║
║  │     .px_3().py_1()                                                      │  ║
║  │     .bg(rgb(0x4a9eff))                                                  │  ║
║  │     .hover(|s| s.bg(rgb(0x5aafff)))                                     │  ║
║  │     .rounded_md().cursor_pointer()                                      │  ║
║  │     .on_mouse_up(gpui::MouseButton::Left, cx.listener(...))             │  ║
║  │     .child(div().text_color(rgb(0xffffff)).child("Create"))            │  ║
║  └────────────────────────────────────────────────────────────────────────┘  ║
║                                                                               ║
║  create_dialog.rs: (different padding!)                                       ║
║  ┌────────────────────────────────────────────────────────────────────────┐  ║
║  │ div()                                                                   │  ║
║  │     .id("create-btn")                                                   │  ║
║  │     .px_4().py_2()                      // Different padding!           │  ║
║  │     .bg(rgb(0x4a9eff))                                                  │  ║
║  │     ...                                                                 │  ║
║  └────────────────────────────────────────────────────────────────────────┘  ║
║                                                                               ║
║  PROBLEM: 30+ implementations, inconsistent, no type safety                   ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  components/button.rs (NEW):                                                   ║
║  ┌────────────────────────────────────────────────────────────────────────┐  ║
║  │ pub enum ButtonVariant { Primary, Secondary, Ghost, Success, ... }      │  ║
║  │                                                                         │  ║
║  │ pub struct Button {                                                     │  ║
║  │     id: ElementId,                                                      │  ║
║  │     label: SharedString,                                                │  ║
║  │     variant: ButtonVariant,                                             │  ║
║  │     disabled: bool,                                                     │  ║
║  │     on_click: Option<...>,                                              │  ║
║  │ }                                                                       │  ║
║  │                                                                         │  ║
║  │ impl RenderOnce for Button {                                            │  ║
║  │     fn render(self, ...) -> impl IntoElement { ... }                    │  ║
║  │ }                                                                       │  ║
║  └────────────────────────────────────────────────────────────────────────┘  ║
║                                                                               ║
║  Usage in views:                                                               ║
║  ┌────────────────────────────────────────────────────────────────────────┐  ║
║  │ Button::new("create-btn", "Create")                                     │  ║
║  │     .variant(ButtonVariant::Primary)                                    │  ║
║  │     .on_click(cx.listener(|view, _, cx| view.on_create(cx)))            │  ║
║  └────────────────────────────────────────────────────────────────────────┘  ║
║                                                                               ║
║  VALUE: Type-safe, consistent, 10x less code per button                       ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| All views | 15 lines per button | 3 lines per button | Cleaner code |
| Color changes | Edit 30+ places | Edit button.rs only | Single source |
| New button types | Copy-paste + modify | Add enum variant | Type safety |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-ui/src/theme.rs` | all | Theme colors to use (depends on 9.1) |
| P0 | `crates/kild-ui/src/views/main_view.rs` | 616-658 | `render_bulk_button` - closest to component pattern |
| P0 | `crates/kild-ui/src/views/main_view.rs` | 896-919 | Primary button pattern |
| P0 | `crates/kild-ui/src/views/confirm_dialog.rs` | 126-142 | Danger button pattern |
| P1 | `crates/kild-ui/src/views/kild_list.rs` | 258-282 | Small/compact button pattern |
| P1 | `.claude/PRPs/branding/mockup-dashboard.html` | 152-225 | Button CSS specifications |

**External Documentation:**

| Source | Section | Why Needed |
|--------|---------|------------|
| [GPUI docs.rs - RenderOnce](https://docs.rs/gpui/latest/gpui/trait.RenderOnce.html) | Trait definition | How to implement component |
| [GPUI docs.rs - IntoElement](https://docs.rs/gpui/latest/gpui/trait.IntoElement.html) | Derive macro | Required for component |
| [longbridge/gpui-component Button](https://github.com/longbridge/gpui-component) | Button example | Reference implementation |

---

## Patterns to Mirror

**MODULE_STRUCTURE:**
```rust
// SOURCE: crates/kild-ui/src/views/mod.rs:1-8
// COPY THIS PATTERN for components/mod.rs:
pub mod add_project_dialog;
pub mod confirm_dialog;
pub mod create_dialog;
pub mod kild_list;
pub mod main_view;
pub mod project_selector;

pub use main_view::MainView;
```

**CLICK_HANDLER_PATTERN:**
```rust
// SOURCE: crates/kild-ui/src/views/main_view.rs:887-891
// COPY THIS PATTERN:
.on_mouse_up(
    gpui::MouseButton::Left,
    cx.listener(|view, _, _, cx| {
        view.on_refresh_click(cx);
    }),
)
```

**DISABLED_STATE_PATTERN:**
```rust
// SOURCE: crates/kild-ui/src/views/main_view.rs:636-648
// COPY THIS PATTERN:
let is_disabled = count == 0;
let bg_color = if is_disabled { rgb(0x333333) } else { rgb(enabled_bg) };
let hover_color = if is_disabled { rgb(0x333333) } else { rgb(enabled_hover) };
let text_color = if is_disabled { rgb(0x666666) } else { rgb(0xffffff) };

div()
    // ...
    .when(!is_disabled, |d| d.hover(|style| style.bg(hover_color)))
    .when(!is_disabled, |d| d.cursor_pointer())
    .when(!is_disabled, |d| {
        d.on_mouse_up(gpui::MouseButton::Left, on_click)
    })
```

**CONDITIONAL_RENDERING:**
```rust
// SOURCE: crates/kild-ui/src/views/kild_list.rs:343-368
// COPY THIS PATTERN:
.when(!is_running, |row| {
    row.child(...)
})
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/kild-ui/src/components/mod.rs` | CREATE | Components module root |
| `crates/kild-ui/src/components/button.rs` | CREATE | Button component |
| `crates/kild-ui/src/main.rs` | UPDATE | Add `mod components;` |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Replacing existing buttons in views** - That's Phase 9.6
- **Icon-only buttons** - Use label with emoji/symbol for now
- **Loading state** - Not needed for MVP
- **Button groups** - YAGNI
- **Dropdown buttons** - Separate component if needed

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: CREATE `crates/kild-ui/src/components/mod.rs`

- **ACTION**: Create the components module root
- **IMPLEMENT**:

```rust
//! Reusable UI components for kild-ui.
//!
//! This module contains extracted, styled components that ensure
//! visual consistency across the application.

mod button;

pub use button::{Button, ButtonVariant};
```

- **VALIDATE**: `cargo build -p kild-ui` (will fail until button.rs exists)

### Task 2: CREATE `crates/kild-ui/src/components/button.rs`

- **ACTION**: Create the Button component with all variants
- **IMPLEMENT**:

```rust
//! Button component with themed variants.
//!
//! Provides consistent button styling across the application.
//! All colors come from the theme module.

use gpui::{
    div, prelude::*, px, ClickEvent, ElementId, Hsla, IntoElement, MouseButton,
    RenderOnce, SharedString, Window, App,
};

use crate::theme;

/// Button style variants matching the brand system.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Primary action - Ice background, used for main CTAs
    #[default]
    Primary,
    /// Secondary action - Surface background with border
    Secondary,
    /// Ghost button - Transparent, text only
    Ghost,
    /// Success action - Aurora (green) background
    Success,
    /// Warning action - Copper (yellow) background
    Warning,
    /// Danger action - Ember (red) for destructive actions
    Danger,
}

impl ButtonVariant {
    /// Get the background color for this variant.
    fn bg_color(&self, disabled: bool) -> Hsla {
        if disabled {
            return theme::surface();
        }
        match self {
            ButtonVariant::Primary => theme::ice(),
            ButtonVariant::Secondary => theme::surface(),
            ButtonVariant::Ghost => gpui::transparent_black(),
            ButtonVariant::Success => theme::aurora(),
            ButtonVariant::Warning => theme::copper(),
            ButtonVariant::Danger => gpui::transparent_black(),
        }
    }

    /// Get the hover background color for this variant.
    fn hover_color(&self) -> Hsla {
        match self {
            ButtonVariant::Primary => theme::ice_bright(),
            ButtonVariant::Secondary => theme::elevated(),
            ButtonVariant::Ghost => theme::surface(),
            ButtonVariant::Success => theme::aurora_dim(),
            ButtonVariant::Warning => theme::copper_dim(),
            ButtonVariant::Danger => theme::ember_glow(),
        }
    }

    /// Get the text color for this variant.
    fn text_color(&self, disabled: bool) -> Hsla {
        if disabled {
            return theme::text_muted();
        }
        match self {
            ButtonVariant::Primary => theme::void(),
            ButtonVariant::Secondary => theme::text(),
            ButtonVariant::Ghost => theme::text_subtle(),
            ButtonVariant::Success => theme::void(),
            ButtonVariant::Warning => theme::void(),
            ButtonVariant::Danger => theme::ember(),
        }
    }

    /// Get the border color for this variant (if any).
    fn border_color(&self) -> Option<Hsla> {
        match self {
            ButtonVariant::Secondary => Some(theme::border()),
            ButtonVariant::Danger => Some(theme::ember()),
            _ => None,
        }
    }
}

/// A styled button component.
///
/// # Example
///
/// ```rust
/// Button::new("create-btn", "Create")
///     .variant(ButtonVariant::Primary)
///     .on_click(cx.listener(|view, _, cx| {
///         view.on_create(cx);
///     }))
/// ```
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    variant: ButtonVariant,
    disabled: bool,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl Button {
    /// Create a new button with the given ID and label.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            variant: ButtonVariant::default(),
            disabled: false,
            on_click: None,
        }
    }

    /// Set the button variant (styling).
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set whether the button is disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Set the click handler.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let bg = self.variant.bg_color(self.disabled);
        let hover_bg = self.variant.hover_color();
        let text = self.variant.text_color(self.disabled);
        let border = self.variant.border_color();
        let disabled = self.disabled;
        let on_click = self.on_click;

        let mut button = div()
            .id(self.id)
            .px(px(theme::SPACE_3))
            .py(px(theme::SPACE_2))
            .bg(bg)
            .rounded(px(theme::RADIUS_MD))
            .child(
                div()
                    .text_color(text)
                    .child(self.label),
            );

        // Apply border if variant has one
        if let Some(border_color) = border {
            button = button.border_1().border_color(border_color);
        }

        // Apply hover and click only when not disabled
        if !disabled {
            button = button
                .hover(|style| style.bg(hover_bg))
                .cursor_pointer();

            if let Some(handler) = on_click {
                button = button.on_click(handler);
            }
        }

        button
    }
}
```

- **GOTCHA**: Use `on_click` method from GPUI's `InteractiveElement` trait (takes `ClickEvent`), not `on_mouse_up` with `MouseButton::Left`
- **GOTCHA**: The `#[derive(IntoElement)]` macro requires implementing `RenderOnce`
- **GOTCHA**: Theme functions return `Hsla`, which is what GPUI expects for colors
- **VALIDATE**: `cargo build -p kild-ui`

### Task 3: UPDATE `crates/kild-ui/src/main.rs`

- **ACTION**: Add the components module declaration
- **IMPLEMENT**: Add `mod components;` after other mod declarations

```rust
// Add after line 14 (after mod views;):
mod components;

// For public access:
pub use components::{Button, ButtonVariant};
```

- **MIRROR**: `crates/kild-ui/src/main.rs:10-14` - follow existing mod declaration pattern
- **VALIDATE**: `cargo build -p kild-ui`

### Task 4: VERIFY Button component is accessible and renders

- **ACTION**: Verify the component compiles and can be imported
- **IMPLEMENT**: Temporarily add a test usage in a view (remove after verification)

```rust
// Temporary verification in main_view.rs (remove after):
use crate::components::{Button, ButtonVariant};

// In render():
// let _test = Button::new("test", "Test").variant(ButtonVariant::Primary);
```

- **VALIDATE**:
  ```bash
  cargo build -p kild-ui
  cargo clippy -p kild-ui -- -D warnings
  ```

---

## Testing Strategy

### Unit Tests to Write

No unit tests for this phase - it's a visual component. Validation is through:
1. Compilation (type safety)
2. Visual inspection when integrated (Phase 9.6)

### Edge Cases Checklist

- [x] All 6 variants have distinct styling
- [x] Disabled state dims colors and removes interactivity
- [x] Click handler is optional (buttons can be display-only)
- [x] Labels support any SharedString content (emoji, unicode)
- [x] Empty label creates a valid (small) button

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

**EXPECT**: Compiles successfully, Button and ButtonVariant exported

### Level 3: FULL_SUITE

```bash
cargo build --all && cargo clippy --all -- -D warnings
```

**EXPECT**: All crates build, no warnings

---

## Acceptance Criteria

- [x] `ButtonVariant` enum with 6 variants (Primary, Secondary, Ghost, Success, Warning, Danger)
- [x] `Button` struct with builder pattern (new, variant, disabled, on_click)
- [x] `RenderOnce` implemented correctly
- [x] Colors sourced from theme module (not hardcoded hex)
- [x] Disabled state dims colors and removes hover/click
- [x] Border applied only for Secondary and Danger variants
- [x] `cargo build -p kild-ui` succeeds
- [x] `cargo clippy -p kild-ui -- -D warnings` passes

---

## Completion Checklist

- [ ] Task 1: components/mod.rs created
- [ ] Task 2: components/button.rs created with full implementation
- [ ] Task 3: main.rs updated with mod declaration
- [ ] Task 4: Verified accessibility from views
- [ ] Level 1: `cargo fmt` and `cargo clippy` pass
- [ ] Level 2: `cargo build -p kild-ui` succeeds
- [ ] Level 3: Full workspace builds

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| `#[derive(IntoElement)]` macro issues | MED | MED | Manual impl if derive fails |
| Theme module not ready (Phase 9.1) | HIGH | HIGH | Must complete 9.1 first |
| `on_click` signature mismatch | MED | LOW | Check GPUI docs for exact signature |
| Missing GPUI imports | LOW | LOW | Add imports as compiler errors indicate |

---

## Notes

**Design Decision: on_click vs on_mouse_up**

GPUI provides both `on_click` (which handles `ClickEvent`) and `on_mouse_up` (which handles `MouseUpEvent`). The `on_click` is cleaner and handles both mouse and potentially keyboard activation. However, existing code uses `on_mouse_up`. The new component will use `on_click` for simplicity, but views may need adjustment when integrating.

**Design Decision: No Icon Support Initially**

The PRD suggests icon support, but for simplicity the initial implementation uses labels only. Icons can be added by using emoji/symbols in the label (e.g., "+" for create, "×" for close). A dedicated icon prop can be added later if needed.

**Dependency on Phase 9.1**

This component REQUIRES the theme module from Phase 9.1. The theme functions (`ice()`, `aurora()`, etc.) must exist before this can compile. Ensure Phase 9.1 is complete first.

**Button Sizing**

The PRD mockup shows two padding sizes:
- Header buttons: `px_3 py_1` (compact)
- Dialog buttons: `px_4 py_2` (comfortable)

This implementation uses `SPACE_3` / `SPACE_2` as a middle ground. A `size` prop could be added later for explicit control.

---

## Sources

- [GPUI docs.rs - RenderOnce](https://docs.rs/gpui/latest/gpui/trait.RenderOnce.html)
- [GPUI docs.rs - IntoElement](https://docs.rs/gpui/latest/gpui/trait.IntoElement.html)
- [longbridge/gpui-component](https://github.com/longbridge/gpui-component) - Reference implementation
- [Zed Button Component](https://github.com/zed-industries/zed/blob/main/crates/ui/src/components/button/button.rs) - Pattern reference
