//! Project rail component.
//!
//! Fixed left rail (48px) for project navigation with icon-based selection.

use gpui::{Context, FontWeight, IntoElement, ParentElement, Styled, div, prelude::*, px};

use crate::state::AppState;
use crate::theme;
use crate::views::main_view::MainView;

/// Render the project rail (48px vertical strip on the far left).
pub fn render_project_rail(state: &AppState, cx: &mut Context<MainView>) -> impl IntoElement {
    let active_path = state.active_project_path();
    let is_all_selected = active_path.is_none();
    let total_count = state.total_kild_count();

    let projects: Vec<_> = state
        .projects_iter()
        .enumerate()
        .map(|(idx, project)| {
            let path = project.path().to_path_buf();
            let is_selected = active_path == Some(project.path());
            let count = state.kild_count_for_project(project.path());
            let first_char = project
                .name()
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| "?".to_string());
            (idx, path, first_char, is_selected, count)
        })
        .collect();

    div()
        .w(px(theme::RAIL_WIDTH))
        .h_full()
        .bg(theme::void())
        .border_r_1()
        .border_color(theme::border_subtle())
        .flex()
        .flex_col()
        // Top: project icons
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .py(px(theme::SPACE_2))
                .gap(px(theme::SPACE_2))
                // "All Projects" icon
                .child(render_rail_icon(
                    "rail-all",
                    "\u{2217}".to_string(), // ∗
                    is_all_selected,
                    total_count,
                    cx.listener(|view, _, _, cx| {
                        view.on_project_select_all(cx);
                    }),
                ))
                // Per-project icons
                .children(projects.into_iter().map(
                    |(idx, path, first_char, is_selected, count)| {
                        render_rail_icon(
                            ("rail-project", idx),
                            first_char,
                            is_selected,
                            count,
                            cx.listener(move |view, _, _, cx| {
                                view.on_project_select(path.clone(), cx);
                            }),
                        )
                    },
                )),
        )
        // Bottom: add project button
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .py(px(theme::SPACE_2))
                .gap(px(theme::SPACE_2))
                .child(
                    div()
                        .id("rail-add-project")
                        .size(px(32.0))
                        .rounded(px(theme::RADIUS_MD))
                        .bg(theme::border())
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::surface()))
                        .text_size(px(theme::TEXT_LG))
                        .text_color(theme::text_muted())
                        .on_mouse_up(
                            gpui::MouseButton::Left,
                            cx.listener(|view, _, window, cx| {
                                view.on_add_project_click(window, cx);
                            }),
                        )
                        .child("+"),
                ),
        )
}

/// Render a single rail icon (32px rounded square with letter, optional selected pill).
fn render_rail_icon(
    id: impl Into<gpui::ElementId>,
    label: String,
    is_selected: bool,
    count: usize,
    on_click: impl Fn(&gpui::MouseUpEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .child(
            // Selected pill indicator (4px wide, 24px tall)
            div()
                .w(px(4.0))
                .h(px(24.0))
                .rounded(px(2.0))
                .when(is_selected, |s| s.bg(theme::ice())),
        )
        .child(
            div()
                .id(id.into())
                .relative()
                .size(px(32.0))
                .rounded(px(theme::RADIUS_MD))
                .bg(if is_selected {
                    theme::surface()
                } else {
                    theme::border()
                })
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|s| s.bg(theme::surface()))
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(if is_selected {
                    theme::text_bright()
                } else {
                    theme::text_muted()
                })
                .on_mouse_up(gpui::MouseButton::Left, on_click)
                .child(label)
                // Badge count (top-right corner)
                .when(count > 0, |this| {
                    this.child(
                        div()
                            .absolute()
                            .top(px(-4.0))
                            .right(px(-4.0))
                            .min_w(px(16.0))
                            .h(px(16.0))
                            .rounded(px(8.0))
                            .bg(theme::border_strong())
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(9.0))
                            .text_color(theme::text_bright())
                            .child(count.to_string()),
                    )
                }),
        )
}
