//! Add project dialog component.
//!
//! Modal dialog for adding new projects with path input and optional name.

use gpui::{Context, IntoElement, div, prelude::*, px, rgb};

use crate::state::{AddProjectDialogField, AppState};
use crate::views::MainView;

/// Render the add project dialog.
///
/// This is a modal dialog with:
/// - Semi-transparent overlay background
/// - Dialog box with form fields
/// - Path input (keyboard capture)
/// - Name input (optional)
/// - Cancel/Add buttons
/// - Error message display
pub fn render_add_project_dialog(state: &AppState, cx: &mut Context<MainView>) -> impl IntoElement {
    let path = state.add_project_form.path.clone();
    let name = state.add_project_form.name.clone();
    let focused_field = state.add_project_form.focused_field.clone();
    let add_project_error = state.add_project_error.clone();

    // Overlay background (press Escape or click Cancel to dismiss)
    div()
        .id("add-project-dialog-overlay")
        .absolute()
        .inset_0()
        .bg(gpui::rgba(0x000000aa))
        .flex()
        .justify_center()
        .items_center()
        // Dialog box
        .child(
            div()
                .id("add-project-dialog-box")
                .w(px(450.0))
                .bg(rgb(0x2d2d2d))
                .rounded_lg()
                .border_1()
                .border_color(rgb(0x444444))
                .flex()
                .flex_col()
                // Title bar
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(rgb(0x444444))
                        .child(
                            div()
                                .text_lg()
                                .text_color(rgb(0xffffff))
                                .child("Add Project"),
                        ),
                )
                // Form content
                .child(
                    div()
                        .px_4()
                        .py_4()
                        .flex()
                        .flex_col()
                        .gap_4()
                        // Path field
                        .child({
                            let is_focused = focused_field == AddProjectDialogField::Path;
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(div().text_sm().text_color(rgb(0xaaaaaa)).child("Path"))
                                .child(
                                    div()
                                        .px_3()
                                        .py_2()
                                        .bg(rgb(0x1e1e1e))
                                        .rounded_md()
                                        .border_1()
                                        .border_color(if is_focused {
                                            rgb(0x4a9eff)
                                        } else {
                                            rgb(0x555555)
                                        })
                                        .min_h(px(36.0))
                                        .child(
                                            div()
                                                .text_color(if path.is_empty() {
                                                    rgb(0x666666)
                                                } else {
                                                    rgb(0xffffff)
                                                })
                                                .child(if path.is_empty() {
                                                    "/path/to/repository".to_string()
                                                } else if is_focused {
                                                    format!("{}|", path)
                                                } else {
                                                    path.clone()
                                                }),
                                        ),
                                )
                        })
                        // Name field (optional)
                        .child({
                            let is_focused = focused_field == AddProjectDialogField::Name;
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(0xaaaaaa))
                                        .child("Name (optional)"),
                                )
                                .child(
                                    div()
                                        .px_3()
                                        .py_2()
                                        .bg(rgb(0x1e1e1e))
                                        .rounded_md()
                                        .border_1()
                                        .border_color(if is_focused {
                                            rgb(0x4a9eff)
                                        } else {
                                            rgb(0x555555)
                                        })
                                        .min_h(px(36.0))
                                        .child(
                                            div()
                                                .text_color(if name.is_empty() {
                                                    rgb(0x666666)
                                                } else {
                                                    rgb(0xffffff)
                                                })
                                                .child(if name.is_empty() {
                                                    "Defaults to directory name".to_string()
                                                } else if is_focused {
                                                    format!("{}|", name)
                                                } else {
                                                    name.clone()
                                                }),
                                        ),
                                )
                        })
                        // Error message (if any)
                        .when_some(add_project_error, |this, error| {
                            this.child(
                                div()
                                    .px_3()
                                    .py_2()
                                    .bg(rgb(0x3d1e1e))
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(0x662222))
                                    .child(div().text_sm().text_color(rgb(0xff6b6b)).child(error)),
                            )
                        }),
                )
                // Button row
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .border_t_1()
                        .border_color(rgb(0x444444))
                        .flex()
                        .justify_end()
                        .gap_2()
                        // Cancel button
                        .child(
                            div()
                                .id("add-project-cancel-btn")
                                .px_4()
                                .py_2()
                                .bg(rgb(0x444444))
                                .hover(|style| style.bg(rgb(0x555555)))
                                .rounded_md()
                                .cursor_pointer()
                                .on_mouse_up(
                                    gpui::MouseButton::Left,
                                    cx.listener(|view, _, _, cx| {
                                        view.on_add_project_cancel(cx);
                                    }),
                                )
                                .child(div().text_color(rgb(0xffffff)).child("Cancel")),
                        )
                        // Add button
                        .child(
                            div()
                                .id("add-project-submit-btn")
                                .px_4()
                                .py_2()
                                .bg(rgb(0x4a9eff))
                                .hover(|style| style.bg(rgb(0x5aafff)))
                                .rounded_md()
                                .cursor_pointer()
                                .on_mouse_up(
                                    gpui::MouseButton::Left,
                                    cx.listener(|view, _, _, cx| {
                                        view.on_add_project_submit(cx);
                                    }),
                                )
                                .child(div().text_color(rgb(0xffffff)).child("Add")),
                        ),
                ),
        )
}
