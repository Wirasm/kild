# Feature: GPUI Phase 2 - Empty Window

## Summary

Create a minimal GPUI window that displays "Shards" title text. This phase validates that GPUI works correctly on the system - window management, event loop, and basic rendering. The shards-ui binary will open a real window instead of exiting immediately.

## User Story

As a developer testing shards-ui
I want to see a GPUI window open with a title
So that I can verify the UI framework is working before building actual features

## Problem Statement

The shards-ui binary currently exits immediately with a scaffolding message. We need to prove GPUI can create and manage a window before building any real UI functionality.

## Solution Statement

Replace the placeholder main.rs with actual GPUI Application and Window initialization code. Create a simple MainView that renders "Shards" centered in the window. The window should be resizable and close cleanly.

## Metadata

| Field            | Value                                |
| ---------------- | ------------------------------------ |
| Type             | NEW_CAPABILITY                       |
| Complexity       | LOW                                  |
| Systems Affected | shards-ui                            |
| Dependencies     | gpui 0.2.2                           |
| Estimated Tasks  | 4                                    |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   $ cargo run -p shards-ui                                                    ║
║                                                                               ║
║   ┌─────────────────────────────────────┐                                     ║
║   │  Terminal Output:                   │                                     ║
║   │                                     │                                     ║
║   │  shards-ui: GPUI scaffolding ready. │                                     ║
║   │  See Phase 2 of gpui-native-...     │                                     ║
║   │  [Process exits with code 1]        │                                     ║
║   │                                     │                                     ║
║   └─────────────────────────────────────┘                                     ║
║                                                                               ║
║   NO WINDOW - Binary exits immediately                                        ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   $ cargo run -p shards-ui                                                    ║
║                                                                               ║
║   ┌──────────────────────────────────────────────────────────┐                ║
║   │ ● ○ ○                        Shards                      │ ◄── Title bar  ║
║   ├──────────────────────────────────────────────────────────┤                ║
║   │                                                          │                ║
║   │                                                          │                ║
║   │                                                          │                ║
║   │                        Shards                            │ ◄── Centered   ║
║   │                                                          │                ║
║   │                                                          │                ║
║   │                                                          │                ║
║   └──────────────────────────────────────────────────────────┘                ║
║                                                                               ║
║   WINDOW OPENS - Can resize, close exits app cleanly                          ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | User Impact |
|----------|--------|-------|-------------|
| `cargo run -p shards-ui` | Prints message, exits | Opens window | Actual GUI visible |
| Window close button | N/A | Closes window, exits app | Clean shutdown |
| Window resize | N/A | Resizable | Standard window behavior |

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File | Lines | Why Read This |
|----------|------|-------|---------------|
| P0 | `crates/shards-ui/src/main.rs` | 1-13 | Current placeholder to REPLACE |
| P0 | `crates/shards-ui/Cargo.toml` | 1-14 | Dependencies available |
| P1 | `Cargo.toml` (root) | 36-39 | GPUI workspace config (default-features = false) |

**External Documentation:**
| Source | Section | Why Needed |
|--------|---------|------------|
| [GPUI Official](https://www.gpui.rs/) | Getting Started | Window creation pattern |
| [GPUI docs.rs](https://docs.rs/gpui) | Application, Window, Render | API reference |

---

## Patterns to Mirror

**GPUI_APPLICATION_PATTERN:**
```rust
// SOURCE: https://www.gpui.rs/ (official example)
// COPY THIS PATTERN for Application setup:
fn main() {
    Application::new().run(|cx: &mut App| {
        // Window creation here
    });
}
```

**GPUI_WINDOW_PATTERN:**
```rust
// SOURCE: https://www.gpui.rs/ (official example)
// COPY THIS PATTERN for opening a window:
let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
cx.open_window(
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from("Shards")),
            ..Default::default()
        }),
        ..Default::default()
    },
    |_, cx| {
        cx.new(|_| MainView)
    },
).unwrap();
```

**GPUI_RENDER_PATTERN:**
```rust
// SOURCE: https://www.gpui.rs/ (official example)
// COPY THIS PATTERN for implementing Render trait:
impl Render for MainView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .justify_center()
            .items_center()
            .bg(rgb(0x1e1e1e))
            .text_3xl()
            .text_color(rgb(0xffffff))
            .child("Shards")
    }
}
```

---

## Files to Change

| File | Action | Justification |
| ---- | ------ | ------------- |
| `crates/shards-ui/src/main.rs` | REWRITE | Replace placeholder with actual GPUI application |
| `crates/shards-ui/src/ui/mod.rs` | CREATE | Module declarations for UI code |
| `crates/shards-ui/src/ui/app.rs` | CREATE | Application initialization (optional, can inline) |
| `crates/shards-ui/src/ui/views/mod.rs` | CREATE | Views module declarations |
| `crates/shards-ui/src/ui/views/main_view.rs` | CREATE | MainView struct implementing Render |

**Alternative (KISS approach):**
Put everything in main.rs for this phase. Split into modules in Phase 3 when complexity warrants it.

| File | Action | Justification |
| ---- | ------ | ------------- |
| `crates/shards-ui/src/main.rs` | REWRITE | Complete GPUI app in single file |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **No shard data loading** - That's Phase 3
- **No buttons or interactions** - That's Phase 4+
- **No state management** - Not needed yet
- **No module splitting** - KISS, everything in main.rs
- **No theming or styling beyond basics** - Dark bg + white text is enough
- **No CLI integration** - shards-ui is standalone binary

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable.

### Task 1: REWRITE `crates/shards-ui/src/main.rs`

- **ACTION**: Replace placeholder with minimal GPUI application
- **IMPLEMENT**: Application, Window, and MainView in single file
- **IMPORTS**: Use only from gpui crate
- **CODE**:
  ```rust
  //! shards-ui: GUI for Shards
  //!
  //! GPUI-based visual dashboard for shard management.

  use gpui::{
      div, prelude::*, px, rgb, size, App, Application, Bounds, Context,
      IntoElement, Render, SharedString, TitlebarOptions, Window, WindowBounds,
      WindowOptions,
  };

  /// Main view displaying the Shards title
  struct MainView;

  impl Render for MainView {
      fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
          div()
              .size_full()
              .flex()
              .justify_center()
              .items_center()
              .bg(rgb(0x1e1e1e))
              .text_3xl()
              .text_color(rgb(0xffffff))
              .child("Shards")
      }
  }

  fn main() {
      Application::new().run(|cx: &mut App| {
          let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
          cx.open_window(
              WindowOptions {
                  window_bounds: Some(WindowBounds::Windowed(bounds)),
                  titlebar: Some(TitlebarOptions {
                      title: Some(SharedString::from("Shards")),
                      ..Default::default()
                  }),
                  ..Default::default()
              },
              |_, cx| cx.new(|_| MainView),
          )
          .expect("Failed to open window");
      });
  }
  ```
- **GOTCHA**: GPUI 0.2 uses `cx.new()` not `cx.build_entity()` for creating views
- **GOTCHA**: `Bounds::centered()` requires `cx` as third argument
- **GOTCHA**: `TitlebarOptions` sets window title, not a title element
- **VALIDATE**: `cargo check -p shards-ui`

### Task 2: VERIFY window opens

- **ACTION**: Build and run the binary
- **IMPLEMENT**: No code changes, just verification
- **COMMAND**: `cargo run -p shards-ui`
- **EXPECTED**:
  - Window appears with dark background (#1e1e1e)
  - "Shards" text centered in white
  - Window title bar shows "Shards"
  - Window is resizable
- **VALIDATE**: Visual inspection

### Task 3: VERIFY clean shutdown

- **ACTION**: Test window close behavior
- **IMPLEMENT**: No code changes, just verification
- **STEPS**:
  1. Run `cargo run -p shards-ui`
  2. Click the red close button (×)
  3. Verify process exits (no zombie)
- **EXPECTED**: Process terminates with exit code 0
- **VALIDATE**: `echo $?` after close should show 0

### Task 4: RUN quality checks

- **ACTION**: Ensure all quality gates pass
- **IMPLEMENT**: No code changes, fix any issues found
- **COMMANDS**:
  ```bash
  cargo fmt --check
  cargo clippy --all -- -D warnings
  cargo test --all
  cargo build --all
  ```
- **EXPECTED**: All commands exit 0
- **VALIDATE**: All quality gates green

---

## Testing Strategy

### Manual Tests

| Test | Steps | Expected Result |
|------|-------|-----------------|
| Window opens | Run binary | Dark window with "Shards" text |
| Window resize | Drag corner | Window resizes, text stays centered |
| Window close | Click × | App exits cleanly |
| Title bar | Observe | Shows "Shards" in title |

### Edge Cases Checklist

- [ ] Window close via × button
- [ ] Window close via Cmd+Q
- [ ] Window close via Cmd+W
- [ ] Window minimize and restore
- [ ] Multiple rapid resize operations

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

**EXPECT**: Exit 0, no errors or warnings

### Level 2: TYPE_CHECK

```bash
cargo check -p shards-ui
```

**EXPECT**: Exit 0, all types resolve

### Level 3: BUILD

```bash
cargo build -p shards-ui
```

**EXPECT**: Binary created at `target/debug/shards-ui`

### Level 4: SMOKE_TEST

```bash
cargo run -p shards-ui &
PID=$!
sleep 2
if ps -p $PID > /dev/null; then
    echo "Window opened successfully"
    kill $PID
else
    echo "FAILED: Process died immediately"
    exit 1
fi
```

**EXPECT**: Process stays alive, window opens

### Level 5: FULL_SUITE

```bash
cargo test --all && cargo build --all
```

**EXPECT**: All tests pass, all crates build

---

## Acceptance Criteria

- [ ] `cargo run -p shards-ui` opens a window
- [ ] Window has dark background (#1e1e1e)
- [ ] "Shards" text displayed centered in white
- [ ] Window title bar shows "Shards"
- [ ] Window is resizable
- [ ] Closing window exits the application
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all -- -D warnings` passes
- [ ] `cargo test --all` passes
- [ ] `cargo build --all` succeeds

---

## Completion Checklist

- [ ] Task 1: main.rs rewritten with GPUI application
- [ ] Task 2: Window opens visually verified
- [ ] Task 3: Clean shutdown verified
- [ ] Task 4: All quality gates pass
- [ ] All acceptance criteria met

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
| ---- | ---------- | ------ | ---------- |
| GPUI API changes since research | LOW | MED | Docs checked Jan 2025, use docs.rs for exact signatures |
| Metal/GPU issues on system | LOW | HIGH | GPUI requirement - macOS with Metal support |
| Import paths wrong | MED | LOW | Follow docs.rs exactly, fix compile errors |
| Window doesn't appear | LOW | MED | Check console for errors, ensure macOS permissions |

---

## Notes

- This is the simplest possible GPUI window - intentionally minimal
- All code in main.rs for KISS - split into modules in Phase 3 when needed
- Dark theme (#1e1e1e background) chosen to match typical developer tools
- Window size 800x600 is reasonable default, can adjust later
- No shards-core integration yet - that's Phase 3
- The PRD mentioned feature flags, but Phase 1 implemented as separate crate - we follow actual implementation

## External References

- [GPUI Official Site](https://www.gpui.rs/) - Getting started guide
- [GPUI docs.rs](https://docs.rs/gpui) - API documentation
- [GPUI Component Library](https://github.com/longbridge/gpui-component) - For future phases

---

*Source PRD: `.claude/PRPs/prds/gpui-native-terminal-ui.prd.md`*
*Phase: 2 - Empty Window*
