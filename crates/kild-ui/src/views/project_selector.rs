//! Project selector dropdown component.
//!
//! Dropdown for switching between projects and adding new ones.

use gpui::{Context, FontWeight, IntoElement, div, prelude::*, px};

use crate::projects::Project;
use crate::state::AppState;
use crate::theme;
use crate::views::MainView;

/// Render the project selector dropdown.
///
/// States:
/// - No projects: Show "Add Project" button
/// - Projects exist: Show dropdown with active project name
pub fn render_project_selector(state: &AppState, cx: &mut Context<MainView>) -> impl IntoElement {
    let projects = &state.projects;
    let active_project = &state.active_project;
    let show_dropdown = state.show_project_dropdown;

    if projects.is_empty() {
        // No projects - show Add Project button
        return div()
            .id("project-selector-empty")
            .px(px(theme::SPACE_3))
            .py(px(theme::SPACE_1))
            .bg(theme::blade())
            .hover(|style| style.bg(theme::blade_bright()))
            .rounded(px(theme::RADIUS_MD))
            .cursor_pointer()
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|view, _, _, cx| {
                    view.on_add_project_click(cx);
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(theme::SPACE_1))
                    .child(div().text_color(theme::text_white()).child("+"))
                    .child(div().text_color(theme::text_white()).child("Add Project")),
            )
            .into_any_element();
    }

    let active_name = match active_project {
        Some(path) => projects
            .iter()
            .find(|p| p.path() == path)
            .map(|p| p.name().to_string())
            .unwrap_or_else(|| "Select Project".to_string()),
        None => "All Projects".to_string(),
    };

    let projects_for_dropdown: Vec<Project> = projects.clone();
    let active_for_dropdown = active_project.clone();

    div()
        .id("project-selector")
        .relative()
        .child(
            // Trigger button
            div()
                .id("project-selector-trigger")
                .px(px(theme::SPACE_3))
                .py(px(theme::SPACE_1))
                .bg(theme::blade())
                .hover(|style| style.bg(theme::blade_bright()))
                .rounded(px(theme::RADIUS_MD))
                .cursor_pointer()
                .on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|view, _, _, cx| {
                        view.on_toggle_project_dropdown(cx);
                    }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(theme::SPACE_2))
                        .child(
                            div()
                                .text_color(theme::text_white())
                                .max_w(px(150.0))
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(active_name),
                        )
                        .child(
                            div()
                                .text_color(theme::text_subtle())
                                .text_size(px(theme::TEXT_SM))
                                .child(if show_dropdown { "▲" } else { "▼" }),
                        ),
                ),
        )
        // Dropdown menu (only when open)
        .when(show_dropdown, |this| {
            this.child(
                div()
                    .id("project-dropdown-menu")
                    .absolute()
                    .top(px(36.0))
                    .left_0()
                    .min_w(px(200.0))
                    .max_w(px(300.0))
                    .bg(theme::elevated())
                    .border_1()
                    .border_color(theme::border())
                    .rounded(px(theme::RADIUS_MD))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    // "All Projects" option
                    .child(
                        div()
                            .id("project-all")
                            .px(px(theme::SPACE_3))
                            .py(px(theme::SPACE_2))
                            .hover(|style| style.bg(theme::surface()))
                            .cursor_pointer()
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|view, _, _, cx| {
                                    view.on_project_select_all(cx);
                                }),
                            )
                            .child({
                                let is_active = active_for_dropdown.is_none();
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(theme::SPACE_2))
                                    .child(
                                        div()
                                            .w(px(16.0))
                                            .text_color(if is_active {
                                                theme::ice()
                                            } else {
                                                theme::border()
                                            })
                                            .child(if is_active { "●" } else { "○" }),
                                    )
                                    .child(
                                        div()
                                            .text_color(theme::text_white())
                                            .font_weight(FontWeight::MEDIUM)
                                            .child("All Projects"),
                                    )
                            }),
                    )
                    // Divider after "All Projects"
                    .child(
                        div()
                            .h(px(1.0))
                            .bg(theme::border_subtle())
                            .mx(px(theme::SPACE_2))
                            .my(px(theme::SPACE_1)),
                    )
                    // Project list
                    .children(
                        projects_for_dropdown
                            .iter()
                            .enumerate()
                            .map(|(idx, project)| {
                                let path = project.path().to_path_buf();
                                let is_active = active_for_dropdown.as_ref() == Some(&path);
                                let name = project.name().to_string();

                                div()
                                    .id(("project-item", idx))
                                    .px(px(theme::SPACE_3))
                                    .py(px(theme::SPACE_2))
                                    .hover(|style| style.bg(theme::surface()))
                                    .cursor_pointer()
                                    .on_mouse_up(gpui::MouseButton::Left, {
                                        let path = path.clone();
                                        cx.listener(move |view, _, _, cx| {
                                            view.on_project_select(path.clone(), cx);
                                        })
                                    })
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(theme::SPACE_2))
                                            .child(
                                                div()
                                                    .w(px(16.0))
                                                    .text_color(if is_active {
                                                        theme::ice()
                                                    } else {
                                                        theme::border()
                                                    })
                                                    .child(if is_active { "●" } else { "○" }),
                                            )
                                            .child(
                                                div()
                                                    .text_color(theme::text_white())
                                                    .overflow_hidden()
                                                    .text_ellipsis()
                                                    .child(name),
                                            ),
                                    )
                            }),
                    )
                    // Divider
                    .child(
                        div()
                            .h(px(1.0))
                            .bg(theme::border_subtle())
                            .mx(px(theme::SPACE_2))
                            .my(px(theme::SPACE_1)),
                    )
                    // Add Project option
                    .child(
                        div()
                            .id("project-add-option")
                            .px(px(theme::SPACE_3))
                            .py(px(theme::SPACE_2))
                            .hover(|style| style.bg(theme::surface()))
                            .cursor_pointer()
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|view, _, _, cx| {
                                    view.on_add_project_click(cx);
                                }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(theme::SPACE_2))
                                    .child(div().w(px(16.0)).text_color(theme::ice()).child("+"))
                                    .child(
                                        div().text_color(theme::text_white()).child("Add Project"),
                                    ),
                            ),
                    )
                    // Remove current option (only if there's an active project)
                    .when(active_for_dropdown.is_some(), |this| {
                        let active_path = active_for_dropdown.clone().unwrap();
                        this.child(
                            div()
                                .id("project-remove-option")
                                .px(px(theme::SPACE_3))
                                .py(px(theme::SPACE_2))
                                .hover(|style| style.bg(theme::surface()))
                                .cursor_pointer()
                                .on_mouse_up(gpui::MouseButton::Left, {
                                    cx.listener(move |view, _, _, cx| {
                                        view.on_remove_project(active_path.clone(), cx);
                                    })
                                })
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(theme::SPACE_2))
                                        .child(
                                            div().w(px(16.0)).text_color(theme::ember()).child("−"),
                                        )
                                        .child(
                                            div()
                                                .text_color(theme::ember())
                                                .child("Remove current"),
                                        ),
                                ),
                        )
                    }),
            )
        })
        .into_any_element()
}
