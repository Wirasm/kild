# Events and Subscriptions

## Custom Events

### Define, Declare, and Emit

To emit events, you must:
1. Define the event type
2. Implement `EventEmitter<E>` for your component (marker trait, no methods)
3. Call `cx.emit()` and `cx.notify()`

```rust
use gpui::EventEmitter;

#[derive(Clone)]
enum MyEvent {
    DataUpdated(String),
    ActionTriggered,
}

// Required: declare that MyComponent can emit MyEvent
impl EventEmitter<MyEvent> for MyComponent {}

// You can emit multiple event types from the same component:
// impl EventEmitter<OtherEvent> for MyComponent {}

impl MyComponent {
    fn update_data(&mut self, data: String, cx: &mut Context<Self>) {
        self.data = data.clone();
        cx.emit(MyEvent::DataUpdated(data));
        cx.notify();
    }
}
```

### Subscribe

```rust
impl Listener {
    fn new(source: Entity<MyComponent>, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            cx.subscribe(&source, |this, _emitter, event: &MyEvent, cx| {
                match event {
                    MyEvent::DataUpdated(data) => this.handle_update(data, cx),
                    MyEvent::ActionTriggered => this.handle_action(cx),
                }
            }).detach(); // Must detach to keep alive

            Self { source }
        })
    }
}
```

## Observing State Changes

Fires whenever the observed entity calls `cx.notify()`:

```rust
cx.observe(&entity, |this, observed, cx| {
    let value = observed.read(cx).value;
    this.sync(value, cx);
}).detach();
```

## Key Differences

| Mechanism | Fires when | Use for |
|-----------|-----------|---------|
| `cx.subscribe()` | `cx.emit(event)` called | Typed event handling |
| `cx.observe()` | `cx.notify()` called | React to any state change |

## Rules

1. **Implement `EventEmitter<E>`** on any type before calling `cx.emit()` — it won't compile without it
2. **Always `.detach()`** subscriptions/observations to keep them alive
3. **Avoid observation cycles** — A observes B, B observes A = infinite loop
4. Events are typed — subscribe to specific event enum variants
5. Subscriptions are automatically cleaned up when either entity is dropped
