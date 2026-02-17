---
name: gpui
description: GPUI framework patterns for building native UI. Use when implementing views, components, elements, actions, async tasks, entity management, focus handling, or any UI work in the kild-ui crate. Covers Render, Element, Entity, async, focus, actions, styling, and the gpui-component library.
---

## Overview

GPUI is a GPU-accelerated UI framework. This project uses `gpui = "0.2"` with `gpui-component` for higher-level components.

**Architecture:**
- `crates/kild-ui/` — GPUI-based native GUI
- `gpui-component` — Component library (Button, Input, Modal, Root, Theme)
- Theme: Tallinn Night (see `theme.rs`, `theme_bridge.rs`)

## Decision Tree: What Do I Need?

| Task | Use | Reference |
|------|-----|-----------|
| Standard view/component | `impl Render for T` | [rendering.md](references/rendering.md) |
| One-shot component (no state) | `impl RenderOnce for T` | [rendering.md](references/rendering.md) |
| Custom layout/paint control | `impl Element for T` | [elements.md](references/elements.md) |
| Component state management | `Entity<T>`, `WeakEntity<T>` | [entities.md](references/entities.md) |
| Async data fetching / timers | `cx.spawn()`, `cx.background_spawn()` | [async.md](references/async.md) |
| Keyboard shortcuts | `actions!` macro, `KeyBinding` | [actions.md](references/actions.md) |
| Focus management | `FocusHandle`, `Focusable` trait | [focus.md](references/focus.md) |
| Events between components | `EventEmitter<E>`, `cx.emit()`, `cx.subscribe()` | [events.md](references/events.md) |
| App-wide shared state | `impl Global for T` | [globals.md](references/globals.md) |
| Animation / continuous updates | `Animation`, `request_animation_frame` | [async.md](references/async.md) |
| Text rendering (low-level) | `shape_line()` → `ShapedLine::paint()` | [elements.md](references/elements.md) |
| Styling and layout | `div()` builder, flexbox, theme | [styling.md](references/styling.md) |
| Buttons, inputs, modals | `gpui-component` library | [component-lib.md](references/component-lib.md) |

## Quick Patterns

### Minimal View

```rust
use gpui::{Context, IntoElement, Render, Window, div, prelude::*, px};

pub struct MyView {
    count: usize,
}

impl Render for MyView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(format!("Count: {}", self.count))
    }
}
```

### Background Task with UI Update

```rust
impl MyView {
    fn fetch_data(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let data = some_async_work().await;
            let _ = this.update(cx, |view, cx| {
                view.data = Some(data);
                cx.notify();
            });
        }).detach();
    }
}
```

### Button with Click Handler

```rust
use gpui_component::button::{Button, ButtonVariants};

Button::new("create-btn")
    .primary()
    .label("Create")
    .on_click(cx.listener(|view, _, window, cx| {
        view.on_create(window, cx);
    }))
```

## Critical Rules

1. **Always call `cx.notify()`** after mutating state that affects rendering
2. **Use weak refs in closures**: `cx.entity().downgrade()` to prevent retain cycles
3. **Use inner `cx`** inside `entity.update(cx, |state, inner_cx| ...)` — never the outer one
4. **Store `Task<()>`** in struct fields to prevent cancellation (prefix with `_` if unused)
5. **Entity updates from background**: chain `cx.background_spawn().then(cx.spawn(...))` or use foreground `cx.spawn()`
6. **Hitboxes in prepaint**, mouse events in paint — never the other way around
7. **`cx.propagate()`** to let unhandled events bubble to parent
8. **`impl EventEmitter<E> for T`** required before `cx.emit()` compiles — marker trait, no methods
9. **`.id()` required** on elements using `.hover()`, scroll, or mouse events — without it, state resets each frame

## Reference Documentation

See the `references/` directory for complete guides on each topic:

- [rendering.md](references/rendering.md) — Render/RenderOnce traits, div builder
- [elements.md](references/elements.md) — Custom Element trait (3-phase rendering)
- [entities.md](references/entities.md) — Entity lifecycle, weak refs, observations
- [async.md](references/async.md) — Foreground/background tasks, timers
- [actions.md](references/actions.md) — Action definitions, keybindings
- [events.md](references/events.md) — Custom events, subscriptions, observers
- [focus.md](references/focus.md) — Focus handles, keyboard navigation
- [globals.md](references/globals.md) — Global state management
- [styling.md](references/styling.md) — Div builder API, flexbox, theme
- [component-lib.md](references/component-lib.md) — gpui-component (Button, Input, Root, Theme)
