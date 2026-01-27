//! kild-ui: GUI for KILD
//!
//! GPUI-based visual dashboard for kild management.

use gpui::{
    App, AppContext, Application, Bounds, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};

mod actions;
mod components;
mod projects;
mod refresh;
mod state;
mod theme;
mod views;

pub use components::{Button, ButtonVariant};

use views::MainView;

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("KILD")),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(MainView::new),
        )
        .expect("Failed to open window");
    });
}
