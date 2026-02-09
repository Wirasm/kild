//! kild-ui: GUI for KILD
//!
//! GPUI-based visual dashboard for kild management.

use gpui::{
    App, AppContext, Application, Bounds, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};
use gpui_component::Root;

mod actions;
mod components;
mod refresh;
mod state;
mod theme;
mod theme_bridge;
mod views;
mod watcher;

use views::MainView;

fn main() {
    // Initialize tracing for debug output
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    Application::new().run(|cx: &mut App| {
        // Initialize gpui-component (must be first)
        gpui_component::init(cx);

        // Apply Tallinn Night theme
        theme_bridge::apply_tallinn_night_theme(cx);

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
            |window, cx| {
                let view = cx.new(MainView::new);
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("Failed to open window");
    });
}
