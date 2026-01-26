//! shards-ui: GUI for Shards
//!
//! GPUI-based visual dashboard for shard management.

use gpui::{
    App, AppContext, Application, Bounds, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};

mod actions;
mod projects;
mod refresh;
mod state;
mod views;

use views::MainView;

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
            |_, cx| cx.new(MainView::new),
        )
        .expect("Failed to open window");
    });
}
