//! Split pane container with recursive rendering.
//!
//! Provides a two-pane layout (horizontal or vertical) with a resize handle.
//! Panes can contain terminal views or be empty with a placeholder message.

use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::terminal::TerminalView;
use crate::theme;
use crate::views::main_view::MainView;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// Content of a pane -- either a terminal or an empty placeholder.
pub enum PaneContent {
    Terminal(gpui::Entity<TerminalView>),
    Empty,
}

/// Split pane state for rendering.
pub struct SplitPane {
    pub direction: SplitDirection,
    pub first: PaneContent,
    pub second: PaneContent,
    /// Split ratio (0.0 to 1.0, default 0.5).
    pub ratio: f32,
}

/// Render the content of a single pane.
pub fn render_pane_content(content: &PaneContent, _cx: &mut Context<MainView>) -> impl IntoElement {
    match content {
        PaneContent::Terminal(entity) => div().size_full().child(entity.clone()),
        PaneContent::Empty => div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_color(theme::text_muted())
                    .text_size(px(theme::TEXT_BASE))
                    .child("Select a kild from the sidebar"),
            ),
    }
}

/// Render a split pane with two children and a resize handle between them.
pub fn render_split(split: &SplitPane, cx: &mut Context<MainView>) -> impl IntoElement {
    match split.direction {
        SplitDirection::Vertical => div()
            .size_full()
            .flex()
            .child(
                div()
                    .flex_basis(gpui::relative(split.ratio))
                    .overflow_hidden()
                    .child(render_pane_content(&split.first, cx)),
            )
            .child(render_resize_handle(split.direction))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(render_pane_content(&split.second, cx)),
            ),
        SplitDirection::Horizontal => div()
            .size_full()
            .flex_col()
            .child(
                div()
                    .flex_basis(gpui::relative(split.ratio))
                    .overflow_hidden()
                    .child(render_pane_content(&split.first, cx)),
            )
            .child(render_resize_handle(split.direction))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(render_pane_content(&split.second, cx)),
            ),
    }
}

fn render_resize_handle(direction: SplitDirection) -> impl IntoElement {
    match direction {
        SplitDirection::Vertical => div()
            .w(px(4.0))
            .h_full()
            .bg(theme::border_subtle())
            .hover(|style| style.bg(theme::ice_dim()))
            .cursor_pointer(),
        SplitDirection::Horizontal => div()
            .w_full()
            .h(px(4.0))
            .bg(theme::border_subtle())
            .hover(|style| style.bg(theme::ice_dim()))
            .cursor_pointer(),
    }
}
