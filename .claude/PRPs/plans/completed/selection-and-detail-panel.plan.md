# Feature: Selection & Detail Panel (Phase 9.8)

## Summary

Implement click-to-select functionality for kild rows and a right-side detail panel (320px) that displays comprehensive information about the selected kild. The detail panel replaces inline action buttons with a richer experience showing full note text, detailed session info, git status, and action buttons.

## User Story

As a Tōryō (developer managing multiple kilds)
I want to click a kild row to see its full details in a side panel
So that I can see the complete note, git status, timestamps, and actions without leaving the list view

## Problem Statement

Currently, kild rows show truncated information and action buttons appear on hover. Users cannot see the full note text or detailed session information. The mockup design specifies a 320px detail panel on the right side that provides a dedicated space for viewing and managing a selected kild.

## Solution Statement

1. Add selection state to `AppState` tracking which kild is selected (by session ID)
2. Add click handler to kild rows that sets selection
3. Update kild row styling to show selected state (ice border)
4. Create a detail panel component that renders on the right side
5. Update main view to use 2-column layout when a kild is selected

## Metadata

| Field            | Value                                                |
| ---------------- | ---------------------------------------------------- |
| Type             | ENHANCEMENT                                          |
| Complexity       | MEDIUM                                               |
| Systems Affected | kild-ui: state.rs, kild_list.rs, main_view.rs, views |
| Dependencies     | gpui (existing)                                      |
| Estimated Tasks  | 6                                                    |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╝
║                                                                               ║
║  ┌─────────────────────────────────────────────────────────────────────────┐  ║
║  │ Header: KILD [Project ▼]          [Open All] [Stop All] [Refresh] [+]   │  ║
║  ├─────────────────────────────────────────────────────────────────────────┤  ║
║  │                                                                          │  ║
║  │  ● +42 -12  feature-auth     claude  23m  JWT auth...  [Actions hover]  │  ║
║  │  ● +156 -34 feature-payment  claude  1h   Refund...    [Actions hover]  │  ║
║  │  ○          fix-login        kiro    4h   Issue #234   [Actions hover]  │  ║
║  │                                                                          │  ║
║  │                          (Full width list)                               │  ║
║  │                                                                          │  ║
║  └─────────────────────────────────────────────────────────────────────────┘  ║
║                                                                               ║
║   USER_FLOW: Hover row → see actions → click action button                    ║
║   PAIN_POINT: Note is truncated, no way to see full info, no visual focus    ║
║   DATA_FLOW: Session → KildDisplay → Row render → Action click → handler      ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╝
║                                                                               ║
║  ┌─────────────────────────────────────────────────────────────────────────┐  ║
║  │ Header: KILD [Project ▼]          [Open All] [Stop All] [Refresh] [+]   │  ║
║  ├────────────────────────────────────────┬────────────────────────────────┤  ║
║  │                                        │ feature-auth           (320px) │  ║
║  │  ▌● feature-auth  (ice border=select) │ ● Active                        │  ║
║  │  │  +42 -12 claude 23m                │                                 │  ║
║  │  │  JWT auth...                       │ NOTE                            │  ║
║  │                                        │ ┌────────────────────────────┐ │  ║
║  │  ● feature-payment                    │ │ JWT authentication for API.│ │  ║
║  │    +156 -34 claude 1h                 │ │ Implementing access tokens,│ │  ║
║  │    Refund...                          │ │ refresh tokens, and        │ │  ║
║  │                                        │ │ session management.        │ │  ║
║  │  ○ fix-login                          │ └────────────────────────────┘ │  ║
║  │    kiro 4h ago                        │                                 │  ║
║  │    Issue #234                         │ DETAILS                         │  ║
║  │                                        │ Agent      claude               │  ║
║  │                 (flex: 1)             │ Status     Running              │  ║
║  │                                        │ Duration   23m 45s              │  ║
║  │                                        │ Created    Today 14:32          │  ║
║  │                                        │ Branch     kild_8f2a3b          │  ║
║  │                                        │                                 │  ║
║  │                                        │ GIT STATUS                      │  ║
║  │                                        │ Changes    Uncommitted          │  ║
║  │                                        │ Files      +42 -12 (5 files)    │  ║
║  │                                        │                                 │  ║
║  │                                        │ PATH                            │  ║
║  │                                        │ ~/.kilds/kild/feature-auth      │  ║
║  │                                        ├────────────────────────────────┤  ║
║  │                                        │ [Copy Path] [Open Editor]       │  ║
║  │                                        │ [Focus Terminal] [⏹ Stop]      │  ║
║  │                                        │ [🗑 Destroy Kild]               │  ║
║  └────────────────────────────────────────┴────────────────────────────────┘  ║
║                                                                               ║
║   USER_FLOW: Click row → row selected (ice border) → detail panel appears     ║
║   VALUE_ADD: Full note visible, all details visible, actions in clear panel   ║
║   DATA_FLOW: Click → set selected_kild_id → re-render with 2-column layout    ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| Kild row | Hover shows actions | Click selects, shows ice left border | Clear visual feedback |
| Main view | Full-width list | 2-column: list + detail panel | Richer info display |
| Note display | Truncated in row | Full text in detail panel | Complete context |
| Actions | Hover buttons on row | Buttons in detail panel | Dedicated action area |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/kild-ui/src/state.rs` | 42-48, 240-277 | KildDisplay + AppState - add selection state here |
| P0 | `crates/kild-ui/src/views/kild_list.rs` | 125-378 | Row rendering - add click handler + selected styling |
| P0 | `crates/kild-ui/src/views/main_view.rs` | 780-930 | Main render - add 2-column layout |
| P1 | `crates/kild-ui/src/components/button.rs` | 110-176 | Button API for detail panel actions |
| P1 | `crates/kild-ui/src/components/status_indicator.rs` | 86-170 | StatusIndicator::badge() for header |
| P2 | `crates/kild-ui/src/views/create_dialog.rs` | 27-175 | Dialog structure pattern to mirror |
| P2 | `crates/kild-ui/src/theme.rs` | 1-211 | All color, spacing, typography constants |

**External Documentation:**
| Source | Section | Why Needed |
|--------|---------|------------|
| Mockup | `mockup-dashboard.html:469-565` | Detail panel CSS specs |
| Mockup | `mockup-dashboard.html:1002-1068` | Detail panel HTML structure |
| Mockup | `mockup-dashboard.html:93-98` | 3-column grid (200px sidebar, flex, 320px detail) |

---

## Patterns to Mirror

**NAMING_CONVENTION:**
```rust
// SOURCE: crates/kild-ui/src/views/kild_list.rs:39-42
// Function naming pattern for render functions
pub fn render_kild_list(state: &AppState, cx: &mut Context<MainView>) -> impl IntoElement
```

**CLICK_HANDLER_PATTERN:**
```rust
// SOURCE: crates/kild-ui/src/views/kild_list.rs:285-291
// Click handler with closure capturing
Button::new(("copy-btn", ix), "Copy")
    .variant(ButtonVariant::Ghost)
    .on_click(cx.listener(
        move |view, _, _, cx| {
            view.on_copy_path_click(&worktree_path_for_copy, cx);
        },
    ))
```

**ROW_CLICK_PATTERN:**
```rust
// SOURCE: crates/kild-ui/src/views/kild_list.rs:125-130
// Row element with ID - add .on_click() to this pattern
div()
    .id(ix)
    .w_full()
    .flex()
    .flex_col()
```

**CONDITIONAL_RENDERING:**
```rust
// SOURCE: crates/kild-ui/src/views/main_view.rs:916-918
// Conditional child rendering
.when(self.state.show_create_dialog, |this| {
    this.child(create_dialog::render_create_dialog(&self.state, cx))
})
```

**SECTION_STYLING_PATTERN:**
```rust
// SOURCE: crates/kild-ui/src/views/create_dialog.rs:47-60
// Section with label + content
div()
    .flex()
    .flex_col()
    .gap(px(theme::SPACE_1))
    .child(
        div()
            .text_size(px(theme::TEXT_SM))
            .text_color(theme::text_subtle())
            .child("Section Label"),
    )
    .child(/* content */)
```

**STATUS_BADGE_PATTERN:**
```rust
// SOURCE: crates/kild-ui/src/components/status_indicator.rs:150-170
// StatusIndicator badge mode for detail panel header
StatusIndicator::badge(status)
```

**STATE_UPDATE_PATTERN:**
```rust
// SOURCE: crates/kild-ui/src/views/main_view.rs:286-307
// Handler updates state and calls cx.notify()
pub fn on_open_click(&mut self, branch: &str, cx: &mut Context<Self>) {
    // ... do work ...
    cx.notify();
}
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/kild-ui/src/state.rs` | UPDATE | Add `selected_kild_id: Option<String>` field |
| `crates/kild-ui/src/views/kild_list.rs` | UPDATE | Add row click handler, selected styling |
| `crates/kild-ui/src/views/detail_panel.rs` | CREATE | New detail panel component |
| `crates/kild-ui/src/views/mod.rs` | UPDATE | Export detail_panel module |
| `crates/kild-ui/src/views/main_view.rs` | UPDATE | Add selection handlers, 2-column layout |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Sidebar layout (Phase 9.9)** - Detail panel only, not the left sidebar
- **Keyboard selection (Phase 10)** - Only mouse click selection this phase
- **Multi-select** - Single selection only
- **Collapsible panel** - Always visible when kild selected
- **Panel resize** - Fixed 320px width per mockup
- **Deselection on outside click** - Clicking another row changes selection

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: UPDATE `crates/kild-ui/src/state.rs` - Add selection state

- **ACTION**: Add field to track selected kild
- **IMPLEMENT**:
  ```rust
  // In AppState struct (around line 277):
  /// ID of the currently selected kild (for detail panel)
  pub selected_kild_id: Option<String>,
  ```
- **ALSO ADD**: Helper method to get selected KildDisplay
  ```rust
  impl AppState {
      /// Get the selected kild display, if any
      pub fn selected_kild(&self) -> Option<&KildDisplay> {
          self.selected_kild_id.as_ref().and_then(|id| {
              self.displays.iter().find(|d| d.session.id == *id)
          })
      }

      /// Clear selection (e.g., when kild is destroyed)
      pub fn clear_selection(&mut self) {
          self.selected_kild_id = None;
      }
  }
  ```
- **ALSO UPDATE**: `Default::default()` or `new()` to initialize field to `None`
- **MIRROR**: Other `Option<String>` fields like `confirm_target_branch`
- **GOTCHA**: Selection should persist across refreshes (matched by session.id)
- **VALIDATE**: `cargo build -p kild-ui`

### Task 2: UPDATE `crates/kild-ui/src/views/main_view.rs` - Add selection handlers

- **ACTION**: Add handlers for selection events
- **IMPLEMENT**: In `impl MainView`:
  ```rust
  /// Handle kild row click - select for detail panel
  pub fn on_kild_select(&mut self, session_id: &str, cx: &mut Context<Self>) {
      tracing::debug!(event = "ui.kild.selected", session_id = session_id);
      self.state.selected_kild_id = Some(session_id.to_string());
      cx.notify();
  }
  ```
- **MIRROR**: `on_open_click` handler pattern at line 286-307
- **GOTCHA**: Don't clear selection on refresh - keep it stable
- **VALIDATE**: `cargo build -p kild-ui`

### Task 3: UPDATE `crates/kild-ui/src/views/kild_list.rs` - Add row click and selection styling

- **ACTION**: Make rows clickable and show selected state
- **IMPLEMENT**:
  1. Pass `selected_kild_id` from state into the render function
  2. In the uniform_list callback, check if current row is selected:
     ```rust
     let is_selected = state.selected_kild_id.as_ref() == Some(&display.session.id);
     ```
  3. Add click handler to row container:
     ```rust
     let session_id_for_click = display.session.id.clone();
     div()
         .id(ix)
         // ... existing styles ...
         .cursor_pointer()
         .on_click(cx.listener(move |view, _, _, cx| {
             view.on_kild_select(&session_id_for_click, cx);
         }))
     ```
  4. Add selected styling (ice left border):
     ```rust
     .when(is_selected, |row| {
         row.border_l_2()
             .border_color(theme::ice())
             .bg(theme::surface())
     })
     ```
- **MIRROR**: Button click handler pattern at line 285-291
- **GOTCHA**: Clone session_id before moving into closure
- **GOTCHA**: Ice border on left side only (border_l_2), background surface
- **VALIDATE**: `cargo build -p kild-ui && cargo run -p kild-ui` - click rows, see selection

### Task 4: CREATE `crates/kild-ui/src/views/detail_panel.rs` - Detail panel component

- **ACTION**: Create the detail panel render function
- **IMPLEMENT**: Full detail panel matching mockup structure:
  ```rust
  use crate::components::{Button, ButtonVariant, StatusIndicator};
  use crate::state::{AppState, GitStatus, ProcessStatus};
  use crate::theme;
  use crate::views::main_view::MainView;
  use crate::components::status_indicator::Status;
  use gpui::{px, Context, IntoElement, ParentElement, Styled};

  /// Width of the detail panel in pixels (from mockup)
  pub const DETAIL_PANEL_WIDTH: f32 = 320.0;

  pub fn render_detail_panel(state: &AppState, cx: &mut Context<MainView>) -> impl IntoElement {
      let Some(kild) = state.selected_kild() else {
          // Should not happen if called correctly, but handle gracefully
          return div().into_any_element();
      };

      let session = &kild.session;
      let branch = session.branch.clone();
      let agent = session.agent.clone();
      let note = session.note.clone();
      let worktree_path = session.worktree_path.display().to_string();
      let created_at = session.created_at.clone();

      // Map process status to display status
      let status = match kild.status {
          ProcessStatus::Running => Status::Active,
          ProcessStatus::Stopped => Status::Stopped,
          ProcessStatus::Unknown => Status::Crashed,
      };
      let status_text = match kild.status {
          ProcessStatus::Running => "Running",
          ProcessStatus::Stopped => "Stopped",
          ProcessStatus::Unknown => "Unknown",
      };
      let status_color = match kild.status {
          ProcessStatus::Running => theme::aurora(),
          ProcessStatus::Stopped => theme::copper(),
          ProcessStatus::Unknown => theme::ember(),
      };

      // Git status info
      let git_status_text = match kild.git_status {
          GitStatus::Clean => "Clean",
          GitStatus::Dirty => "Uncommitted",
          GitStatus::Unknown => "Unknown",
      };
      let git_status_color = match kild.git_status {
          GitStatus::Clean => theme::aurora(),
          GitStatus::Dirty => theme::copper(),
          GitStatus::Unknown => theme::text_muted(),
      };
      let diff_stats_display = kild.diff_stats.as_ref().map(|s| {
          format!("+{} -{} ({} files)", s.insertions, s.deletions, s.files_changed)
      });

      // Variables for action handlers
      let branch_for_copy = worktree_path.clone();
      let branch_for_editor = worktree_path.clone();
      let branch_for_focus = branch.clone();
      let branch_for_action = branch.clone();
      let branch_for_destroy = branch.clone();
      let is_running = kild.status == ProcessStatus::Running;

      div()
          .w(px(DETAIL_PANEL_WIDTH))
          .h_full()
          .bg(theme::obsidian())
          .border_l_1()
          .border_color(theme::border_subtle())
          .flex()
          .flex_col()
          // Header
          .child(
              div()
                  .px(px(theme::SPACE_4))
                  .py(px(theme::SPACE_4))
                  .border_b_1()
                  .border_color(theme::border_subtle())
                  .child(
                      div()
                          .text_size(px(theme::TEXT_MD))
                          .font_weight(gpui::FontWeight::SEMIBOLD)
                          .text_color(theme::text_bright())
                          .child(branch.clone()),
                  )
                  .child(
                      div()
                          .mt(px(theme::SPACE_1))
                          .child(StatusIndicator::badge(status)),
                  ),
          )
          // Content (scrollable)
          .child(
              div()
                  .flex_1()
                  .overflow_y_scroll()
                  .px(px(theme::SPACE_4))
                  .py(px(theme::SPACE_4))
                  // Note section (if present)
                  .when_some(note, |this, note_text| {
                      this.child(render_section("Note",
                          div()
                              .px(px(theme::SPACE_3))
                              .py(px(theme::SPACE_3))
                              .bg(theme::surface())
                              .rounded(px(theme::RADIUS_MD))
                              .text_size(px(theme::TEXT_SM))
                              .text_color(theme::text())
                              .child(note_text)
                      ))
                  })
                  // Details section
                  .child(render_section("Details",
                      div()
                          .flex()
                          .flex_col()
                          .child(render_detail_row("Agent", &agent, theme::text()))
                          .child(render_detail_row_colored("Status", status_text, status_color))
                          // TODO: Duration calculation would require time tracking
                          .child(render_detail_row("Created", &created_at, theme::text()))
                          .child(render_detail_row("Branch", &session.id, theme::text()))
                  ))
                  // Git Status section
                  .child(render_section("Git Status",
                      div()
                          .flex()
                          .flex_col()
                          .child(render_detail_row_colored("Changes", git_status_text, git_status_color))
                          .when_some(diff_stats_display, |this, stats| {
                              this.child(render_detail_row("Files", &stats, theme::text()))
                          })
                  ))
                  // Path section
                  .child(render_section("Path",
                      div()
                          .px(px(theme::SPACE_2))
                          .py(px(theme::SPACE_2))
                          .bg(theme::surface())
                          .rounded(px(theme::RADIUS_MD))
                          .text_size(px(theme::TEXT_XS))
                          .text_color(theme::text_subtle())
                          .child(worktree_path.clone())
                  ))
          )
          // Actions footer
          .child(
              div()
                  .px(px(theme::SPACE_4))
                  .py(px(theme::SPACE_4))
                  .border_t_1()
                  .border_color(theme::border_subtle())
                  .flex()
                  .flex_col()
                  .gap(px(theme::SPACE_2))
                  // Row 1: Copy Path, Open Editor
                  .child(
                      div()
                          .flex()
                          .gap(px(theme::SPACE_2))
                          .child(
                              Button::new("detail-copy-path", "Copy Path")
                                  .variant(ButtonVariant::Secondary)
                                  .on_click(cx.listener(move |view, _, _, cx| {
                                      view.on_copy_path_click(&branch_for_copy, cx);
                                  }))
                          )
                          .child(
                              Button::new("detail-open-editor", "Open Editor")
                                  .variant(ButtonVariant::Secondary)
                                  .on_click(cx.listener(move |view, _, _, cx| {
                                      view.on_editor_click(&branch_for_editor, cx);
                                  }))
                          )
                  )
                  // Row 2: Focus Terminal, Open/Stop
                  .child(
                      div()
                          .flex()
                          .gap(px(theme::SPACE_2))
                          .child(
                              Button::new("detail-focus-terminal", "Focus")
                                  .variant(ButtonVariant::Secondary)
                                  .on_click(cx.listener(move |view, _, _, cx| {
                                      view.on_focus_click(&branch_for_focus, cx);
                                  }))
                          )
                          .when(is_running, |this| {
                              let br = branch_for_action.clone();
                              this.child(
                                  Button::new("detail-stop", "Stop")
                                      .variant(ButtonVariant::Warning)
                                      .on_click(cx.listener(move |view, _, _, cx| {
                                          view.on_stop_click(&br, cx);
                                      }))
                              )
                          })
                          .when(!is_running, |this| {
                              let br = branch_for_action.clone();
                              this.child(
                                  Button::new("detail-open", "Open")
                                      .variant(ButtonVariant::Success)
                                      .on_click(cx.listener(move |view, _, _, cx| {
                                          view.on_open_click(&br, cx);
                                      }))
                              )
                          })
                  )
                  // Row 3: Destroy
                  .child(
                      Button::new("detail-destroy", "Destroy Kild")
                          .variant(ButtonVariant::Danger)
                          .on_click(cx.listener(move |view, _, _, cx| {
                              view.on_destroy_click(&branch_for_destroy, cx);
                          }))
                  )
          )
  }

  fn render_section(title: &str, content: impl IntoElement) -> impl IntoElement {
      div()
          .mb(px(theme::SPACE_5))
          .child(
              div()
                  .text_size(px(theme::TEXT_XS))
                  .font_weight(gpui::FontWeight::SEMIBOLD)
                  .text_color(theme::text_muted())
                  .mb(px(theme::SPACE_2))
                  .child(title.to_uppercase()),
          )
          .child(content)
  }

  fn render_detail_row(label: &str, value: &str, value_color: gpui::Hsla) -> impl IntoElement {
      div()
          .flex()
          .justify_between()
          .py(px(theme::SPACE_2))
          .text_size(px(theme::TEXT_SM))
          .child(
              div()
                  .text_color(theme::text_subtle())
                  .child(label.to_string()),
          )
          .child(
              div()
                  .text_color(value_color)
                  .text_size(px(theme::TEXT_XS))
                  .child(value.to_string()),
          )
  }

  fn render_detail_row_colored(label: &str, value: &str, value_color: gpui::Hsla) -> impl IntoElement {
      render_detail_row(label, value, value_color)
  }
  ```
- **MIRROR**: create_dialog.rs structure at line 27-175
- **GOTCHA**: Use `.into_any_element()` for empty div return in None case
- **GOTCHA**: Clone strings before moving into button click handlers
- **GOTCHA**: Use `overflow_y_scroll()` for content area
- **VALIDATE**: `cargo build -p kild-ui`

### Task 5: UPDATE `crates/kild-ui/src/views/mod.rs` - Export detail_panel

- **ACTION**: Add module export
- **IMPLEMENT**:
  ```rust
  pub mod detail_panel;
  ```
- **MIRROR**: Other module exports in the file
- **VALIDATE**: `cargo build -p kild-ui`

### Task 6: UPDATE `crates/kild-ui/src/views/main_view.rs` - Add 2-column layout

- **ACTION**: Update render to show detail panel when kild selected
- **IMPLEMENT**: Modify the `Render` impl to use flex layout with optional detail panel:

  Replace the kild_list.render_kild_list call with a 2-column layout:
  ```rust
  // Instead of just:
  // .child(kild_list::render_kild_list(&self.state, cx))

  // Use conditional 2-column layout:
  .child(
      div()
          .flex_1()
          .flex()
          .overflow_hidden()
          // Kild list (flexible width)
          .child(
              div()
                  .flex_1()
                  .overflow_hidden()
                  .child(kild_list::render_kild_list(&self.state, cx))
          )
          // Detail panel (fixed 320px, conditional)
          .when(self.state.selected_kild_id.is_some(), |this| {
              this.child(detail_panel::render_detail_panel(&self.state, cx))
          })
  )
  ```
- **ALSO ADD**: Import at top of file:
  ```rust
  use super::detail_panel;
  ```
- **MIRROR**: Conditional rendering pattern at line 916-918
- **GOTCHA**: Use `.flex_1()` on list container to take remaining space
- **GOTCHA**: Use `.overflow_hidden()` on containers to prevent scrollbar issues
- **VALIDATE**: `cargo build -p kild-ui && cargo run -p kild-ui`

---

## Testing Strategy

### Manual Test Cases

| Test Case | Steps | Expected Result |
|-----------|-------|-----------------|
| Selection works | Click a kild row | Row shows ice left border, detail panel appears |
| Selection changes | Click different row | Previous row deselects, new row selected, panel updates |
| Panel shows data | Select kild with note | Full note visible in panel |
| Actions work | Click "Stop" in panel | Kild stops, status updates |
| Destroy clears selection | Destroy selected kild | Kild removed, panel disappears |
| No selection on start | Open app | No panel, full-width list |

### Edge Cases Checklist

- [ ] Empty state (no kilds) - no selection possible
- [ ] Single kild - can select and deselect (by destroying)
- [ ] Long note text - wraps properly in panel
- [ ] Long worktree path - wraps with word-break
- [ ] Refresh with selection - selection persists if kild still exists
- [ ] Destroy selected kild - panel disappears, selection cleared

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy -p kild-ui -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: BUILD

```bash
cargo build -p kild-ui
```

**EXPECT**: Clean build, no errors

### Level 3: FULL_SUITE

```bash
cargo test --all && cargo build --all
```

**EXPECT**: All tests pass, full workspace builds

### Level 4: VISUAL_VALIDATION

```bash
cargo run -p kild-ui
```

Manual steps:
1. Create a kild with a note: `cargo run -p kild -- create test-select --note "Testing selection feature with a longer note that should wrap"`
2. Open UI, click the kild row
3. Verify: Ice border on selected row, detail panel appears on right
4. Verify: Note displayed in full, all details shown
5. Verify: Actions work (Stop, Open, Copy Path, etc.)
6. Verify: Clicking another row changes selection

---

## Acceptance Criteria

- [ ] Clicking a kild row selects it (ice left border visual)
- [ ] Detail panel (320px) appears on right when kild selected
- [ ] Panel shows: branch name, status badge, note (full), agent, status, created, branch ID, git status, diff stats, path
- [ ] All action buttons in panel work (Copy Path, Open Editor, Focus, Stop/Open, Destroy)
- [ ] Destroying selected kild clears selection and hides panel
- [ ] Selection persists across refresh
- [ ] Level 1-3 validation passes

---

## Completion Checklist

- [ ] Task 1: State field added
- [ ] Task 2: Selection handlers added
- [ ] Task 3: Row click + selected styling
- [ ] Task 4: Detail panel component created
- [ ] Task 5: Module exported
- [ ] Task 6: 2-column layout in main view
- [ ] Level 1: cargo fmt + clippy passes
- [ ] Level 2: cargo build -p kild-ui succeeds
- [ ] Level 3: cargo test --all passes
- [ ] Level 4: Visual validation complete
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Selection not persisting on refresh | LOW | MED | Selection by session.id, verified in Task 1 |
| Click conflicts with row actions | LOW | MED | Row click on container, buttons stop propagation |
| Panel layout breaks on small windows | MED | LOW | Fixed width, overflow handled |
| Handler references stale data | MED | MED | Always clone before closures |

---

## Notes

- This phase does NOT include keyboard selection (j/k navigation) - that's Phase 10
- This phase does NOT include the left sidebar - that's Phase 9.9
- The layout matches the mockup: list takes flex space, panel is fixed 320px
- Copy Path action in detail panel should use worktree_path, not branch name
- Status badge uses StatusIndicator::badge() for larger display in panel header
