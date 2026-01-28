//! Create kild dialog component.
//!
//! Modal dialog for creating new kilds with branch name input
//! and agent selection.

use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{Button, ButtonVariant, Modal, TextInput};
use crate::state::{AppState, CreateDialogField};
use crate::theme;
use crate::views::MainView;

/// Available agent names for selection (pre-sorted by kild-core).
pub fn agent_options() -> Vec<&'static str> {
    kild_core::agents::valid_agent_names()
}

/// Render the create kild dialog.
///
/// This is a modal dialog with:
/// - Semi-transparent overlay background
/// - Dialog box with form fields
/// - Branch name input (keyboard capture)
/// - Agent selection (click to cycle)
/// - Cancel/Create buttons
/// - Error message display
pub fn render_create_dialog(state: &AppState, cx: &mut Context<MainView>) -> impl IntoElement {
    let agents = agent_options();
    let current_agent = state.create_form.selected_agent();
    let branch_name = state.create_form.branch_name.clone();
    let note = state.create_form.note.clone();
    let focused_field = state.create_form.focused_field.clone();
    let create_error = state.create_error.clone();

    Modal::new("create-dialog", "Create New KILD")
        .body(
            div()
                .flex()
                .flex_col()
                .gap(px(theme::SPACE_4))
                // Branch name field
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(theme::SPACE_1))
                        .child(
                            div()
                                .text_size(px(theme::TEXT_SM))
                                .text_color(theme::text_subtle())
                                .child("Branch Name"),
                        )
                        .child(
                            TextInput::new("branch-input")
                                .value(&branch_name)
                                .placeholder("Type branch name...")
                                .focused(focused_field == CreateDialogField::BranchName),
                        ),
                )
                // Agent selection field (custom - click to cycle)
                .child({
                    let is_focused = focused_field == CreateDialogField::Agent;
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(theme::SPACE_1))
                        .child(
                            div()
                                .text_size(px(theme::TEXT_SM))
                                .text_color(theme::text_subtle())
                                .child("Agent"),
                        )
                        .child(
                            div()
                                .id("agent-selector")
                                .px(px(theme::SPACE_3))
                                .py(px(theme::SPACE_2))
                                .bg(theme::surface())
                                .hover(|style| style.bg(theme::elevated()))
                                .rounded(px(theme::RADIUS_MD))
                                .border_1()
                                .border_color(if is_focused {
                                    theme::ice()
                                } else {
                                    theme::border()
                                })
                                .cursor_pointer()
                                .on_mouse_up(
                                    gpui::MouseButton::Left,
                                    cx.listener(|view, _, _, cx| {
                                        view.on_agent_cycle(cx);
                                    }),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .justify_between()
                                        .items_center()
                                        .child(
                                            div()
                                                .text_color(theme::text_bright())
                                                .child(current_agent),
                                        )
                                        .child(
                                            div()
                                                .text_color(theme::text_subtle())
                                                .text_size(px(theme::TEXT_SM))
                                                .child(format!(
                                                    "({}/{})",
                                                    state.create_form.selected_agent_index + 1,
                                                    agents.len()
                                                )),
                                        ),
                                ),
                        )
                })
                // Note field (optional)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(theme::SPACE_1))
                        .child(
                            div()
                                .text_size(px(theme::TEXT_SM))
                                .text_color(theme::text_subtle())
                                .child("Note (optional)"),
                        )
                        .child(
                            TextInput::new("note-input")
                                .value(&note)
                                .placeholder("What is this kild for?")
                                .focused(focused_field == CreateDialogField::Note),
                        ),
                )
                // Error message (if any)
                .when_some(create_error, |this, error| {
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
                    Button::new("cancel-btn", "Cancel")
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.on_dialog_cancel(cx);
                        })),
                )
                .child(
                    Button::new("create-btn", "Create")
                        .variant(ButtonVariant::Primary)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.on_dialog_submit(cx);
                        })),
                ),
        )
}
