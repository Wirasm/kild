//! Project selector dropdown component.
//!
//! Dropdown for switching between projects and adding new ones.

use gpui::{Context, IntoElement, div, prelude::*, px, rgb};

use crate::projects::Project;
use crate::state::AppState;
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
            .px_3()
            .py_1()
            .bg(rgb(0x444444))
            .hover(|style| style.bg(rgb(0x555555)))
            .rounded_md()
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
                    .gap_1()
                    .child(div().text_color(rgb(0xffffff)).child("+"))
                    .child(div().text_color(rgb(0xffffff)).child("Add Project")),
            )
            .into_any_element();
    }

    // Find active project name
    let active_name = active_project
        .as_ref()
        .and_then(|path| projects.iter().find(|p| &p.path == path))
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "Select Project".to_string());

    // Clone data needed for dropdown items
    let projects_for_dropdown: Vec<Project> = projects.clone();
    let active_for_dropdown = active_project.clone();

    div()
        .id("project-selector")
        .relative()
        .child(
            // Trigger button
            div()
                .id("project-selector-trigger")
                .px_3()
                .py_1()
                .bg(rgb(0x444444))
                .hover(|style| style.bg(rgb(0x555555)))
                .rounded_md()
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
                        .gap_2()
                        .child(
                            div()
                                .text_color(rgb(0xffffff))
                                .max_w(px(150.0))
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(active_name),
                        )
                        .child(
                            div()
                                .text_color(rgb(0x888888))
                                .text_sm()
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
                    .bg(rgb(0x2d2d2d))
                    .border_1()
                    .border_color(rgb(0x444444))
                    .rounded_md()
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    // Project list
                    .children(
                        projects_for_dropdown
                            .iter()
                            .enumerate()
                            .map(|(idx, project)| {
                                let path = project.path.clone();
                                let is_active = active_for_dropdown.as_ref() == Some(&project.path);
                                let name = project.name.clone();

                                div()
                                    .id(("project-item", idx))
                                    .px_3()
                                    .py_2()
                                    .hover(|style| style.bg(rgb(0x3d3d3d)))
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
                                            .gap_2()
                                            .child(
                                                div()
                                                    .w(px(16.0))
                                                    .text_color(if is_active {
                                                        rgb(0x4a9eff)
                                                    } else {
                                                        rgb(0x444444)
                                                    })
                                                    .child(if is_active { "●" } else { "○" }),
                                            )
                                            .child(
                                                div()
                                                    .text_color(rgb(0xffffff))
                                                    .overflow_hidden()
                                                    .text_ellipsis()
                                                    .child(name),
                                            ),
                                    )
                            }),
                    )
                    // Divider
                    .child(div().h(px(1.0)).bg(rgb(0x444444)).mx_2().my_1())
                    // Add Project option
                    .child(
                        div()
                            .id("project-add-option")
                            .px_3()
                            .py_2()
                            .hover(|style| style.bg(rgb(0x3d3d3d)))
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
                                    .gap_2()
                                    .child(div().w(px(16.0)).text_color(rgb(0x4a9eff)).child("+"))
                                    .child(div().text_color(rgb(0xffffff)).child("Add Project")),
                            ),
                    )
                    // Remove current option (only if there's an active project)
                    .when(active_for_dropdown.is_some(), |this| {
                        let active_path = active_for_dropdown.clone().unwrap();
                        this.child(
                            div()
                                .id("project-remove-option")
                                .px_3()
                                .py_2()
                                .hover(|style| style.bg(rgb(0x3d3d3d)))
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
                                        .gap_2()
                                        .child(
                                            div().w(px(16.0)).text_color(rgb(0xff6b6b)).child("−"),
                                        )
                                        .child(
                                            div().text_color(rgb(0xff6b6b)).child("Remove current"),
                                        ),
                                ),
                        )
                    }),
            )
        })
        .into_any_element()
}
