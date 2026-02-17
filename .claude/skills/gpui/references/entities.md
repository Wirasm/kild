# Entity Management

## Overview

`Entity<T>` is a reference-counted handle to state of type `T`.

**Types:**
- `Entity<T>` — Strong reference (keeps entity alive)
- `WeakEntity<T>` — Weak reference (doesn't prevent cleanup, returns `Result`)

## Core Operations

```rust
// Create
let entity = cx.new(|cx| MyState { count: 0 });

// Read
let count = entity.read(cx).count;

// Read with closure
let (a, b) = entity.read_with(cx, |state, cx| (state.a, state.b));

// Update (mutable)
entity.update(cx, |state, cx| {
    state.count += 1;
    cx.notify(); // Trigger re-render
});

// Weak reference
let weak = entity.downgrade();
let _ = weak.update(cx, |state, cx| {
    state.count += 1;
    cx.notify();
}); // Returns Result — entity may be dropped
```

## Critical Rules

### Always use weak refs in closures

```rust
// GOOD: Weak reference prevents retain cycles
let weak = cx.entity().downgrade();
cx.spawn(async move |cx| {
    let _ = weak.update(cx, |state, cx| cx.notify());
}).detach();

// BAD: Strong reference may leak
let strong = cx.entity();
cx.spawn(async move |cx| {
    strong.update(cx, |state, cx| cx.notify()); // Retain cycle!
}).detach();
```

### Always use inner cx

```rust
// GOOD
entity.update(cx, |state, inner_cx| {
    inner_cx.notify();
});

// BAD — multiple borrow error
entity.update(cx, |state, inner_cx| {
    cx.notify(); // Wrong! Using outer cx
});
```

### Never nest updates

```rust
// GOOD: Sequential updates
entity1.update(cx, |state, cx| { /* ... */ });
entity2.update(cx, |state, cx| { /* ... */ });

// BAD: Will panic
entity1.update(cx, |_, cx| {
    entity2.update(cx, |_, cx| { /* ... */ }); // Panic!
});
```

## Observing Entities

```rust
// Observe any state change (fires when cx.notify() called)
cx.observe(&entity, |this, observed, cx| {
    let value = observed.read(cx).value;
    this.sync(value, cx);
}).detach();

// Subscribe to typed events
cx.subscribe(&entity, |this, emitter, event: &MyEvent, cx| {
    match event {
        MyEvent::Updated(data) => this.handle(data, cx),
    }
}).detach();
```

**Always `.detach()`** subscriptions to keep them alive.

## Batch Updates

```rust
// BAD: Multiple re-renders
self.field1 = a; cx.notify();
self.field2 = b; cx.notify();

// GOOD: Single re-render
self.field1 = a;
self.field2 = b;
cx.notify();
```

## Conditional Updates

```rust
fn set_value(&mut self, new: i32, cx: &mut Context<Self>) {
    if self.value != new {
        self.value = new;
        cx.notify(); // Only notify if changed
    }
}
```

## Lifecycle Cleanup

Run cleanup code when an entity is about to be dropped:

```rust
impl MyComponent {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.on_release(|this, _, _cx| {
            // Runs when entity is dropped
            // Clean up resources, cancel subscriptions, etc.
        });

        Self { /* ... */ }
    }
}
```

Observe when *another* entity is released:

```rust
cx.observe_release(&other_entity, |this, released, cx| {
    // `released` is &mut T of the released entity
    this.handle_child_dropped(cx);
}).detach();
```
