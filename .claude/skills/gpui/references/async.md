# Async Operations and Background Tasks

## Foreground Tasks (UI Thread)

Run on the UI thread. Can update entities directly.

```rust
impl MyView {
    fn fetch_data(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let data = fetch_from_api().await;

            let _ = this.update(cx, |view, cx| {
                view.data = Some(data);
                cx.notify();
            });
        }).detach();
    }
}
```

**`this`** is a `WeakEntity<Self>` — automatically created by `cx.spawn()`.

## Background Tasks (Worker Threads)

For CPU-intensive work. Cannot update entities directly.

```rust
impl MyView {
    fn process(&mut self, cx: &mut Context<Self>) {
        cx.background_spawn(async move {
            heavy_computation() // Runs on background thread
        })
        .then(cx.spawn(move |result, cx| {
            // Back to foreground to update UI
            this.update(cx, |view, cx| {
                view.result = result;
                cx.notify();
            }).ok();
        }))
        .detach();
    }
}
```

## Periodic Tasks (Timers)

```rust
let task = cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
    loop {
        cx.background_executor().timer(Duration::from_secs(5)).await;

        if let Err(_) = this.update(cx, |view, cx| {
            view.refresh();
            cx.notify();
        }) {
            break; // View was dropped
        }
    }
});
// Store to prevent cancellation
self._refresh_task = task;
```

## Task Lifecycle

```rust
struct MyView {
    _task: Task<()>,  // Stored: runs until view is dropped
}

// Task is cancelled when dropped
// .detach() runs independently (fire-and-forget)
// Store in struct to keep alive
```

## Animation / Continuous Updates

### Declarative Animation

```rust
use gpui::Animation;

div()
    .with_animation(
        "fade-in",
        Animation::new(Duration::from_millis(300))
            .with_easing(ease_in_out),
        |el, progress| {
            el.opacity(progress)
        },
    )
```

Available: `.repeat()` for looping, `.with_easing()` with `ease_in_out`, `ease_out_quint`, `linear`, `bounce`, `pulsating_between`.

### Manual Animation (request_animation_frame)

For custom frame-by-frame updates (cursor blink, live terminal):

```rust
// Request callback on next frame
window.request_animation_frame(cx.listener(|this, _event, window, cx| {
    this.advance_animation();
    cx.notify();
    // Request again for continuous animation
    window.request_animation_frame(cx.listener(Self::on_frame));
}));
```

### Task Cancellation

Replace an in-flight task by overwriting the stored `Task`:

```rust
struct MyView {
    search_task: Option<Task<()>>,
}

impl MyView {
    fn on_input_changed(&mut self, query: String, cx: &mut Context<Self>) {
        // Overwrites previous task, which drops and cancels it
        self.search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(200)).await;
            let results = search(query).await;
            let _ = this.update(cx, |view, cx| {
                view.results = results;
                cx.notify();
            });
        }));
    }
}
```

## Key Rules

1. **Store tasks** in struct fields to prevent cancellation (prefix `_` if not accessed)
2. **`cx.spawn()`** gives you `this: WeakEntity<Self>` automatically
3. **`cx.background_spawn()`** has no entity access — chain with `.then(cx.spawn(...))` for UI updates
4. **Break loops** when `this.update()` returns `Err` (entity was dropped)
5. **Use `cx.background_executor().timer()`** for delays, not `tokio::time::sleep`
6. **Overwrite stored `Task`** to cancel a previous in-flight task (drop = cancel)
