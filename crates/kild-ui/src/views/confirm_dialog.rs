//! Confirmation dialog component for destructive actions.
//!
//! Modal dialog that asks the user to confirm before destroying a kild.

use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{Button, ButtonVariant, Modal};
use crate::state::AppState;
use crate::theme;
use crate::views::MainView;

/// Render the confirmation dialog for destroying a kild.
///
/// This is a modal dialog with:
/// - Semi-transparent overlay background
/// - Dialog box with warning message
/// - Cancel and Destroy buttons
/// - Error message display (if destroy fails)
pub fn render_confirm_dialog(state: &AppState, cx: &mut Context<MainView>) -> impl IntoElement {
    let branch = state.confirm_target_branch.clone().unwrap_or_else(|| {
        tracing::warn!(
            event = "ui.confirm_dialog.missing_target_branch",
            "Confirm dialog rendered without target branch - this is a bug"
        );
        "unknown".to_string()
    });
    let confirm_error = state.confirm_error.clone();

    Modal::new("confirm-dialog", "Destroy KILD?")
        .body(
            div()
                .flex()
                .flex_col()
                .gap(px(theme::SPACE_3))
                .child(
                    div()
                        .text_color(theme::text_bright())
                        .child(format!("Destroy '{branch}'?")),
                )
                .child(
                    div()
                        .text_color(theme::text_subtle())
                        .text_size(px(theme::TEXT_SM))
                        .child(
                            "This will delete the working directory and stop any running agent.",
                        ),
                )
                .child(
                    div()
                        .text_color(theme::ember())
                        .text_size(px(theme::TEXT_SM))
                        .child("This cannot be undone."),
                )
                // Error message (if any)
                .when_some(confirm_error, |this, error| {
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
                    Button::new("confirm-cancel-btn", "Cancel")
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.on_confirm_cancel(cx);
                        })),
                )
                .child(
                    Button::new("confirm-destroy-btn", "Destroy")
                        .variant(ButtonVariant::Danger)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.on_confirm_destroy(cx);
                        })),
                ),
        )
}
