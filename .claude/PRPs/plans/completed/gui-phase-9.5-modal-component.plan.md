# Feature: GUI Phase 9.5 - Modal Component

## Summary

Create a reusable Modal component that encapsulates the overlay + centered dialog box pattern currently duplicated across create_dialog.rs, confirm_dialog.rs, and add_project_dialog.rs. The Modal component will use theme constants and provide a consistent structure with header/body/footer sections.

## User Story

As a developer working on kild-ui
I want a reusable Modal component with themed styling
So that I can create consistent dialogs without duplicating overlay/centering/styling code

## Problem Statement

The current kild-ui codebase has three dialog implementations (create, confirm, add_project) that all duplicate:
- Overlay pattern: `div().absolute().inset_0().bg(rgba(0x000000aa)).flex().justify_center().items_center()`
- Dialog box pattern: `div().w(px(400.0)).bg(rgb(0x2d2d2d)).rounded_lg().border_1().border_color(rgb(0x444444)).flex_col()`
- Header pattern: `div().px_4().py_3().border_b_1().child(title)`
- Footer pattern: `div().px_4().py_3().border_t_1().flex().justify_end().gap_2()`
- Hardcoded colors: `0x2d2d2d`, `0x444444`, `0x000000aa`

## Solution Statement

Create a Modal component that:
1. Renders the overlay + centered dialog box automatically
2. Uses theme constants (elevated, border_subtle, overlay)
3. Accepts title (required), body (required), and footer (optional) as builder params
4. Follows the same `RenderOnce` pattern as the existing Button component
5. Can be used to simplify existing dialogs in Phase 9.6

## Metadata

| Field | Value |
|-------|-------|
| Type | NEW_CAPABILITY |
| Complexity | MEDIUM |
| Systems Affected | kild-ui/components |
| Dependencies | gpui 0.2, theme module (Phase 9.1) |
| Estimated Tasks | 3 |

---

## UX Design

### Before State

```
┌─────────────────────────────────────────────────────────────────────┐
│                         CURRENT STATE                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  create_dialog.rs:34-52                                             │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │ div()                                                         │   │
│  │   .id("dialog-overlay")                                       │   │
│  │   .absolute().inset_0()                                       │   │
│  │   .bg(gpui::rgba(0x000000aa))  // hardcoded                   │   │
│  │   .flex().justify_center().items_center()                     │   │
│  │   .child(                                                     │   │
│  │     div().w(px(400.0)).bg(rgb(0x2d2d2d))  // hardcoded       │   │
│  │       .rounded_lg().border_1().border_color(rgb(0x444444))   │   │
│  │       .flex_col()                                             │   │
│  │       .child(/* header */)                                    │   │
│  │       .child(/* body */)                                      │   │
│  │       .child(/* footer */)                                    │   │
│  │   )                                                           │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                      │
│  confirm_dialog.rs:28-46   ← SAME PATTERN, DUPLICATED               │
│  add_project_dialog.rs     ← SAME PATTERN, DUPLICATED               │
│                                                                      │
│  PROBLEM: 3 files with identical overlay/dialog structure           │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### After State

```
┌─────────────────────────────────────────────────────────────────────┐
│                          AFTER STATE                                 │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  components/modal.rs (NEW):                                         │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │ pub struct Modal {                                            │   │
│  │     id: ElementId,                                            │   │
│  │     title: SharedString,                                      │   │
│  │     body: AnyElement,                                         │   │
│  │     footer: Option<AnyElement>,                               │   │
│  │     width: Pixels,                                            │   │
│  │ }                                                             │   │
│  │                                                               │   │
│  │ impl Modal {                                                  │   │
│  │     pub fn new(id, title) -> Self;                           │   │
│  │     pub fn body(self, body: impl IntoElement) -> Self;       │   │
│  │     pub fn footer(self, footer: impl IntoElement) -> Self;   │   │
│  │     pub fn width(self, width: Pixels) -> Self;               │   │
│  │ }                                                             │   │
│  │                                                               │   │
│  │ impl RenderOnce for Modal {                                   │   │
│  │     // Renders overlay + centered dialog with theme colors   │   │
│  │ }                                                             │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                      │
│  Usage (future Phase 9.6):                                          │
│  Modal::new("create-dialog", "Create New KILD")                     │
│      .body(form_fields_element)                                     │
│      .footer(buttons_element)                                       │
│                                                                      │
│  VALUE: Single source of truth for modal styling                    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| New dialogs | Copy 50+ lines of overlay code | `Modal::new().body().footer()` | Much simpler |
| Theme updates | Edit 3 files with hardcoded colors | Edit theme.rs only | Consistency |
| Dialog width | Hardcoded `px(400.0)` | Configurable via `.width()` | Flexibility |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-ui/src/components/button.rs` | all | Pattern to MIRROR exactly - RenderOnce, builder pattern |
| P0 | `crates/kild-ui/src/views/create_dialog.rs` | 33-66, 228-274 | Current modal structure to extract |
| P0 | `crates/kild-ui/src/views/confirm_dialog.rs` | 27-60, 97-143 | Simpler modal example |
| P1 | `crates/kild-ui/src/theme.rs` | all | Theme constants to use |
| P1 | `crates/kild-ui/src/components/mod.rs` | all | How to export new component |

**External Documentation:**

| Source | Section | Why Needed |
|--------|---------|------------|
| [GPUI docs.rs](https://docs.rs/gpui/latest/gpui/) | AnyElement, IntoElement | Understanding element composition |

---

## Patterns to Mirror

**COMPONENT_STRUCT:**
```rust
// SOURCE: crates/kild-ui/src/components/button.rs:100-107
// COPY THIS PATTERN:
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    variant: ButtonVariant,
    disabled: bool,
    on_click: Option<ClickHandler>,
}
```

**BUILDER_PATTERN:**
```rust
// SOURCE: crates/kild-ui/src/components/button.rs:109-140
// COPY THIS PATTERN:
impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            variant: ButtonVariant::default(),
            disabled: false,
            on_click: None,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
    // ... more builders
}
```

**RENDER_ONCE_IMPL:**
```rust
// SOURCE: crates/kild-ui/src/components/button.rs:143-175
// COPY THIS PATTERN:
impl RenderOnce for Button {
    fn render(self, _window: &mut Window, _cx: &mut gpui::App) -> impl IntoElement {
        // Build the element tree
        div()
            .id(self.id)
            // ... styling
    }
}
```

**OVERLAY_PATTERN:**
```rust
// SOURCE: crates/kild-ui/src/views/create_dialog.rs:34-41
// Pattern to encapsulate:
div()
    .id("dialog-overlay")
    .absolute()
    .inset_0()
    .bg(gpui::rgba(0x000000aa))
    .flex()
    .justify_center()
    .items_center()
```

**DIALOG_BOX_PATTERN:**
```rust
// SOURCE: crates/kild-ui/src/views/create_dialog.rs:43-52
// Pattern to encapsulate:
div()
    .id("dialog-box")
    .w(px(400.0))
    .bg(rgb(0x2d2d2d))
    .rounded_lg()
    .border_1()
    .border_color(rgb(0x444444))
    .flex()
    .flex_col()
```

**HEADER_PATTERN:**
```rust
// SOURCE: crates/kild-ui/src/views/create_dialog.rs:54-66
// Pattern to encapsulate:
div()
    .px_4()
    .py_3()
    .border_b_1()
    .border_color(rgb(0x444444))
    .child(
        div()
            .text_lg()
            .text_color(rgb(0xffffff))
            .child("Title"),
    )
```

**FOOTER_PATTERN:**
```rust
// SOURCE: crates/kild-ui/src/views/create_dialog.rs:229-237
// Pattern to encapsulate:
div()
    .px_4()
    .py_3()
    .border_t_1()
    .border_color(rgb(0x444444))
    .flex()
    .justify_end()
    .gap_2()
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/kild-ui/src/components/modal.rs` | CREATE | Modal component implementation |
| `crates/kild-ui/src/components/mod.rs` | UPDATE | Export Modal |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Dismiss handling** - Modal is rendering-only; dismiss (Escape, click outside) remains in MainView's key handler
- **Refactoring existing dialogs** - That's Phase 9.6; this phase just creates the component
- **Animation** - No fade-in/fade-out animations for MVP
- **Multiple modal stacking** - Only one modal visible at a time (existing behavior)
- **Focus trap** - Keyboard focus management is handled by existing dialog code

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: CREATE `crates/kild-ui/src/components/modal.rs`

- **ACTION**: Create the Modal component following the Button pattern
- **IMPLEMENT**:

```rust
//! Modal component for dialog overlays.
//!
//! Provides consistent modal styling with overlay, centered dialog box,
//! and header/body/footer structure. All colors come from the theme module.

use gpui::{
    AnyElement, ElementId, IntoElement, Pixels, RenderOnce, SharedString, Window, div, prelude::*,
    px,
};

use crate::theme;

/// Default modal width (400px, matching existing dialogs).
const DEFAULT_WIDTH: f32 = 400.0;

/// A styled modal dialog component.
///
/// Modal renders:
/// - Semi-transparent overlay covering the screen
/// - Centered dialog box with themed styling
/// - Header with title and bottom border
/// - Body content area
/// - Optional footer with top border (typically for buttons)
///
/// # Example
///
/// ```ignore
/// Modal::new("create-dialog", "Create New KILD")
///     .body(
///         div().flex_col().gap_4()
///             .child(/* form fields */)
///     )
///     .footer(
///         div().flex().justify_end().gap_2()
///             .child(Button::new("cancel", "Cancel").variant(ButtonVariant::Secondary))
///             .child(Button::new("create", "Create").variant(ButtonVariant::Primary))
///     )
/// ```
#[derive(IntoElement)]
pub struct Modal {
    id: ElementId,
    title: SharedString,
    body: Option<AnyElement>,
    footer: Option<AnyElement>,
    width: Pixels,
}

impl Modal {
    /// Create a new modal with the given ID and title.
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            body: None,
            footer: None,
            width: px(DEFAULT_WIDTH),
        }
    }

    /// Set the body content of the modal.
    pub fn body(mut self, body: impl IntoElement) -> Self {
        self.body = Some(body.into_any_element());
        self
    }

    /// Set the footer content of the modal (typically buttons).
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Set a custom width for the modal (default: 400px).
    pub fn width(mut self, width: impl Into<Pixels>) -> Self {
        self.width = width.into();
        self
    }
}

impl RenderOnce for Modal {
    fn render(self, _window: &mut Window, _cx: &mut gpui::App) -> impl IntoElement {
        // Overlay: covers entire screen with semi-transparent background
        div()
            .id(self.id.clone())
            .absolute()
            .inset_0()
            .bg(theme::overlay())
            .flex()
            .justify_center()
            .items_center()
            // Dialog box: centered, themed container
            .child(
                div()
                    .id(ElementId::Name(format!("{}-box", self.id).into()))
                    .w(self.width)
                    .bg(theme::elevated())
                    .rounded(px(theme::RADIUS_LG))
                    .border_1()
                    .border_color(theme::border())
                    .flex()
                    .flex_col()
                    // Header: title with bottom border
                    .child(
                        div()
                            .px(px(theme::SPACE_4))
                            .py(px(theme::SPACE_3))
                            .border_b_1()
                            .border_color(theme::border_subtle())
                            .child(
                                div()
                                    .text_size(px(theme::TEXT_LG))
                                    .text_color(theme::text_bright())
                                    .child(self.title),
                            ),
                    )
                    // Body: main content area
                    .when_some(self.body, |this, body| {
                        this.child(
                            div()
                                .px(px(theme::SPACE_4))
                                .py(px(theme::SPACE_4))
                                .child(body),
                        )
                    })
                    // Footer: optional, typically for action buttons
                    .when_some(self.footer, |this, footer| {
                        this.child(
                            div()
                                .px(px(theme::SPACE_4))
                                .py(px(theme::SPACE_3))
                                .border_t_1()
                                .border_color(theme::border_subtle())
                                .child(footer),
                        )
                    }),
            )
    }
}
```

- **MIRROR**: `crates/kild-ui/src/components/button.rs` - RenderOnce pattern
- **IMPORTS**: Follow button.rs import style
- **GOTCHA**: Use `AnyElement` for body/footer to accept any element type; use `.into_any_element()` in builders
- **GOTCHA**: Clone `self.id` for use in the inner box ID
- **VALIDATE**: `cargo build -p kild-ui`

### Task 2: UPDATE `crates/kild-ui/src/components/mod.rs`

- **ACTION**: Add modal module and exports
- **IMPLEMENT**:

```rust
//! Reusable UI components for kild-ui.
//!
//! This module contains extracted, styled components that ensure
//! visual consistency across the application.

mod button;
mod modal;
mod status_indicator;

pub use button::{Button, ButtonVariant};
pub use modal::Modal;
#[allow(unused_imports)]
pub use status_indicator::{Status, StatusIndicator, StatusSize};
```

- **MIRROR**: Existing pattern in mod.rs
- **VALIDATE**: `cargo build -p kild-ui`

### Task 3: VERIFY Modal renders correctly

- **ACTION**: Verify Modal component compiles and can be imported
- **IMPLEMENT**: Temporarily add test usage in main_view.rs (remove after verification)

```rust
// Temporary verification (in imports):
use crate::components::Modal;

// In render() method, add temporarily to verify it works:
// .when(false, |this| {
//     this.child(
//         Modal::new("test-modal", "Test Modal")
//             .body(div().child("Test body content"))
//             .footer(div().child("Test footer"))
//     )
// })
```

- **VALIDATE**:
  ```bash
  cargo build -p kild-ui
  cargo clippy -p kild-ui -- -D warnings
  ```

---

## Testing Strategy

### Unit Tests to Write

No unit tests needed for this phase - it's a UI component. Validation is done through compilation and visual verification.

### Edge Cases Checklist

- [x] Modal with no body (just title + footer)
- [x] Modal with no footer (just title + body)
- [x] Modal with custom width
- [x] Modal with default width (400px)
- [x] Theme colors applied correctly (overlay, elevated, border, text_bright)

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

**EXPECT**: Compiles successfully, Modal exported from components

### Level 3: FULL_SUITE

```bash
cargo build --all && cargo clippy --all -- -D warnings && cargo test --all
```

**EXPECT**: All crates build, no warnings, tests pass

### Level 5: VISUAL_VALIDATION

```bash
cargo run -p kild-ui
# Temporarily render a Modal to verify:
# - Overlay covers screen with correct opacity
# - Dialog box is centered
# - Header, body, footer have correct spacing
# - Colors match theme (elevated bg, border, text_bright title)
```

---

## Acceptance Criteria

- [x] `modal.rs` created with Modal struct implementing RenderOnce
- [x] Modal uses theme colors (overlay, elevated, border, border_subtle, text_bright)
- [x] Modal uses theme spacing constants (SPACE_3, SPACE_4, RADIUS_LG)
- [x] Builder pattern for body, footer, and width
- [x] Exported from components/mod.rs
- [x] `cargo build -p kild-ui` succeeds
- [x] `cargo clippy -p kild-ui -- -D warnings` passes

---

## Completion Checklist

- [ ] Task 1: modal.rs created with full implementation
- [ ] Task 2: mod.rs updated with modal export
- [ ] Task 3: Verified accessibility from views
- [ ] Level 1: `cargo fmt` and `cargo clippy` pass
- [ ] Level 2: `cargo build -p kild-ui` succeeds
- [ ] Level 3: Full workspace builds and tests pass

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| AnyElement type complexity | MED | LOW | Follow GPUI patterns from existing code, use `.into_any_element()` |
| ElementId cloning issues | LOW | LOW | Use `.clone()` on ElementId as it implements Clone |
| Theme overlay() not visible | LOW | MED | Verify theme::overlay() returns correct rgba value (0x08090ACC) |

---

## Notes

**Design Decision: Rendering-Only Component**

The Modal component is intentionally rendering-only. It does NOT handle:
- Escape key dismissal
- Click-outside dismissal
- State management (show_dialog boolean)

These remain in MainView's existing handlers. This keeps Modal simple and follows GPUI's unidirectional data flow pattern.

**Future Usage (Phase 9.6)**

In Phase 9.6, existing dialogs will be refactored to use Modal:

```rust
// create_dialog.rs (future)
Modal::new("create-dialog", "Create New KILD")
    .body(
        div().flex_col().gap_4()
            .child(/* branch input */)
            .child(/* agent selector */)
            .child(/* note input */)
            .when_some(error, |this, e| this.child(/* error display */))
    )
    .footer(
        div().flex().justify_end().gap_2()
            .child(Button::new("cancel", "Cancel").variant(ButtonVariant::Secondary).on_click(...))
            .child(Button::new("create", "Create").variant(ButtonVariant::Primary).on_click(...))
    )
```

**Width Flexibility**

The default 400px width matches existing dialogs. The `.width()` builder allows customization for wider dialogs (e.g., add_project uses 450px).

---

## Sources

- [GPUI docs.rs](https://docs.rs/gpui/latest/gpui/) - AnyElement, IntoElement, RenderOnce
- Existing Button component: `crates/kild-ui/src/components/button.rs`
- Existing dialogs: `crates/kild-ui/src/views/create_dialog.rs`, `confirm_dialog.rs`
