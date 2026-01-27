# Feature: GUI Phase 9.4 - TextInput Component

## Summary

Create a reusable TextInput component for kild-ui that encapsulates the text input styling pattern currently duplicated in create_dialog.rs. The component will use theme colors, display placeholder text, show a cursor when focused, and support read-only display mode. Note: keyboard input handling remains in the parent view (MainView) as GPUI's input model is parent-driven.

## User Story

As a developer working on kild-ui
I want a reusable TextInput component with consistent styling
So that all text inputs across dialogs look identical and I don't duplicate styling code

## Problem Statement

The create_dialog.rs file contains duplicated text input styling patterns:
- Branch name input (lines 88-116) and Note input (lines 184-212) have nearly identical code
- Both manually handle: background, border, focus border color, placeholder vs value display, cursor
- Colors are still hardcoded (`rgb(0x1e1e1e)`, `rgb(0x4a9eff)`, etc.) instead of using theme
- Any styling change requires updating multiple places

## Solution Statement

Create a TextInput component that:
1. Renders a styled input field with theme colors
2. Accepts value, placeholder, and focus state as props
3. Displays cursor (`|`) when focused
4. Shows placeholder in muted color when empty
5. Mirrors the Button component's builder pattern and structure

**Important Design Decision**: TextInput is a **display-only component**. Keyboard input is handled by the parent view (MainView.on_key_down) because GPUI's input model is parent-driven - the parent owns state and processes key events, then passes updated values to child components.

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
│  create_dialog.rs:88-116 (Branch Name):                             │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  div()                                                       │   │
│  │    .px_3().py_2()                                           │   │
│  │    .bg(rgb(0x1e1e1e))          ← hardcoded                  │   │
│  │    .border_color(if is_focused {                            │   │
│  │        rgb(0x4a9eff)           ← hardcoded                  │   │
│  │    } else {                                                  │   │
│  │        rgb(0x555555)           ← hardcoded                  │   │
│  │    })                                                        │   │
│  │    .child(/* placeholder/value/cursor logic */)             │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                      │
│  create_dialog.rs:184-212 (Note):                                   │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  /* IDENTICAL PATTERN DUPLICATED */                         │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                      │
│  PROBLEM: Duplicated code, hardcoded colors, no reusability         │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### After State

```
┌─────────────────────────────────────────────────────────────────────┐
│                          AFTER STATE                                 │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  components/text_input.rs (NEW):                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  TextInput::new("branch-input")                             │   │
│  │    .value(&branch_name)                                      │   │
│  │    .placeholder("Type branch name...")                       │   │
│  │    .focused(is_focused)                                      │   │
│  │                                                              │   │
│  │  // Internally uses:                                         │   │
│  │  // - theme::surface() for background                        │   │
│  │  // - theme::ice() for focus border                          │   │
│  │  // - theme::border() for normal border                      │   │
│  │  // - theme::text_muted() for placeholder                    │   │
│  │  // - theme::text_bright() for value                         │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                      │
│  create_dialog.rs (SIMPLIFIED - Phase 9.6):                         │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  TextInput::new("branch-input")                             │   │
│  │    .value(&branch_name)                                      │   │
│  │    .placeholder("Type branch name...")                       │   │
│  │    .focused(focused_field == CreateDialogField::BranchName)  │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                      │
│  VALUE: Single source of truth, theme colors, zero duplication      │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| create_dialog.rs | 60+ lines of duplicated styling | `TextInput::new(...)` one-liner | Cleaner code |
| Theme colors | Hardcoded hex values | `theme::surface()`, `theme::ice()` | Consistent branding |
| New dialogs | Copy-paste styling code | Use TextInput component | Faster development |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-ui/src/components/button.rs` | all | Pattern to MIRROR exactly |
| P0 | `crates/kild-ui/src/components/mod.rs` | all | Export pattern |
| P0 | `crates/kild-ui/src/theme.rs` | all | Color functions to use |
| P1 | `crates/kild-ui/src/views/create_dialog.rs` | 88-116, 184-212 | Current input patterns |
| P1 | `.claude/PRPs/prds/gpui-native-terminal-ui.prd.md` | 1390-1454 | Phase 9.4 spec |

**External Documentation:**

| Source | Section | Why Needed |
|--------|---------|------------|
| [GPUI docs.rs](https://docs.rs/gpui/latest/gpui/) | IntoElement, RenderOnce | Component traits |

---

## Patterns to Mirror

**COMPONENT_STRUCTURE (from Button):**
```rust
// SOURCE: crates/kild-ui/src/components/button.rs:97-104
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

**BUILDER_PATTERN (from Button):**
```rust
// SOURCE: crates/kild-ui/src/components/button.rs:106-137
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
    // ... more builder methods
}
```

**RENDER_ONCE_IMPL (from Button):**
```rust
// SOURCE: crates/kild-ui/src/components/button.rs:140-173
// COPY THIS PATTERN:
impl RenderOnce for Button {
    fn render(self, _window: &mut Window, _cx: &mut gpui::App) -> impl IntoElement {
        // Build and return the div element
    }
}
```

**CURRENT_INPUT_STYLING (to extract):**
```rust
// SOURCE: crates/kild-ui/src/views/create_dialog.rs:88-116
// EXTRACT THIS PATTERN:
div()
    .px_3()
    .py_2()
    .bg(rgb(0x1e1e1e))
    .rounded_md()
    .border_1()
    .border_color(if is_focused {
        rgb(0x4a9eff)
    } else {
        rgb(0x555555)
    })
    .min_h(px(36.0))
    .child(
        div()
            .text_color(if branch_name.is_empty() {
                rgb(0x666666)
            } else {
                rgb(0xffffff)
            })
            .child(if branch_name.is_empty() {
                "Type branch name...".to_string()
            } else if is_focused {
                format!("{}|", branch_name)
            } else {
                branch_name.clone()
            }),
    )
```

**MODULE_EXPORT (from mod.rs):**
```rust
// SOURCE: crates/kild-ui/src/components/mod.rs:6-8
// COPY THIS PATTERN:
mod button;

pub use button::{Button, ButtonVariant};
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/kild-ui/src/components/text_input.rs` | CREATE | TextInput component |
| `crates/kild-ui/src/components/mod.rs` | UPDATE | Export TextInput |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Keyboard handling in component** - Parent view owns input handling (GPUI pattern)
- **Focus management** - Parent passes `focused` bool, component just renders
- **on_change/on_submit callbacks** - Not needed; parent handles all events
- **Updating create_dialog.rs to use TextInput** - That's Phase 9.6
- **Disabled state** - Not needed for current use cases (can add later)

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: CREATE `crates/kild-ui/src/components/text_input.rs`

- **ACTION**: Create the TextInput component file
- **IMPLEMENT**:

```rust
//! TextInput component with themed styling.
//!
//! A display-only text input that renders value, placeholder, and cursor.
//! Keyboard input is handled by the parent view.

use gpui::{ElementId, IntoElement, RenderOnce, SharedString, Window, div, prelude::*, px};

use crate::theme;

/// A styled text input component.
///
/// This is a display-only component - keyboard input handling remains
/// in the parent view. The component renders:
/// - Themed background and border
/// - Placeholder text when empty (muted color)
/// - Value text when not empty (bright color)
/// - Cursor indicator (`|`) when focused
///
/// # Example
///
/// ```ignore
/// TextInput::new("branch-input")
///     .value(&branch_name)
///     .placeholder("Type branch name...")
///     .focused(is_branch_focused)
/// ```
#[derive(IntoElement)]
pub struct TextInput {
    id: ElementId,
    value: String,
    placeholder: SharedString,
    focused: bool,
}

impl TextInput {
    /// Create a new text input with the given ID.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            value: String::new(),
            placeholder: SharedString::default(),
            focused: false,
        }
    }

    /// Set the current value to display.
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    /// Set the placeholder text shown when value is empty.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Set whether the input is currently focused.
    ///
    /// When focused, the border color changes to Ice and a cursor is shown.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl RenderOnce for TextInput {
    fn render(self, _window: &mut Window, _cx: &mut gpui::App) -> impl IntoElement {
        let is_empty = self.value.is_empty();

        // Determine what text to display
        let display_text = if is_empty {
            self.placeholder.to_string()
        } else if self.focused {
            format!("{}|", self.value)
        } else {
            self.value.clone()
        };

        // Determine text color
        let text_color = if is_empty {
            theme::text_muted()
        } else {
            theme::text_bright()
        };

        // Determine border color based on focus
        let border_color = if self.focused {
            theme::ice()
        } else {
            theme::border()
        };

        div()
            .id(self.id)
            .px(px(theme::SPACE_3))
            .py(px(theme::SPACE_2))
            .bg(theme::surface())
            .rounded(px(theme::RADIUS_MD))
            .border_1()
            .border_color(border_color)
            .min_h(px(36.0))
            .child(
                div()
                    .text_color(text_color)
                    .child(display_text)
            )
    }
}
```

- **MIRROR**: `crates/kild-ui/src/components/button.rs` - follow exact structure
- **IMPORTS**: `use crate::theme;` for colors
- **GOTCHA**: Use `theme::surface()` not `theme::obsidian()` - surface is the standard input background per mockup
- **VALIDATE**: `cargo build -p kild-ui`

### Task 2: UPDATE `crates/kild-ui/src/components/mod.rs`

- **ACTION**: Add TextInput module and export
- **IMPLEMENT**:

```rust
//! Reusable UI components for kild-ui.
//!
//! This module contains extracted, styled components that ensure
//! visual consistency across the application.

mod button;
mod text_input;

pub use button::{Button, ButtonVariant};
pub use text_input::TextInput;
```

- **MIRROR**: `crates/kild-ui/src/components/mod.rs:6-8` - follow existing pattern
- **VALIDATE**: `cargo build -p kild-ui`

### Task 3: VERIFY TextInput is accessible and compiles

- **ACTION**: Verify the component compiles and is accessible
- **IMPLEMENT**: Build and run clippy to ensure no warnings

```bash
cargo build -p kild-ui
cargo clippy -p kild-ui -- -D warnings
```

- **VALIDATE**: Both commands exit 0 with no errors or warnings

---

## Testing Strategy

### Unit Tests to Write

No unit tests needed for this phase - it's a pure rendering component. Validation is done through compilation and visual inspection when used in Phase 9.6.

### Edge Cases Checklist

- [x] Empty value shows placeholder
- [x] Non-empty value shows value
- [x] Focused + non-empty shows cursor
- [x] Focused + empty shows placeholder (no cursor needed for empty)
- [x] Not focused shows value without cursor

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

**EXPECT**: Compiles successfully, TextInput exported

### Level 3: FULL_SUITE

```bash
cargo build --all && cargo clippy --all -- -D warnings && cargo test --all
```

**EXPECT**: All crates build, no warnings, tests pass

---

## Acceptance Criteria

- [x] `text_input.rs` created with TextInput struct
- [x] TextInput implements IntoElement via `#[derive(IntoElement)]`
- [x] TextInput implements RenderOnce trait
- [x] Builder methods: `new()`, `value()`, `placeholder()`, `focused()`
- [x] Uses theme colors: `surface()`, `ice()`, `border()`, `text_muted()`, `text_bright()`
- [x] Shows cursor (`|`) when focused and has value
- [x] Shows placeholder when empty
- [x] Exported from `components/mod.rs`
- [x] `cargo build -p kild-ui` succeeds
- [x] `cargo clippy -p kild-ui -- -D warnings` passes

---

## Completion Checklist

- [ ] Task 1: text_input.rs created with full implementation
- [ ] Task 2: mod.rs updated with export
- [ ] Task 3: Build and clippy verification passed
- [ ] Level 1: `cargo fmt` and `cargo clippy` pass
- [ ] Level 2: `cargo build -p kild-ui` succeeds
- [ ] Level 3: Full workspace builds

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Theme color mismatch with mockup | LOW | LOW | Cross-reference with mockup-dashboard.html CSS |
| Cursor display logic wrong | LOW | LOW | Test manually with focused=true/false, empty/non-empty |

---

## Notes

**Design Decision: Display-Only Component**

GPUI's input model is parent-driven. The parent view (MainView) owns the FocusHandle and receives KeyDownEvent. The parent then:
1. Updates its state (e.g., `create_form.branch_name.push(c)`)
2. Calls `cx.notify()` to trigger re-render
3. TextInput receives new value as prop and re-renders

This is different from web components where inputs have their own state. In GPUI, components are more like "controlled components" in React - they render what they're told.

**Future Enhancement**

If we need click-to-focus behavior, we can add an `on_click` handler that the parent can use to update focus state. But for now, Tab key navigation (handled by parent) is sufficient.

**Color Mapping from Mockup**

Per mockup-dashboard.html CSS:
- Input background: `--surface: #151719` → `theme::surface()`
- Focus border: `--ice: #38BDF8` → `theme::ice()`
- Normal border: `--border: #2D3139` → `theme::border()`
- Placeholder text: `--text-muted: #5C6370` → `theme::text_muted()`
- Value text: `--text-bright: #E8ECF0` → `theme::text_bright()`

---

## Sources

- [GPUI docs.rs - IntoElement](https://docs.rs/gpui/latest/gpui/trait.IntoElement.html)
- [GPUI docs.rs - RenderOnce](https://docs.rs/gpui/latest/gpui/trait.RenderOnce.html)
- Button component: `crates/kild-ui/src/components/button.rs`
