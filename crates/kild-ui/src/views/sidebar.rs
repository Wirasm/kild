//! Project sidebar component.
//!
//! Fixed left sidebar (200px) for project navigation.

use gpui::{Context, FontWeight, IntoElement, ParentElement, Styled, div, prelude::*, px};
use std::path::PathBuf;

use crate::state::AppState;
use crate::theme;
use crate::views::main_view::MainView;
use gpui_component::button::{Button, ButtonVariants};

/// Width of the sidebar in pixels.
pub const SIDEBAR_WIDTH: f32 = 200.0;

/// Data for rendering a project item in the sidebar.
struct ProjectItemData {
    list_position: usize,
    path: PathBuf,
    name: String,
    first_char: String,
    is_selected: bool,
    count: usize,
}

impl ProjectItemData {
    fn from_project(
        idx: usize,
        project: &kild_core::projects::Project,
        active_path: Option<&std::path::Path>,
        state: &AppState,
    ) -> Self {
        let path = project.path().to_path_buf();
        let is_selected = active_path == Some(project.path());
        let count = state.kild_count_for_project(project.path());
        let name = project.name().to_string();
        let first_char = name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| {
                tracing::debug!(
                    event = "ui.sidebar.empty_project_name",
                    project_path = %path.display(),
                    "Project has empty name - showing '?' icon"
                );
                "?".to_string()
            });

        Self {
            list_position: idx,
            path,
            name,
            first_char,
            is_selected,
            count,
        }
    }
}

/// Render the project sidebar.
pub fn render_sidebar(state: &AppState, cx: &mut Context<MainView>) -> impl IntoElement {
    let active_path = state.active_project_path();
    let total_count = state.total_kild_count();

    // Prepare project data for rendering
    let projects_for_list: Vec<ProjectItemData> = state
        .projects_iter()
        .enumerate()
        .map(|(idx, project)| ProjectItemData::from_project(idx, project, active_path, state))
        .collect();

    let is_all_selected = active_path.is_none();
    let active_for_footer = active_path.map(|p| p.to_path_buf());

    div()
        .w(px(SIDEBAR_WIDTH))
        .h_full()
        .bg(theme::obsidian())
        .border_r_1()
        .border_color(theme::border_subtle())
        .flex()
        .flex_col()
        // Header: "SCOPE"
        .child(
            div()
                .px(px(theme::SPACE_4))
                .py(px(theme::SPACE_3))
                .border_b_1()
                .border_color(theme::border_subtle())
                .text_size(px(theme::TEXT_XS))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text_muted())
                .child("SCOPE"),
        )
        // Project list content
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                // "All Projects" option
                .child(
                    div()
                        .id("sidebar-all-projects")
                        .flex()
                        .items_center()
                        .gap(px(theme::SPACE_2))
                        .px(px(theme::SPACE_4))
                        .py(px(theme::SPACE_2))
                        .cursor_pointer()
                        .hover(|style| style.bg(theme::surface()))
                        .when(is_all_selected, |this| {
                            this.bg(theme::surface())
                                .border_l_2()
                                .border_color(theme::ice())
                                .pl(px(theme::SPACE_4 - SELECTED_PADDING_ADJUSTMENT))
                        })
                        .on_mouse_up(
                            gpui::MouseButton::Left,
                            cx.listener(|view, _, _, cx| {
                                view.on_project_select_all(cx);
                            }),
                        )
                        // Radio indicator
                        .child(render_radio_indicator(is_all_selected))
                        // "All" text
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(theme::TEXT_SM))
                                .text_color(theme::text())
                                .child("All"),
                        )
                        // Count badge
                        .child(render_count_badge(total_count)),
                )
                // Project list
                .children(projects_for_list.into_iter().map(|data| {
                    let ProjectItemData {
                        list_position,
                        path,
                        name,
                        first_char,
                        is_selected,
                        count,
                    } = data;

                    div()
                        .id(("sidebar-project", list_position))
                        .flex()
                        .items_center()
                        .gap(px(theme::SPACE_2))
                        .px(px(theme::SPACE_4))
                        .py(px(theme::SPACE_2))
                        .cursor_pointer()
                        .hover(|style| style.bg(theme::surface()))
                        .when(is_selected, |this| {
                            this.bg(theme::surface())
                                .border_l_2()
                                .border_color(theme::ice())
                                .pl(px(theme::SPACE_4 - SELECTED_PADDING_ADJUSTMENT))
                        })
                        .on_mouse_up(gpui::MouseButton::Left, {
                            let path = path.clone();
                            cx.listener(move |view, _, _, cx| {
                                view.on_project_select(path.clone(), cx);
                            })
                        })
                        // Project icon (first letter)
                        .child(
                            div()
                                .size(px(16.0))
                                .bg(theme::border())
                                .rounded(px(theme::RADIUS_SM))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(10.0))
                                .text_color(theme::text_muted())
                                .child(first_char),
                        )
                        // Project name
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(theme::TEXT_SM))
                                .text_color(theme::text())
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(name),
                        )
                        // Count badge
                        .child(render_count_badge(count))
                })),
        )
        // Footer: Add Project button (and Remove if project selected)
        .child(
            div()
                .px(px(theme::SPACE_4))
                .py(px(theme::SPACE_3))
                .border_t_1()
                .border_color(theme::border_subtle())
                .flex()
                .flex_col()
                .gap(px(theme::SPACE_2))
                // Add Project button
                .child(
                    Button::new("sidebar-add-project")
                        .label("+ Add Project")
                        .ghost()
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.on_add_project_click(window, cx);
                        })),
                )
                // Remove current (only if project selected)
                .when_some(active_for_footer, |this, path| {
                    this.child(
                        div()
                            .id("sidebar-remove-project")
                            .w_full()
                            .px(px(theme::SPACE_3))
                            .py(px(theme::SPACE_2))
                            .rounded(px(theme::RADIUS_MD))
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::surface()))
                            .on_mouse_up(gpui::MouseButton::Left, {
                                cx.listener(move |view, _, _, cx| {
                                    view.on_remove_project(path.clone(), cx);
                                })
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(theme::SPACE_1))
                                    .text_size(px(theme::TEXT_SM))
                                    .text_color(theme::ember())
                                    .child("−")
                                    .child("Remove current"),
                            ),
                    )
                }),
        )
}

/// Render a radio button indicator (selected or unselected).
fn render_radio_indicator(is_selected: bool) -> impl IntoElement {
    let color = if is_selected {
        theme::ice()
    } else {
        theme::border()
    };
    let symbol = if is_selected { "●" } else { "○" };

    div().w(px(16.0)).text_color(color).child(symbol)
}

/// Padding adjustment when selected. Reduces left padding by 2px to account
/// for the 2px left border, keeping text alignment consistent.
const SELECTED_PADDING_ADJUSTMENT: f32 = 2.0;

fn render_count_badge(count: usize) -> impl IntoElement {
    div()
        .text_size(px(theme::TEXT_XS))
        .text_color(theme::text_muted())
        .bg(theme::border_subtle())
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(10.0))
        .child(count.to_string())
}
