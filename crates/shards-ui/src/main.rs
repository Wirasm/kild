//! shards-ui: GUI for Shards
//!
//! GPUI-based visual dashboard for shard management.

use gpui::{
    App, Application, Bounds, Context, IntoElement, Render, SharedString, TitlebarOptions, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
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
