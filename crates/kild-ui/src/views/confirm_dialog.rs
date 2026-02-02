//! Confirmation dialog component for destructive actions.
//!
//! Modal dialog that asks the user to confirm before destroying a kild.
//! Shows git-aware warnings about uncommitted changes, unpushed commits, etc.

use gpui::{Context, IntoElement, div, prelude::*, px};
use kild_core::DestroySafetyInfo;

use crate::components::{Button, ButtonVariant, Modal};
use crate::state::DialogState;
use crate::theme;
use crate::views::MainView;

/// Render the confirmation dialog for destroying a kild.
///
/// This is a modal dialog with:
/// - Semi-transparent overlay background
/// - Dialog box with warning message
/// - Git-aware warnings (uncommitted changes, unpushed commits, etc.)
/// - Cancel and Destroy buttons
/// - Error message display (if destroy fails)
///
/// # Warning Display
/// - Red warning box: Uncommitted changes (blocking - button says "Force Destroy")
/// - Amber warning box: Unpushed commits, no PR, never pushed (non-blocking)
///
/// # Invalid State Handling
/// If called with a non-`DialogState::Confirm` state, logs an error and
/// displays "Internal error: invalid dialog state" to the user.
pub fn render_confirm_dialog(
    dialog: &DialogState,
    loading: bool,
    cx: &mut Context<MainView>,
) -> impl IntoElement {
    let (branch, safety_info, confirm_error) = match dialog {
        DialogState::Confirm {
            branch,
            safety_info,
            error,
        } => (branch.clone(), safety_info.clone(), error.clone()),
        _ => {
            tracing::error!(
                event = "ui.confirm_dialog.invalid_state",
                "render_confirm_dialog called with non-Confirm dialog state"
            );
            (
                "unknown".to_string(),
                None,
                Some("Internal error: invalid dialog state".to_string()),
            )
        }
    };

    // Determine if we should block (uncommitted changes)
    let should_block = safety_info
        .as_ref()
        .map(|s| s.should_block())
        .unwrap_or(false);

    // Button text changes based on blocking state
    let destroy_button_text = if should_block {
        "Force Destroy"
    } else {
        "Destroy"
    };

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
                // Safety warnings (if any)
                .when_some(safety_info, |this, info| {
                    if info.has_warnings() {
                        this.child(render_safety_warnings(&info))
                    } else {
                        this
                    }
                })
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
                .child({
                    let button_text = if loading {
                        "Destroying..."
                    } else {
                        destroy_button_text
                    };
                    Button::new("confirm-destroy-btn", button_text)
                        .variant(ButtonVariant::Danger)
                        .disabled(loading)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.on_confirm_destroy(cx);
                        }))
                }),
        )
}

/// Render the safety warnings box.
///
/// Uses red styling for blocking warnings (uncommitted changes),
/// amber styling for non-blocking warnings (unpushed commits, etc.).
fn render_safety_warnings(info: &DestroySafetyInfo) -> impl IntoElement {
    let warnings = info.warning_messages();
    let is_blocking = info.should_block();

    // Use red for blocking (uncommitted changes), amber for warnings only
    let (bg_color, border_color, text_color) = if is_blocking {
        (
            theme::with_alpha(theme::ember(), 0.15),
            theme::ember(),
            theme::ember(),
        )
    } else {
        (
            theme::with_alpha(theme::copper(), 0.15),
            theme::copper(),
            theme::copper(),
        )
    };

    div()
        .px(px(theme::SPACE_3))
        .py(px(theme::SPACE_2))
        .bg(bg_color)
        .rounded(px(theme::RADIUS_MD))
        .border_1()
        .border_color(border_color)
        .flex()
        .flex_col()
        .gap(px(theme::SPACE_1))
        .children(warnings.into_iter().map(move |warning| {
            div()
                .text_size(px(theme::TEXT_SM))
                .text_color(text_color)
                .child(format!("⚠ {}", warning))
        }))
}
