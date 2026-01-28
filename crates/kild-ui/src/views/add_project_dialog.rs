//! Add project dialog component.
//!
//! Modal dialog for adding new projects with path input and optional name.

use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{Button, ButtonVariant, Modal, TextInput};
use crate::state::{AddProjectDialogField, AppState};
use crate::theme;
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

    Modal::new("add-project-dialog", "Add Project")
        .width(px(450.0))
        .body(
            div()
                .flex()
                .flex_col()
                .gap(px(theme::SPACE_4))
                // Path field
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(theme::SPACE_1))
                        .child(
                            div()
                                .text_size(px(theme::TEXT_SM))
                                .text_color(theme::text_subtle())
                                .child("Path"),
                        )
                        .child(
                            TextInput::new("path-input")
                                .value(&path)
                                .placeholder("/path/to/repository")
                                .focused(focused_field == AddProjectDialogField::Path),
                        ),
                )
                // Name field (optional)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(theme::SPACE_1))
                        .child(
                            div()
                                .text_size(px(theme::TEXT_SM))
                                .text_color(theme::text_subtle())
                                .child("Name (optional)"),
                        )
                        .child(
                            TextInput::new("name-input")
                                .value(&name)
                                .placeholder("Defaults to directory name")
                                .focused(focused_field == AddProjectDialogField::Name),
                        ),
                )
                // Error message (if any)
                .when_some(add_project_error, |this, error| {
                    this.child(
                        div()
                            .px(px(theme::SPACE_3))
                            .py(px(theme::SPACE_2))
                            .bg(theme::with_alpha(theme::ember(), 0.2))
                            .rounded(px(theme::RADIUS_MD))
                            .border_1()
                            .border_color(theme::ember())
                            .child(
                                div()
                                    .text_size(px(theme::TEXT_SM))
                                    .text_color(theme::ember())
                                    .child(error),
                            ),
                    )
                }),
        )
        .footer(
            div()
                .flex()
                .justify_end()
                .gap(px(theme::SPACE_2))
                .child(
                    Button::new("add-project-cancel-btn", "Cancel")
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.on_add_project_cancel(cx);
                        })),
                )
                .child(
                    Button::new("add-project-submit-btn", "Add")
                        .variant(ButtonVariant::Primary)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.on_add_project_submit(cx);
                        })),
                ),
        )
}
