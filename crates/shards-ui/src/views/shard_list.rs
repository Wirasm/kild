//! Shard list view component.
//!
//! Renders the list of shards with status indicators and session info.

use gpui::{Context, IntoElement, div, prelude::*, rgb, uniform_list};

use crate::state::{AppState, ProcessStatus};
use crate::views::MainView;

/// Render the shard list based on current state.
///
/// Handles three states:
/// - Error: Display error message
/// - Empty: Display "No active shards" message
/// - List: Display uniform_list of shards
pub fn render_shard_list(state: &AppState, _cx: &mut Context<MainView>) -> impl IntoElement {
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
        // List state - show shards
        let item_count = state.displays.len();
        let displays = state.displays.clone();

        div().flex_1().child(
            uniform_list("shard-list", item_count, move |range, _window, _cx| {
                range
                    .map(|ix| {
                        let display = &displays[ix];
                        let status_color = match display.status {
                            ProcessStatus::Running => rgb(0x00ff00), // Green
                            ProcessStatus::Stopped => rgb(0xff0000), // Red
                            ProcessStatus::Unknown => rgb(0xffa500), // Orange
                        };

                        div()
                            .id(ix)
                            .w_full()
                            .px_4()
                            .py_2()
                            .flex()
                            .gap_3()
                            .child(div().text_color(status_color).child("●"))
                            .child(
                                div()
                                    .flex_1()
                                    .text_color(rgb(0xffffff))
                                    .child(display.session.branch.clone()),
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
                    })
                    .collect()
            })
            .h_full(),
        )
    }
}
