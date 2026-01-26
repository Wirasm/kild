# Feature: GUI Bulk Operations (Open All / Stop All)

## Summary

Add "Open All" and "Stop All" buttons to the shards-ui header that operate on all stopped/running shards respectively, enabling quick bulk lifecycle operations.

## User Story

As a power user with multiple shards, I want to open or stop all shards with one click so that I can quickly start/end my work session.

## Problem Statement

From PRD Phase 7.6: Power users managing multiple shards need bulk operations. Enables quick "end of day" cleanup and "start of day" launch. Currently, users must click individual Open/Stop buttons for each shard, which is tedious when managing 5+ shards.

## Solution Statement

Add two buttons to the header between the title and Create button:
- `[Open All (N)]` - enabled when N shards are stopped, launches agents in all stopped shards
- `[Stop All (N)]` - enabled when N shards are running, stops all running agents

Buttons show count and are disabled (grayed) when count is 0.

## Metadata

| Field | Value |
|-------|-------|
| Type | NEW_CAPABILITY |
| Complexity | SMALL |
| Systems Affected | shards-ui |
| Dependencies | CLI open/stop --all (already implemented) |
| Estimated Tasks | 3 |

---

## UX Design

### Before State
```
+-------------------------------------------------------------+
|  Shards                                      [Refresh] [+]  |
+-------------------------------------------------------------+
|  * feature-auth    claude    Running                        |
|  o feature-api     kiro      Stopped                        |
+-------------------------------------------------------------+
```

### After State
```
+-------------------------------------------------------------+
|  Shards             [Open All (1)] [Stop All (1)] [Refresh] [+]|
+-------------------------------------------------------------+
|  * feature-auth    claude    Running                        |
|  o feature-api     kiro      Stopped                        |
+-------------------------------------------------------------+
```

Button states:
- `[Open All (2)]` shows count of stopped shards, enabled
- `[Open All (0)]` grayed/disabled when no stopped shards
- `[Stop All (3)]` shows count of running shards, enabled
- `[Stop All (0)]` grayed/disabled when no running shards

---

## Mandatory Reading

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/shards-ui/src/views/main_view.rs` | 321-385 | Header rendering pattern with Refresh and Create buttons |
| P0 | `crates/shards-ui/src/actions.rs` | 107-142 | Existing open_shard() and stop_shard() action patterns |
| P0 | `crates/shards-ui/src/state.rs` | 8-17, 191-212 | ProcessStatus enum and AppState structure |
| P1 | `crates/shards/src/commands.rs` | 336-411, 445-516 | CLI handle_open_all() and handle_stop_all() for logic reference |

---

## Patterns to Mirror

**HEADER_BUTTON_RENDERING:**
```rust
// SOURCE: crates/shards-ui/src/views/main_view.rs:341-358
// Pattern for header buttons (Refresh button):
.child(
    div()
        .id("refresh-btn")
        .px_3()
        .py_1()
        .bg(rgb(0x444444))
        .hover(|style| style.bg(rgb(0x555555)))
        .rounded_md()
        .cursor_pointer()
        .on_mouse_up(
            gpui::MouseButton::Left,
            cx.listener(|view, _, _, cx| {
                view.on_refresh_click(cx);
            }),
        )
        .child(div().text_color(rgb(0xffffff)).child("Refresh")),
)
```

**ACTION_HANDLER_PATTERN:**
```rust
// SOURCE: crates/shards-ui/src/actions.rs:107-124
// Pattern for calling session operations:
pub fn open_shard(branch: &str, agent: Option<String>) -> Result<Session, String> {
    tracing::info!(event = "ui.open_shard.started", branch = branch, agent = ?agent);

    match session_ops::open_session(branch, agent) {
        Ok(session) => {
            tracing::info!(
                event = "ui.open_shard.completed",
                branch = branch,
                process_id = session.process_id
            );
            Ok(session)
        }
        Err(e) => {
            tracing::error!(event = "ui.open_shard.failed", branch = branch, error = %e);
            Err(e.to_string())
        }
    }
}
```

**STATE_UPDATE_PATTERN:**
```rust
// SOURCE: crates/shards-ui/src/views/main_view.rs:168-189
// Pattern for updating state after action:
pub fn on_open_click(&mut self, branch: &str, cx: &mut Context<Self>) {
    tracing::info!(event = "ui.open_clicked", branch = branch);
    self.state.clear_open_error();

    match actions::open_shard(branch, None) {
        Ok(_session) => {
            self.state.refresh_sessions();
        }
        Err(e) => {
            // ... error handling ...
        }
    }
    cx.notify();
}
```

---

## Files to Change

| File | Action | Justification |
|------|--------|---------------|
| `crates/shards-ui/src/state.rs` | UPDATE | Add stopped_count() and running_count() helper methods |
| `crates/shards-ui/src/actions.rs` | UPDATE | Add open_all_stopped() and stop_all_running() functions |
| `crates/shards-ui/src/views/main_view.rs` | UPDATE | Add bulk operation buttons to header and click handlers |

---

## Step-by-Step Tasks

### Task 1: ADD count helpers to state.rs

- **ACTION**: Add methods to AppState to count stopped and running shards
- **FILE**: `crates/shards-ui/src/state.rs`
- **LOCATION**: After line 291 (end of impl AppState block)
- **IMPLEMENT**:
  ```rust
  /// Count shards with Stopped status.
  pub fn stopped_count(&self) -> usize {
      self.displays.iter().filter(|d| d.status == ProcessStatus::Stopped).count()
  }

  /// Count shards with Running status.
  pub fn running_count(&self) -> usize {
      self.displays.iter().filter(|d| d.status == ProcessStatus::Running).count()
  }
  ```
- **VALIDATE**: `cargo check -p shards-ui`

### Task 2: ADD bulk action functions to actions.rs

- **ACTION**: Add functions to open all stopped shards and stop all running shards
- **FILE**: `crates/shards-ui/src/actions.rs`
- **LOCATION**: After stop_shard() function (after line 142)
- **IMPORT NEEDED**: Add `use crate::state::ShardDisplay;` at top of file
- **IMPLEMENT**:
  ```rust
  /// Open agents in all stopped shards.
  ///
  /// Returns (opened_count, errors) where errors contains branch names and error messages.
  pub fn open_all_stopped(displays: &[ShardDisplay]) -> (usize, Vec<(String, String)>) {
      tracing::info!(event = "ui.open_all_stopped.started");

      let stopped: Vec<_> = displays.iter()
          .filter(|d| d.status == crate::state::ProcessStatus::Stopped)
          .collect();

      let mut opened = 0;
      let mut errors = Vec::new();

      for display in stopped {
          match session_ops::open_session(&display.session.branch, None) {
              Ok(session) => {
                  tracing::info!(
                      event = "ui.open_all_stopped.shard_opened",
                      branch = session.branch,
                      process_id = session.process_id
                  );
                  opened += 1;
              }
              Err(e) => {
                  tracing::error!(
                      event = "ui.open_all_stopped.shard_failed",
                      branch = display.session.branch,
                      error = %e
                  );
                  errors.push((display.session.branch.clone(), e.to_string()));
              }
          }
      }

      tracing::info!(
          event = "ui.open_all_stopped.completed",
          opened = opened,
          failed = errors.len()
      );

      (opened, errors)
  }

  /// Stop all running shards.
  ///
  /// Returns (stopped_count, errors) where errors contains branch names and error messages.
  pub fn stop_all_running(displays: &[ShardDisplay]) -> (usize, Vec<(String, String)>) {
      tracing::info!(event = "ui.stop_all_running.started");

      let running: Vec<_> = displays.iter()
          .filter(|d| d.status == crate::state::ProcessStatus::Running)
          .collect();

      let mut stopped = 0;
      let mut errors = Vec::new();

      for display in running {
          match session_ops::stop_session(&display.session.branch) {
              Ok(()) => {
                  tracing::info!(
                      event = "ui.stop_all_running.shard_stopped",
                      branch = display.session.branch
                  );
                  stopped += 1;
              }
              Err(e) => {
                  tracing::error!(
                      event = "ui.stop_all_running.shard_failed",
                      branch = display.session.branch,
                      error = %e
                  );
                  errors.push((display.session.branch.clone(), e.to_string()));
              }
          }
      }

      tracing::info!(
          event = "ui.stop_all_running.completed",
          stopped = stopped,
          failed = errors.len()
      );

      (stopped, errors)
  }
  ```
- **VALIDATE**: `cargo check -p shards-ui`

### Task 3: ADD buttons and handlers to main_view.rs

- **ACTION**: Add Open All / Stop All buttons to header, add click handlers
- **FILE**: `crates/shards-ui/src/views/main_view.rs`

**PART A - Add handler methods** (after on_stop_click, around line 213):
```rust
/// Handle click on the Open All button.
fn on_open_all_click(&mut self, cx: &mut Context<Self>) {
    tracing::info!(event = "ui.open_all_clicked");

    let (opened, errors) = actions::open_all_stopped(&self.state.displays);

    for (branch, err) in &errors {
        tracing::warn!(
            event = "ui.open_all.partial_failure",
            branch = branch,
            error = err
        );
    }

    self.state.refresh_sessions();
    cx.notify();
}

/// Handle click on the Stop All button.
fn on_stop_all_click(&mut self, cx: &mut Context<Self>) {
    tracing::info!(event = "ui.stop_all_clicked");

    let (stopped, errors) = actions::stop_all_running(&self.state.displays);

    for (branch, err) in &errors {
        tracing::warn!(
            event = "ui.stop_all.partial_failure",
            branch = branch,
            error = err
        );
    }

    self.state.refresh_sessions();
    cx.notify();
}
```

**PART B - Add buttons to header** (in render(), around line 337, before Refresh button):
```rust
// Open All button
.child({
    let stopped_count = self.state.stopped_count();
    let is_disabled = stopped_count == 0;
    let bg_color = if is_disabled { rgb(0x333333) } else { rgb(0x446644) };
    let hover_color = if is_disabled { rgb(0x333333) } else { rgb(0x557755) };
    let text_color = if is_disabled { rgb(0x666666) } else { rgb(0xffffff) };

    div()
        .id("open-all-btn")
        .px_3()
        .py_1()
        .bg(bg_color)
        .when(!is_disabled, |d| d.hover(|style| style.bg(hover_color)))
        .rounded_md()
        .when(!is_disabled, |d| d.cursor_pointer())
        .when(!is_disabled, |d| {
            d.on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|view, _, _, cx| {
                    view.on_open_all_click(cx);
                }),
            )
        })
        .child(
            div()
                .text_color(text_color)
                .child(format!("Open All ({})", stopped_count)),
        )
})
// Stop All button
.child({
    let running_count = self.state.running_count();
    let is_disabled = running_count == 0;
    let bg_color = if is_disabled { rgb(0x333333) } else { rgb(0x664444) };
    let hover_color = if is_disabled { rgb(0x333333) } else { rgb(0x775555) };
    let text_color = if is_disabled { rgb(0x666666) } else { rgb(0xffffff) };

    div()
        .id("stop-all-btn")
        .px_3()
        .py_1()
        .bg(bg_color)
        .when(!is_disabled, |d| d.hover(|style| style.bg(hover_color)))
        .rounded_md()
        .when(!is_disabled, |d| d.cursor_pointer())
        .when(!is_disabled, |d| {
            d.on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|view, _, _, cx| {
                    view.on_stop_all_click(cx);
                }),
            )
        })
        .child(
            div()
                .text_color(text_color)
                .child(format!("Stop All ({})", running_count)),
        )
})
```
- **VALIDATE**: `cargo build -p shards-ui`

---

## Validation Commands

### Level 1: STATIC_ANALYSIS
```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

### Level 2: BUILD
```bash
cargo build -p shards-ui
```

### Level 3: MANUAL_TEST
```bash
# Setup: Create 3 shards, stop 2
cargo run -p shards -- create test1 --agent claude
cargo run -p shards -- create test2 --agent kiro
cargo run -p shards -- create test3 --agent claude
cargo run -p shards -- stop test1
cargo run -p shards -- stop test2

# Run UI
cargo run -p shards-ui

# Verify:
# 1. Header shows [Open All (2)] [Stop All (1)] [Refresh] [+ Create]
# 2. "Open All (2)" button is green-ish and clickable
# 3. "Stop All (1)" button is red-ish and clickable
# 4. Click "Open All" -> both stopped shards launch terminals
# 5. Counts update: [Open All (0)] [Stop All (3)]
# 6. Open All button now grayed/disabled
# 7. Click "Stop All" -> all 3 shards stop
# 8. Counts update: [Open All (3)] [Stop All (0)]
# 9. Stop All button now grayed/disabled

# Cleanup
cargo run -p shards -- destroy test1 --force
cargo run -p shards -- destroy test2 --force
cargo run -p shards -- destroy test3 --force
```

---

## Acceptance Criteria

- [ ] Open All button shows count of stopped shards in label
- [ ] Stop All button shows count of running shards in label
- [ ] Buttons are visually disabled (grayed) when count is 0
- [ ] Disabled buttons do not respond to clicks
- [ ] Clicking Open All opens agents in all stopped shards
- [ ] Clicking Stop All stops all running shards
- [ ] State refreshes after bulk operation completes
- [ ] Counts update correctly after operations
- [ ] All validation commands pass (fmt, clippy, build)

---

## Completion Checklist

- [ ] Count helpers added to state.rs (stopped_count, running_count)
- [ ] Bulk action functions added to actions.rs (open_all_stopped, stop_all_running)
- [ ] Handler methods added to main_view.rs (on_open_all_click, on_stop_all_click)
- [ ] Buttons added to header in render()
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all -- -D warnings` passes
- [ ] `cargo build -p shards-ui` succeeds
- [ ] Manual testing validates all acceptance criteria
