//! Confirmation dialog component for destructive actions.
//!
//! Modal dialog that asks the user to confirm before destroying a shard.

use gpui::{Context, IntoElement, div, prelude::*, px, rgb};

use crate::state::AppState;
use crate::views::MainView;

/// Render the confirmation dialog for destroying a shard.
///
/// This is a modal dialog with:
/// - Semi-transparent overlay background
/// - Dialog box with warning message
/// - Cancel and Destroy buttons
/// - Error message display (if destroy fails)
pub fn render_confirm_dialog(state: &AppState, cx: &mut Context<MainView>) -> impl IntoElement {
    let branch = state
        .confirm_target_branch
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let confirm_error = state.confirm_error.clone();

    // Overlay background
    div()
        .id("confirm-dialog-overlay")
        .absolute()
        .inset_0()
        .bg(gpui::rgba(0x000000aa))
        .flex()
        .justify_center()
        .items_center()
        // Dialog box
        .child(
            div()
                .id("confirm-dialog-box")
                .w(px(400.0))
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
                                .child("Destroy Shard?"),
                        ),
                )
                // Warning message
                .child(
                    div()
                        .px_4()
                        .py_4()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(
                            div()
                                .text_color(rgb(0xffffff))
                                .child(format!("Destroy '{branch}'?")),
                        )
                        .child(div().text_color(rgb(0xaaaaaa)).text_sm().child(
                            "This will delete the working directory and stop any running agent.",
                        ))
                        .child(
                            div()
                                .text_color(rgb(0xff6b6b))
                                .text_sm()
                                .child("This cannot be undone."),
                        )
                        // Error message (if any)
                        .when_some(confirm_error, |this, error| {
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
                        // Cancel button (gray)
                        .child(
                            div()
                                .id("confirm-cancel-btn")
                                .px_4()
                                .py_2()
                                .bg(rgb(0x444444))
                                .hover(|style| style.bg(rgb(0x555555)))
                                .rounded_md()
                                .cursor_pointer()
                                .on_mouse_up(
                                    gpui::MouseButton::Left,
                                    cx.listener(|view, _, _, cx| {
                                        view.on_confirm_cancel(cx);
                                    }),
                                )
                                .child(div().text_color(rgb(0xffffff)).child("Cancel")),
                        )
                        // Destroy button (red/danger)
                        .child(
                            div()
                                .id("confirm-destroy-btn")
                                .px_4()
                                .py_2()
                                .bg(rgb(0xcc4444))
                                .hover(|style| style.bg(rgb(0xdd5555)))
                                .rounded_md()
                                .cursor_pointer()
                                .on_mouse_up(
                                    gpui::MouseButton::Left,
                                    cx.listener(|view, _, _, cx| {
                                        view.on_confirm_destroy(cx);
                                    }),
                                )
                                .child(div().text_color(rgb(0xffffff)).child("Destroy")),
                        ),
                ),
        )
}
