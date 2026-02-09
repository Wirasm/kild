//! Create kild dialog component.
//!
//! Modal dialog for creating new kilds with branch name input
//! and agent selection.

use gpui::{Context, IntoElement, div, prelude::*, px};

use gpui_component::ActiveTheme;
use gpui_component::Disableable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};

use crate::state::{CreateDialogField, DialogState};
use crate::theme;
use crate::views::MainView;

/// Available agent names for selection (pre-sorted by kild-core).
pub fn agent_options() -> Vec<&'static str> {
    kild_core::agents::valid_agent_names()
}

/// Render the create kild dialog.
///
/// Text input is managed by gpui-component's Input widget via InputState entities
/// passed from MainView. The dialog reads agent selection from DialogState form state.
pub fn render_create_dialog(
    dialog: &DialogState,
    loading: bool,
    branch_input: Option<&gpui::Entity<InputState>>,
    note_input: Option<&gpui::Entity<InputState>>,
    cx: &mut Context<MainView>,
) -> impl IntoElement {
    let DialogState::Create { form, error } = dialog else {
        unreachable!(
            "render_create_dialog called with non-Create dialog state — this is a bug in MainView render logic"
        );
    };
    let create_error = error.clone();

    let agents = agent_options();
    let current_agent = form.selected_agent();
    let focused_field = form.focused_field.clone();
    let selected_agent_index = form.selected_agent_index();

    // Overlay: covers entire screen with semi-transparent background
    div()
        .id("create-dialog")
        .absolute()
        .inset_0()
        .bg(cx.theme().overlay)
        .flex()
        .justify_center()
        .items_center()
        // Dialog box: centered, themed container
        .child(
            div()
                .id("create-dialog-box")
                .w(px(400.))
                .bg(cx.theme().background)
                .rounded(cx.theme().radius_lg)
                .border_1()
                .border_color(cx.theme().border)
                .flex()
                .flex_col()
                // Header: title with bottom border
                .child(
                    div()
                        .px(px(theme::SPACE_4))
                        .py(px(theme::SPACE_3))
                        .border_b_1()
                        .border_color(theme::border_subtle())
                        .child(
                            div()
                                .text_size(px(theme::TEXT_LG))
                                .text_color(theme::text_bright())
                                .child("Create New KILD"),
                        ),
                )
                // Body
                .child(
                    div().px(px(theme::SPACE_4)).py(px(theme::SPACE_4)).child(
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
                                    .when_some(branch_input, |this, input| {
                                        this.child(Input::new(input))
                                    }),
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
                                                                selected_agent_index + 1,
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
                                    .when_some(note_input, |this, input| {
                                        this.child(Input::new(input))
                                    }),
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
                    ),
                )
                // Footer: buttons
                .child(
                    div()
                        .px(px(theme::SPACE_4))
                        .py(px(theme::SPACE_3))
                        .border_t_1()
                        .border_color(theme::border_subtle())
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(theme::SPACE_2))
                                .child(Button::new("cancel-btn").label("Cancel").on_click(
                                    cx.listener(|view, _, _, cx| {
                                        view.on_dialog_cancel(cx);
                                    }),
                                ))
                                .child({
                                    let button_text =
                                        if loading { "Creating..." } else { "Create" };
                                    Button::new("create-btn")
                                        .label(button_text)
                                        .primary()
                                        .disabled(loading)
                                        .on_click(cx.listener(|view, _, _, cx| {
                                            view.on_dialog_submit(cx);
                                        }))
                                }),
                        ),
                ),
        )
}
