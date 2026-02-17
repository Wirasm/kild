# Styling and Layout

## Div Builder

GPUI uses a fluent builder API on `div()`. Layout is flexbox-based.

```rust
use gpui::{div, px, prelude::*};

div()
    // Layout
    .flex()
    .flex_col()              // Column direction (default: row)
    .gap(px(8.))             // Gap between children
    .items_center()          // Align items center (cross axis)
    .justify_between()       // Justify content space-between (main axis)

    // Sizing
    .w(px(200.))             // Fixed width
    .h(px(100.))             // Fixed height
    .w_full()                // width: 100%
    .h_full()                // height: 100%
    .size_full()             // Both 100%
    .flex_1()                // flex: 1 (fill remaining space)
    .flex_shrink_0()         // Don't shrink
    .min_w(px(100.))         // Minimum width
    .max_h(px(400.))         // Maximum height
    .overflow_hidden()       // Clip overflow
    .overflow_y_scroll()     // Vertical scrolling

    // Spacing
    .p(px(16.))              // Padding all sides
    .px(px(12.))             // Padding horizontal
    .py(px(8.))              // Padding vertical
    .m(px(4.))               // Margin all sides

    // Visual
    .bg(color)               // Background color
    .text_color(color)       // Text color
    .text_size(px(14.))      // Font size
    .font_weight(FontWeight::SEMIBOLD)
    .rounded(px(6.))         // Border radius
    .border_1()              // 1px border
    .border_color(color)     // Border color
    .border_b_1()            // Bottom border only
    .border_r_1()            // Right border only
    .cursor_pointer()        // Pointer cursor
    .text_ellipsis()         // Truncate text with ...

    // Interactions
    .id("my-element")        // Required for hover/click
    .hover(|style| style.bg(hover_color))
    .on_mouse_up(gpui::MouseButton::Left, cx.listener(|view, _, _, cx| {
        view.on_click(cx);
    }))
    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|view, _, _, cx| {
        view.on_mouse_down(cx);
    }))

    // Children
    .child("Text content")
    .child(other_element)
    .children(items.iter().map(|i| div().child(i.name.clone())))
```

## SharedString

GPUI uses `SharedString` (reference-counted) instead of `String` for text passed to elements. Conversions:

```rust
use gpui::SharedString;

SharedString::from("literal")              // &str → SharedString
SharedString::from(format!("item-{}", i))  // String → SharedString
"literal".into()                           // Into<SharedString>
```

Use `SharedString` for element IDs, labels, and text content passed to GPUI APIs.

## ElementId

Required on elements that need **interactive state** (hover, scroll position, focus):

```rust
// String ID
div().id("my-button")

// Tuple ID (for items in a list — disambiguates repeated elements)
div().id(("list-item", index))

// SharedString ID
div().id(SharedString::from(format!("tab-{}", name)))
```

**When is `.id()` required?**
- `.hover()` — needs ID to track hover state
- `.on_mouse_up()` / `.on_mouse_down()` — needs ID for interaction
- `.overflow_y_scroll()` — needs ID to track scroll position
- Elements in lists/iterators — needs unique ID per item

Without `.id()`, hover/scroll state won't persist between renders.

## Conditional Rendering

```rust
div()
    .when(condition, |el| el.child("Shown"))
    .when_some(optional, |el, val| el.child(format!("{}", val)))
```

## Common Layout Patterns

### Horizontal Row

```rust
div()
    .flex()
    .items_center()
    .gap(px(8.))
    .child(icon)
    .child(label)
```

### Vertical Stack

```rust
div()
    .flex()
    .flex_col()
    .gap(px(4.))
    .child(title)
    .child(subtitle)
```

### Fixed Sidebar + Flexible Content

```rust
div()
    .flex()
    .size_full()
    .child(
        div().w(px(200.)).h_full().child(sidebar)
    )
    .child(
        div().flex_1().h_full().child(content)
    )
```

### Scrollable List

```rust
div()
    .id("scroll-container")
    .flex_1()
    .overflow_y_scroll()
    .children(items.iter().enumerate().map(|(ix, item)| {
        div()
            .id(("list-item", ix))
            .px(px(12.))
            .py(px(8.))
            .hover(|s| s.bg(theme::elevated()))
            .child(item.name.clone())
    }))
```

## Theme Colors (Tallinn Night)

Use functions from `crate::theme`:

```rust
use crate::theme;

theme::obsidian()           // Dark background
theme::surface()            // Slightly lighter
theme::elevated()           // Card/hover background
theme::ice()                // Primary accent (blue)
theme::aurora()             // Success (green)
theme::ember()              // Danger/error (red)
theme::amber()              // Warning (yellow)
theme::text_bright()        // Primary text
theme::text_subtle()        // Secondary text
theme::text_muted()         // Tertiary text
theme::border()             // Standard border
theme::border_subtle()      // Subtle border
theme::terminal_background() // Terminal bg
theme::with_alpha(color, 0.2) // Color with alpha

// Typography constants
theme::TEXT_XS              // 11.0
theme::TEXT_SM              // 12.0
theme::TEXT_BASE            // 13.0
theme::FONT_MONO            // "JetBrains Mono"

// Spacing constants
theme::SPACE_1 .. SPACE_5
theme::RADIUS_MD
```
