//! Add project dialog component.
//!
//! Modal dialog for adding new projects with path input and optional name.

use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{Button, ButtonVariant, Modal, TextInput};
use crate::state::{AddProjectDialogField, AddProjectFormState, DialogState};
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
///
/// # Invalid State Handling
/// If called with a non-`DialogState::AddProject` state, logs an error and
/// displays "Internal error: invalid dialog state" to the user.
pub fn render_add_project_dialog(
    dialog: &DialogState,
    cx: &mut Context<MainView>,
) -> impl IntoElement {
    let (form, add_project_error) = match dialog {
        DialogState::AddProject { form, error } => (form, error.clone()),
        _ => {
            tracing::error!(
                event = "ui.add_project_dialog.invalid_state",
                "render_add_project_dialog called with non-AddProject dialog state"
            );
            (
                &AddProjectFormState::default(),
                Some("Internal error: invalid dialog state".to_string()),
            )
        }
    };

    let path = form.path.clone();
    let name = form.name.clone();
    let focused_field = form.focused_field.clone();

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
