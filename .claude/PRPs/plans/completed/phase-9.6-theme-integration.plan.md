# Feature: Phase 9.6 - Theme Integration

## Summary

Apply the KILD brand system (Tallinn Night theme) and integrate reusable UI components (Button, StatusIndicator, Modal, TextInput) across all existing views. This transforms working-but-rough UI into a cohesive, maintainable design system that visually matches the mockup-dashboard.html specification.

## User Story

As a **Tōryō (power user)**
I want the KILD GUI to have **consistent, polished visual styling** aligned with the brand
So that I can **focus on managing kilds without visual inconsistency distracting me**

## Problem Statement

The current UI has ~150+ hardcoded `rgb()` color values scattered across 6 view files. The styled components (Button, StatusIndicator, Modal, TextInput) exist but are not yet used in views. This creates:
- Visual inconsistency (same concept = different colors)
- Maintenance burden (changing a color requires editing multiple files)
- Deviation from the brand system mockup

## Solution Statement

Systematically replace all hardcoded colors with `theme::` function calls and migrate manual button/input/dialog renders to use the extracted components. The result is a pixel-consistent UI matching mockup-dashboard.html.

## Metadata

| Field            | Value                                      |
| ---------------- | ------------------------------------------ |
| Type             | REFACTOR                                   |
| Complexity       | MEDIUM                                     |
| Systems Affected | kild-ui (views, components)                |
| Dependencies     | gpui 0.2.2 (already present)               |
| Estimated Tasks  | 8                                          |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   Hardcoded colors scattered across views:                                    ║
║   ┌─────────────────────────────────────────────────────────────────────┐    ║
║   │ main_view.rs      → rgb(0x1e1e1e), rgb(0xffffff), rgb(0x4a9eff)...  │    ║
║   │ kild_list.rs      → rgb(0x00ff00), rgb(0xff0000), rgb(0x888888)...  │    ║
║   │ create_dialog.rs  → rgb(0x2d2d2d), rgb(0x444444), rgb(0x666666)...  │    ║
║   │ confirm_dialog.rs → rgb(0xcc4444), rgb(0xff6b6b), rgb(0x3d1e1e)...  │    ║
║   └─────────────────────────────────────────────────────────────────────┘    ║
║                                                                               ║
║   Manual button rendering:     Manual input rendering:                        ║
║   .bg(rgb(0x4a9eff))          .bg(rgb(0x1e1e1e))                              ║
║   .hover(|s| s.bg(...))       .border_color(rgb(0x555555))                    ║
║   .child("Label")             .child(if empty { placeholder } else { val })  ║
║                                                                               ║
║   PAIN: Colors don't match mockup. 150+ places to edit to change palette.    ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   Theme-based colors via function calls:                                      ║
║   ┌─────────────────────────────────────────────────────────────────────┐    ║
║   │ All views → use crate::theme;                                        │    ║
║   │            .bg(theme::void())                                        │    ║
║   │            .text_color(theme::text_bright())                         │    ║
║   │            .border_color(theme::ice())                               │    ║
║   └─────────────────────────────────────────────────────────────────────┘    ║
║                                                                               ║
║   Component-based UI:          Reusable components:                           ║
║   Button::new("id", "Create")  TextInput::new("id")                           ║
║       .variant(Primary)            .value(&branch)                            ║
║       .on_click(handler)           .placeholder("...")                        ║
║                                    .focused(is_focused)                       ║
║                                                                               ║
║   StatusIndicator::dot(Active)  Modal::new("id", "Title")                     ║
║   StatusIndicator::badge(Stopped)    .body(content)                           ║
║                                      .footer(buttons)                         ║
║                                                                               ║
║   VALUE: Single source of truth for colors. Components enforce consistency.  ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| All dialogs | Manual div render | Modal component | Consistent overlay, header/body/footer |
| All buttons | Manual hover/click | Button variants | Consistent colors per action type |
| All inputs | Manual styling | TextInput component | Consistent focus states |
| Status dots | `rgb(0x00ff00)` etc | StatusIndicator | Correct status colors with glows |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-ui/src/theme.rs` | all | ALL color constants - use these ONLY |
| P0 | `crates/kild-ui/src/components/button.rs` | 15-90 | ButtonVariant colors and API |
| P0 | `crates/kild-ui/src/components/status_indicator.rs` | 14-57, 90-167 | Status enum and render pattern |
| P0 | `crates/kild-ui/src/components/modal.rs` | 74-167 | Modal API and structure |
| P0 | `crates/kild-ui/src/components/text_input.rs` | 40-112 | TextInput API and render |
| P1 | `.claude/PRPs/branding/mockup-dashboard.html` | all | Target visual design |
| P1 | `.claude/PRPs/branding/brand-system.html` | all | Complete color spec |

**External Documentation:**

| Source | Section | Why Needed |
|--------|---------|------------|
| [GPUI docs.rs](https://docs.rs/gpui) | Rgba, rgb, rgba | Color type APIs |
| [gpui.rs](https://www.gpui.rs/) | Styled trait | div() builder methods |

---

## Patterns to Mirror

**THEME COLOR USAGE:**
```rust
// SOURCE: crates/kild-ui/src/theme.rs (entire file)
// COPY THIS PATTERN for all color usages:
use crate::theme;

div()
    .bg(theme::surface())           // NOT: rgb(0x151719)
    .text_color(theme::text_bright()) // NOT: rgb(0xffffff)
    .border_color(theme::ice())     // NOT: rgb(0x4a9eff)
```

**BUTTON COMPONENT USAGE:**
```rust
// SOURCE: crates/kild-ui/src/components/button.rs:94-99
// COPY THIS PATTERN:
use crate::components::{Button, ButtonVariant};

Button::new("cancel-btn", "Cancel")
    .variant(ButtonVariant::Secondary)
    .on_click(cx.listener(|view, _, _, cx| {
        view.on_cancel(cx);
    }))
```

**STATUS INDICATOR USAGE:**
```rust
// SOURCE: crates/kild-ui/src/components/status_indicator.rs:76-82
// COPY THIS PATTERN:
use crate::components::{Status, StatusIndicator};

// For list rows - 8px dot
StatusIndicator::dot(match display.status {
    ProcessStatus::Running => Status::Active,
    ProcessStatus::Stopped => Status::Stopped,
    ProcessStatus::Unknown => Status::Crashed, // or omit
})

// For detail panel - badge with label
StatusIndicator::badge(Status::Active)
```

**TEXT INPUT USAGE:**
```rust
// SOURCE: crates/kild-ui/src/components/text_input.rs:25-31
// COPY THIS PATTERN:
use crate::components::TextInput;

TextInput::new("branch-input")
    .value(&state.branch_name)
    .placeholder("Type branch name...")
    .focused(state.focused_field == Field::BranchName)
```

**MODAL USAGE:**
```rust
// SOURCE: crates/kild-ui/src/components/modal.rs:34-47
// COPY THIS PATTERN:
use crate::components::{Modal, Button, ButtonVariant};

Modal::new("create-dialog", "Create New KILD")
    .body(
        div().flex().flex_col().gap(px(theme::SPACE_4))
            .child(/* form fields using TextInput */)
    )
    .footer(
        div().flex().justify_end().gap(px(theme::SPACE_2))
            .child(Button::new("cancel", "Cancel").variant(ButtonVariant::Secondary)...)
            .child(Button::new("create", "Create").variant(ButtonVariant::Primary)...)
    )
```

---

## Files to Change

| File | Action | Justification |
| ---- | ------ | ------------- |
| `crates/kild-ui/src/views/create_dialog.rs` | UPDATE | Replace manual dialog with Modal, inputs with TextInput, buttons with Button |
| `crates/kild-ui/src/views/confirm_dialog.rs` | UPDATE | Replace manual dialog with Modal, buttons with Button |
| `crates/kild-ui/src/views/add_project_dialog.rs` | UPDATE | Replace manual dialog with Modal, inputs with TextInput, buttons with Button |
| `crates/kild-ui/src/views/kild_list.rs` | UPDATE | Replace status colors with StatusIndicator, action buttons with Button |
| `crates/kild-ui/src/views/main_view.rs` | UPDATE | Replace hardcoded colors, bulk buttons with Button component |
| `crates/kild-ui/src/views/project_selector.rs` | UPDATE | Replace hardcoded colors with theme functions |
| `crates/kild-ui/src/components/mod.rs` | UPDATE | Remove `#[allow(unused_imports)]` once components are used |
| `crates/kild-ui/src/components/status_indicator.rs` | UPDATE | Remove `#![allow(dead_code)]` once used |
| `crates/kild-ui/src/components/modal.rs` | UPDATE | Remove `#![allow(dead_code)]` once used |
| `crates/kild-ui/src/components/text_input.rs` | UPDATE | Remove `#![allow(dead_code)]` once used |
| `crates/kild-ui/src/theme.rs` | UPDATE | Remove `#![allow(dead_code)]` once used |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Light theme** - Dark theme only for MVP. No theme switching UI.
- **Animation/transitions** - No new animations (keep existing pulse for crashed status)
- **New components** - Only integrate existing Button, StatusIndicator, Modal, TextInput
- **Keyboard shortcuts** - That's Phase 10
- **Layout changes** - Only visual styling, not structural changes
- **New features** - Pure refactor, no new functionality

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `create_dialog.rs` - Migrate to Modal + TextInput + Button

**ACTION**: Replace entire render function to use components

**IMPLEMENT**:
1. Add imports: `use crate::components::{Button, ButtonVariant, Modal, TextInput};`
2. Add import: `use crate::theme;`
3. Remove `rgb` from imports
4. Replace manual overlay + dialog box with `Modal::new("create-dialog", "Create New KILD")`
5. Replace branch name input div with `TextInput::new("branch-input").value(&branch_name).placeholder("Type branch name...").focused(focused_field == BranchName)`
6. Keep agent selector as custom (click to cycle) but use theme colors
7. Replace note input div with `TextInput::new("note-input").value(&note).placeholder("What is this kild for?").focused(focused_field == Note)`
8. Replace error box colors: `rgb(0x3d1e1e)` → `theme::with_alpha(theme::ember(), 0.2)`, `rgb(0x662222)` → `theme::ember()`, `rgb(0xff6b6b)` → `theme::ember()`
9. Replace Cancel button with `Button::new("cancel-btn", "Cancel").variant(ButtonVariant::Secondary).on_click(...)`
10. Replace Create button with `Button::new("create-btn", "Create").variant(ButtonVariant::Primary).on_click(...)`

**COLOR MAPPINGS for this file:**
| Old | New |
|-----|-----|
| `rgb(0x000000aa)` | `theme::overlay()` |
| `rgb(0x2d2d2d)` | `theme::elevated()` |
| `rgb(0x444444)` | `theme::border()` |
| `rgb(0xffffff)` | `theme::text_white()` or `theme::text_bright()` |
| `rgb(0xaaaaaa)` | `theme::text_subtle()` |
| `rgb(0x1e1e1e)` | `theme::surface()` |
| `rgb(0x4a9eff)` | `theme::ice()` |
| `rgb(0x555555)` | `theme::border()` |
| `rgb(0x666666)` | `theme::text_muted()` |
| `rgb(0x888888)` | `theme::text_subtle()` |
| `rgb(0x3d1e1e)` | `theme::with_alpha(theme::ember(), 0.2)` |
| `rgb(0x662222)` | `theme::ember()` (dimmed via context) |
| `rgb(0xff6b6b)` | `theme::ember()` |
| `rgb(0x5aafff)` | `theme::ice_bright()` |
| `rgb(0x2a2a2a)` | `theme::elevated()` |

**CHALLENGE**: Agent selector is custom (click cycles through options). Keep it but use theme colors.

**VALIDATE**: `cargo build -p kild-ui && cargo clippy -p kild-ui -- -D warnings`

---

### Task 2: UPDATE `confirm_dialog.rs` - Migrate to Modal + Button

**ACTION**: Replace manual dialog with Modal, buttons with Button

**IMPLEMENT**:
1. Add imports: `use crate::components::{Button, ButtonVariant, Modal};`
2. Add import: `use crate::theme;`
3. Remove `rgb` from imports
4. Replace manual overlay + dialog with `Modal::new("confirm-dialog", &title)`
5. Body: warning message with themed colors
6. Error box: use theme colors for ember shading
7. Replace Cancel button with `Button::new("cancel-btn", "Cancel").variant(ButtonVariant::Secondary)`
8. Replace Destroy button with `Button::new("destroy-btn", "Destroy").variant(ButtonVariant::Danger)`

**COLOR MAPPINGS:**
| Old | New |
|-----|-----|
| `rgb(0x2d2d2d)` | `theme::elevated()` |
| `rgb(0x444444)` | `theme::border()` |
| `rgb(0xffffff)` | `theme::text_white()` |
| `rgb(0xaaaaaa)` | `theme::text_subtle()` |
| `rgb(0xff6b6b)` | `theme::ember()` |
| `rgb(0x3d1e1e)` | `theme::with_alpha(theme::ember(), 0.2)` |
| `rgb(0x662222)` | `theme::ember()` (border context) |
| `rgb(0x555555)` | `theme::border()` or `theme::blade()` |
| `rgb(0xcc4444)` | `theme::ember()` |
| `rgb(0xdd5555)` | `theme::ember()` (hover handled by Button) |

**VALIDATE**: `cargo build -p kild-ui && cargo clippy -p kild-ui -- -D warnings`

---

### Task 3: UPDATE `add_project_dialog.rs` - Migrate to Modal + TextInput + Button

**ACTION**: Replace manual dialog with Modal, inputs with TextInput, buttons with Button

**IMPLEMENT**:
1. Add imports: `use crate::components::{Button, ButtonVariant, Modal, TextInput};`
2. Add import: `use crate::theme;`
3. Remove `rgb` from imports
4. Replace manual overlay + dialog with `Modal::new("add-project-dialog", "Add Project")`
5. Replace path input with `TextInput::new("path-input").value(&path).placeholder("/path/to/project").focused(focused_field == Path)`
6. Replace name input with `TextInput::new("name-input").value(&name).placeholder("Optional display name").focused(focused_field == Name)`
7. Replace error box with theme colors
8. Replace Cancel button with `Button::new("cancel", "Cancel").variant(ButtonVariant::Secondary)`
9. Replace Add button with `Button::new("add", "Add Project").variant(ButtonVariant::Primary)`

**COLOR MAPPINGS:** (Same as create_dialog.rs)

**VALIDATE**: `cargo build -p kild-ui && cargo clippy -p kild-ui -- -D warnings`

---

### Task 4: UPDATE `kild_list.rs` - StatusIndicator + Button + Theme Colors

**ACTION**: Replace status dots with StatusIndicator, action buttons with Button, all colors with theme

**IMPLEMENT**:
1. Add imports: `use crate::components::{Button, ButtonVariant, Status, StatusIndicator};`
2. Add import: `use crate::theme;`
3. Remove `rgb` from imports
4. Map ProcessStatus to Status for StatusIndicator:
   ```rust
   let status = match display.status {
       ProcessStatus::Running => Status::Active,
       ProcessStatus::Stopped => Status::Stopped,
       ProcessStatus::Unknown => Status::Crashed, // or Stopped for "unknown"
   };
   ```
5. Replace `.child(div().text_color(status_color).child("●"))` with `.child(StatusIndicator::dot(status))`
6. Git dirty indicator: Use `theme::copper()` for dirty, `theme::text_muted()` for unknown
7. Replace all action buttons (Copy, Edit, Focus, Open, Stop, Destroy) with Button component:
   - Copy/Edit: `Button::new(...).variant(ButtonVariant::Ghost)`
   - Focus: `Button::new(...).variant(ButtonVariant::Secondary)` or Ghost
   - Open: `Button::new(...).variant(ButtonVariant::Success)` when stopped
   - Stop: `Button::new(...).variant(ButtonVariant::Warning)` when running
   - Destroy: `Button::new(...).variant(ButtonVariant::Danger)`
8. Replace text colors:
   - Branch name: `theme::text_white()` or `theme::text_bright()`
   - Agent: `theme::text_subtle()` or `theme::kiri()` (agent purple)
   - Project ID: `theme::text_muted()`
   - Timestamps: `theme::text_muted()`
   - Notes: `theme::text_subtle()`

**COLOR MAPPINGS:**
| Old | New |
|-----|-----|
| `rgb(0x00ff00)` | StatusIndicator with Status::Active |
| `rgb(0xff0000)` | StatusIndicator with Status::Stopped |
| `rgb(0x888888)` | `theme::text_subtle()` |
| `rgb(0xffa500)` | `theme::copper()` |
| `rgb(0x666666)` | `theme::text_muted()` |
| `rgb(0xffffff)` | `theme::text_white()` |
| `rgb(0x444444)` | `theme::blade()` |
| `rgb(0x555555)` | `theme::blade_bright()` |
| `rgb(0x444488)` | `theme::ice_dim()` |
| `rgb(0x555599)` | `theme::ice()` |
| `rgb(0x662222)` | `theme::with_alpha(theme::ember(), 0.2)` |
| `rgb(0x883333)` | `theme::with_alpha(theme::ember(), 0.3)` |
| `rgb(0xaaaaaa)` | `theme::text_subtle()` |
| `rgb(0xff6b6b)` | `theme::ember()` |
| `rgb(0x4a9eff)` | `theme::ice()` |
| `rgb(0x5aafff)` | `theme::ice_bright()` |

**NOTE**: The list rows in uniform_list have closures that capture variables. Ensure component usage works within these closures.

**VALIDATE**: `cargo build -p kild-ui && cargo clippy -p kild-ui -- -D warnings`

---

### Task 5: UPDATE `main_view.rs` - Button + Theme Colors

**ACTION**: Replace bulk operation buttons with Button component, all colors with theme

**IMPLEMENT**:
1. Add imports: `use crate::components::{Button, ButtonVariant};`
2. Add/verify import: `use crate::theme;`
3. Remove remaining `rgb` usage
4. Header background: `theme::obsidian()` or `theme::void()`
5. "KILD" title: `theme::text_white()`
6. Replace Open All button with `Button::new("open-all", format!("Open All ({})", count)).variant(ButtonVariant::Success).disabled(stopped_count == 0).on_click(...)`
7. Replace Stop All button with `Button::new("stop-all", format!("Stop All ({})", count)).variant(ButtonVariant::Warning).disabled(running_count == 0).on_click(...)`
8. Replace Refresh button with `Button::new("refresh", "↻").variant(ButtonVariant::Ghost).on_click(...)`
9. Replace Create button with `Button::new("create", "+ Create Kild").variant(ButtonVariant::Primary).on_click(...)`
10. Error banner: `theme::with_alpha(theme::ember(), 0.15)` background
11. Error text: `theme::ember()`
12. Dismiss button: `Button::new("dismiss", "Dismiss").variant(ButtonVariant::Ghost)`

**COLOR MAPPINGS:**
| Old | New |
|-----|-----|
| `rgb(0x1e1e1e)` | `theme::void()` |
| `rgb(0xffffff)` | `theme::text_white()` |
| `rgb(0x333333)` | `theme::surface()` (disabled) |
| `rgb(0x666666)` | `theme::text_muted()` (disabled text) |
| `rgb(0x446644)` | Use ButtonVariant::Success |
| `rgb(0x557755)` | Use ButtonVariant::Success hover |
| `rgb(0x664444)` | Use ButtonVariant::Warning |
| `rgb(0x775555)` | Use ButtonVariant::Warning hover |
| `rgb(0x444444)` | `theme::blade()` |
| `rgb(0x555555)` | `theme::blade_bright()` |
| `rgb(0x4a9eff)` | Use ButtonVariant::Primary |
| `rgb(0x5aafff)` | Use ButtonVariant::Primary hover |
| `rgb(0x662222)` | `theme::with_alpha(theme::ember(), 0.15)` |
| `rgb(0xff6b6b)` | `theme::ember()` |
| `rgb(0xaaaaaa)` | `theme::text_subtle()` |
| `rgb(0xffaaaa)` | `theme::with_alpha(theme::ember(), 0.2)` |

**VALIDATE**: `cargo build -p kild-ui && cargo clippy -p kild-ui -- -D warnings`

---

### Task 6: UPDATE `project_selector.rs` - Theme Colors

**ACTION**: Replace all hardcoded colors with theme functions

**IMPLEMENT**:
1. Add import: `use crate::theme;`
2. Remove `rgb` from imports
3. Add Project button: Use theme colors (blade for secondary action)
4. Trigger button: theme colors
5. Active project name: `theme::text_white()`
6. Dropdown arrow: `theme::text_subtle()`
7. Dropdown menu: `theme::elevated()` background, `theme::border()` border
8. Hover states: `theme::surface()` or `theme::elevated()`
9. Radio button selected: `theme::ice()`
10. Radio button unselected: `theme::border()`
11. Project names: `theme::text_white()`
12. Dividers: `theme::border_subtle()`
13. Add icon: `theme::ice()`
14. Remove icon/text: `theme::ember()`

**COLOR MAPPINGS:**
| Old | New |
|-----|-----|
| `rgb(0x444444)` | `theme::blade()` |
| `rgb(0x555555)` | `theme::blade_bright()` |
| `rgb(0xffffff)` | `theme::text_white()` |
| `rgb(0x888888)` | `theme::text_subtle()` |
| `rgb(0x2d2d2d)` | `theme::elevated()` |
| `rgb(0x3d3d3d)` | `theme::surface()` (hover) |
| `rgb(0x4a9eff)` | `theme::ice()` |
| `rgb(0xff6b6b)` | `theme::ember()` |

**VALIDATE**: `cargo build -p kild-ui && cargo clippy -p kild-ui -- -D warnings`

---

### Task 7: CLEANUP - Remove dead_code/unused_imports allows

**ACTION**: Remove suppression attributes now that everything is used

**IMPLEMENT**:
1. `crates/kild-ui/src/theme.rs`: Remove `#![allow(dead_code)]` on line 19
2. `crates/kild-ui/src/components/mod.rs`: Remove `#[allow(unused_imports)]` on lines 12, 15, 19
3. `crates/kild-ui/src/components/status_indicator.rs`: Remove `#![allow(dead_code)]` on line 8
4. `crates/kild-ui/src/components/modal.rs`: Remove `#![allow(dead_code)]` on line 8
5. `crates/kild-ui/src/components/text_input.rs`: Remove `#![allow(dead_code)]` on line 9

**VALIDATE**: `cargo build -p kild-ui && cargo clippy -p kild-ui -- -D warnings`

---

### Task 8: FINAL VALIDATION - Visual Match Check

**ACTION**: Run the UI and compare against mockup

**IMPLEMENT**:
1. Run `cargo run -p kild-ui`
2. Create a test kild to see list rendering
3. Open create dialog (c key or button)
4. Check project selector dropdown
5. Compare visually against `.claude/PRPs/branding/mockup-dashboard.html`

**CHECKLIST**:
- [ ] Header: Logo white, stats with correct status dots, buttons with correct variants
- [ ] Sidebar: Project list with ice selected border
- [ ] Kild List: StatusIndicator dots with glow for active, no glow for stopped
- [ ] Row hover/selected states
- [ ] Action buttons: correct variants (Ghost, Success, Warning, Danger)
- [ ] Create Dialog: Modal structure, themed inputs, Primary/Secondary buttons
- [ ] Error states: Ember colored with alpha backgrounds
- [ ] No hardcoded `rgb()` calls remaining in view files

**VALIDATE**: `cargo build -p kild-ui && cargo clippy -p kild-ui -- -D warnings && cargo test -p kild-ui`

---

## Testing Strategy

### Unit Tests to Write

No new unit tests required - this is a visual refactor. Existing tests should continue passing.

### Edge Cases Checklist

- [ ] Empty kild list renders correctly
- [ ] Error states render with ember colors
- [ ] Disabled buttons show correct styling (muted colors)
- [ ] All status states render (Active, Stopped, Crashed)
- [ ] Focus states on inputs show ice border
- [ ] Hover states on buttons show correct variant colors

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check -p kild-ui && cargo clippy -p kild-ui -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: BUILD

```bash
cargo build -p kild-ui
```

**EXPECT**: Clean build, no warnings

### Level 3: FULL_SUITE

```bash
cargo test --all && cargo build --all && cargo clippy --all -- -D warnings
```

**EXPECT**: All tests pass, all crates build, no clippy warnings

### Level 4: VISUAL_VALIDATION

```bash
cargo run -p kild-ui
```

**EXPECT**: UI launches, colors match mockup-dashboard.html

---

## Acceptance Criteria

- [ ] All 6 view files use `theme::` functions instead of `rgb()`
- [ ] All dialogs use Modal component
- [ ] All form inputs use TextInput component
- [ ] All action buttons use Button component with appropriate variants
- [ ] All status indicators use StatusIndicator component
- [ ] No `#[allow(dead_code)]` or `#[allow(unused_imports)]` in components
- [ ] Level 1-3 validation commands pass
- [ ] Visual match with mockup-dashboard.html

---

## Completion Checklist

- [ ] Task 1: create_dialog.rs migrated
- [ ] Task 2: confirm_dialog.rs migrated
- [ ] Task 3: add_project_dialog.rs migrated
- [ ] Task 4: kild_list.rs migrated
- [ ] Task 5: main_view.rs migrated
- [ ] Task 6: project_selector.rs migrated
- [ ] Task 7: Cleanup attributes removed
- [ ] Task 8: Visual validation passed
- [ ] All validation commands pass

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
| ---- | ---------- | ------ | ---------- |
| Button component doesn't fit all use cases | LOW | MED | Keep custom render for edge cases (agent selector) |
| TextInput display-only limitation | LOW | LOW | It's designed for display - keyboard handled by parent |
| StatusIndicator sizing mismatch | LOW | LOW | Use dot() for list, badge() for detail |
| GPUI render closure complexity | MED | MED | Test within uniform_list closures early |

---

## Notes

**Button variants mapping to UI actions:**
| Action | Variant | Example |
|--------|---------|---------|
| Primary CTA | Primary (ice) | Create, Add |
| Secondary/Cancel | Secondary (surface) | Cancel |
| Ghost/Subtle | Ghost (transparent) | Refresh, Copy, Edit |
| Success | Success (aurora) | Open All, Open |
| Warning | Warning (copper) | Stop All, Stop |
| Danger | Danger (ember border) | Destroy |

**ProcessStatus to Status mapping:**
| ProcessStatus | Status | Color |
|---------------|--------|-------|
| Running | Active | aurora (green) with glow |
| Stopped | Stopped | copper (amber) no glow |
| Unknown | Crashed or Stopped | ember (red) with glow or copper |

**Key theme functions:**
- Backgrounds: `void()`, `obsidian()`, `surface()`, `elevated()`
- Text: `text_muted()`, `text_subtle()`, `text()`, `text_bright()`, `text_white()`
- Borders: `border_subtle()`, `border()`, `border_strong()`
- Accents: `ice()`, `aurora()`, `copper()`, `ember()`, `kiri()`
- Utility: `with_alpha(color, alpha)`, `overlay()`
