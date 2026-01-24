//! Shard list view component.
//!
//! Renders the list of shards with status indicators, session info, and action buttons.

use gpui::{Context, IntoElement, div, prelude::*, rgb, uniform_list};

use crate::state::{AppState, ProcessStatus};
use crate::views::MainView;

/// Render the shard list based on current state.
///
/// Handles three states:
/// - Error: Display error message
/// - Empty: Display "No active shards" message
/// - List: Display uniform_list of shards with relaunch and destroy buttons
pub fn render_shard_list(state: &AppState, cx: &mut Context<MainView>) -> impl IntoElement {
    if let Some(ref error_msg) = state.load_error {
        // Error state - show error message
        div()
            .flex()
            .flex_1()
            .justify_center()
            .items_center()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_color(rgb(0xff6b6b))
                    .child("Error loading shards"),
            )
            .child(
                div()
                    .text_color(rgb(0x888888))
                    .text_sm()
                    .child(error_msg.clone()),
            )
    } else if state.displays.is_empty() {
        // Empty state - no shards exist
        div()
            .flex()
            .flex_1()
            .justify_center()
            .items_center()
            .text_color(rgb(0x888888))
            .child("No active shards")
    } else {
        // List state - show shards with action buttons
        let item_count = state.displays.len();
        let displays = state.displays.clone();
        let relaunch_error = state.relaunch_error.clone();

        div().flex_1().child(
            uniform_list(
                "shard-list",
                item_count,
                cx.processor(move |_view, range: std::ops::Range<usize>, _window, cx| {
                    range
                        .map(|ix| {
                            let display = &displays[ix];
                            let branch = display.session.branch.clone();
                            let status_color = match display.status {
                                ProcessStatus::Running => rgb(0x00ff00), // Green
                                ProcessStatus::Stopped => rgb(0xff0000), // Red
                                ProcessStatus::Unknown => rgb(0xffa500), // Orange
                            };

                            // Check if this row has a relaunch error
                            let row_error = relaunch_error
                                .as_ref()
                                .filter(|(b, _)| b == &branch)
                                .map(|(_, e)| e.clone());

                            // Only show relaunch button when not running
                            let show_relaunch = display.status != ProcessStatus::Running;

                            // Clone branch for button closures
                            let branch_for_relaunch = branch.clone();
                            let branch_for_destroy = branch.clone();

                            div()
                                .id(ix)
                                .w_full()
                                .flex()
                                .flex_col()
                                // Main row
                                .child(
                                    div()
                                        .px_4()
                                        .py_2()
                                        .flex()
                                        .items_center()
                                        .gap_3()
                                        .child(div().text_color(status_color).child("●"))
                                        .child(
                                            div()
                                                .flex_1()
                                                .text_color(rgb(0xffffff))
                                                .child(branch.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_color(rgb(0x888888))
                                                .child(display.session.agent.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_color(rgb(0x666666))
                                                .child(display.session.project_id.clone()),
                                        )
                                        // Relaunch button [▶] - only shown when not running
                                        .when(show_relaunch, |row| {
                                            row.child(
                                                div()
                                                    .id(("relaunch-btn", ix))
                                                    .px_2()
                                                    .py_1()
                                                    .bg(rgb(0x444444))
                                                    .hover(|style| style.bg(rgb(0x555555)))
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .on_mouse_up(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(move |view, _, _, cx| {
                                                            view.on_relaunch_click(
                                                                &branch_for_relaunch,
                                                                cx,
                                                            );
                                                        }),
                                                    )
                                                    .child(
                                                        div().text_color(rgb(0xffffff)).child("▶"),
                                                    ),
                                            )
                                        })
                                        // Destroy button [×]
                                        .child(
                                            div()
                                                .id(("destroy-btn", ix))
                                                .px_2()
                                                .py_1()
                                                .bg(rgb(0x662222))
                                                .hover(|style| style.bg(rgb(0x883333)))
                                                .rounded_md()
                                                .cursor_pointer()
                                                .on_mouse_up(
                                                    gpui::MouseButton::Left,
                                                    cx.listener(move |view, _, _, cx| {
                                                        view.on_destroy_click(
                                                            &branch_for_destroy,
                                                            cx,
                                                        );
                                                    }),
                                                )
                                                .child(div().text_color(rgb(0xffffff)).child("×")),
                                        ),
                                )
                                // Error message (if relaunch failed for this row)
                                .when_some(row_error, |this, error| {
                                    this.child(div().px_4().pb_2().child(
                                        div().text_sm().text_color(rgb(0xff6b6b)).child(error),
                                    ))
                                })
                        })
                        .collect()
                }),
            )
            .h_full(),
        )
    }
}
