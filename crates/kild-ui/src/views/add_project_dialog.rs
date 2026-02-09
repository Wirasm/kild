//! Add project dialog component.
//!
//! Modal dialog for adding new projects with path input and optional name.

use gpui::{Context, IntoElement, div, prelude::*, px};

use gpui_component::ActiveTheme;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};

use crate::state::DialogState;
use crate::theme;
use crate::views::MainView;

/// Render the add project dialog.
///
/// Text input is managed by gpui-component's Input widget via InputState entities
/// passed from MainView.
pub fn render_add_project_dialog(
    dialog: &DialogState,
    path_input: Option<&gpui::Entity<InputState>>,
    name_input: Option<&gpui::Entity<InputState>>,
    cx: &mut Context<MainView>,
) -> impl IntoElement {
    let DialogState::AddProject { error, .. } = dialog else {
        unreachable!(
            "render_add_project_dialog called with non-AddProject dialog state — this is a bug in MainView render logic"
        );
    };
    let add_project_error = error.clone();

    // Overlay: covers entire screen with semi-transparent background
    div()
        .id("add-project-dialog")
        .absolute()
        .inset_0()
        .bg(cx.theme().overlay)
        .flex()
        .justify_center()
        .items_center()
        // Dialog box: centered, themed container
        .child(
            div()
                .id("add-project-dialog-box")
                .w(px(450.))
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
                                .child("Add Project"),
                        ),
                )
                // Body
                .child(
                    div().px(px(theme::SPACE_4)).py(px(theme::SPACE_4)).child(
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
                                    .when_some(path_input, |this, input| {
                                        this.child(Input::new(input))
                                    }),
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
                                    .when_some(name_input, |this, input| {
                                        this.child(Input::new(input))
                                    }),
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
                                .child(
                                    Button::new("add-project-cancel-btn")
                                        .label("Cancel")
                                        .on_click(cx.listener(|view, _, _, cx| {
                                            view.on_add_project_cancel(cx);
                                        })),
                                )
                                .child(
                                    Button::new("add-project-submit-btn")
                                        .label("Add")
                                        .primary()
                                        .on_click(cx.listener(|view, _, _, cx| {
                                            view.on_add_project_submit(cx);
                                        })),
                                ),
                        ),
                ),
        )
}
