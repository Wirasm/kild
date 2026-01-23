//! Create shard dialog component.
//!
//! Modal dialog for creating new shards with branch name input
//! and agent selection.

use gpui::{Context, IntoElement, div, prelude::*, px, rgb};

use crate::state::AppState;
use crate::views::MainView;

/// Available agent names for selection (pre-sorted by shards-core).
pub fn agent_options() -> Vec<&'static str> {
    shards_core::agents::valid_agent_names()
}

/// Render the create shard dialog.
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
    let current_agent = state.create_form.selected_agent.clone();
    let branch_name = state.create_form.branch_name.clone();
    let create_error = state.create_error.clone();

    // Overlay background (press Escape or click Cancel to dismiss)
    div()
        .id("dialog-overlay")
        .absolute()
        .inset_0()
        .bg(gpui::rgba(0x000000aa))
        .flex()
        .justify_center()
        .items_center()
        // Dialog box
        .child(
            div()
                .id("dialog-box")
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
                                .child("Create New Shard"),
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
                        // Branch name field
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(0xaaaaaa))
                                        .child("Branch Name"),
                                )
                                .child(
                                    div()
                                        .px_3()
                                        .py_2()
                                        .bg(rgb(0x1e1e1e))
                                        .rounded_md()
                                        .border_1()
                                        .border_color(rgb(0x555555))
                                        .min_h(px(36.0))
                                        .child(
                                            div()
                                                .text_color(if branch_name.is_empty() {
                                                    rgb(0x666666)
                                                } else {
                                                    rgb(0xffffff)
                                                })
                                                .child(if branch_name.is_empty() {
                                                    "Type branch name...".to_string()
                                                } else {
                                                    format!("{}|", branch_name)
                                                }),
                                        ),
                                ),
                        )
                        // Agent selection field
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(div().text_sm().text_color(rgb(0xaaaaaa)).child("Agent"))
                                .child(
                                    div()
                                        .id("agent-selector")
                                        .px_3()
                                        .py_2()
                                        .bg(rgb(0x1e1e1e))
                                        .hover(|style| style.bg(rgb(0x2a2a2a)))
                                        .rounded_md()
                                        .border_1()
                                        .border_color(rgb(0x555555))
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
                                                        .text_color(rgb(0xffffff))
                                                        .child(current_agent),
                                                )
                                                .child(
                                                    div()
                                                        .text_color(rgb(0x888888))
                                                        .text_sm()
                                                        .child(format!(
                                                            "({}/{})",
                                                            state.create_form.selected_agent_index
                                                                + 1,
                                                            agents.len()
                                                        )),
                                                ),
                                        ),
                                ),
                        )
                        // Error message (if any)
                        .when_some(create_error, |this, error| {
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
                                .id("cancel-btn")
                                .px_4()
                                .py_2()
                                .bg(rgb(0x444444))
                                .hover(|style| style.bg(rgb(0x555555)))
                                .rounded_md()
                                .cursor_pointer()
                                .on_mouse_up(
                                    gpui::MouseButton::Left,
                                    cx.listener(|view, _, _, cx| {
                                        view.on_dialog_cancel(cx);
                                    }),
                                )
                                .child(div().text_color(rgb(0xffffff)).child("Cancel")),
                        )
                        // Create button
                        .child(
                            div()
                                .id("create-btn")
                                .px_4()
                                .py_2()
                                .bg(rgb(0x4a9eff))
                                .hover(|style| style.bg(rgb(0x5aafff)))
                                .rounded_md()
                                .cursor_pointer()
                                .on_mouse_up(
                                    gpui::MouseButton::Left,
                                    cx.listener(|view, _, _, cx| {
                                        view.on_dialog_submit(cx);
                                    }),
                                )
                                .child(div().text_color(rgb(0xffffff)).child("Create")),
                        ),
                ),
        )
}
