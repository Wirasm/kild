//! Shard list view component.
//!
//! Renders the list of shards with status indicators, session info, and action buttons.

use chrono::{DateTime, Utc};
use gpui::{Context, IntoElement, div, prelude::*, rgb, uniform_list};

use crate::state::{AppState, ProcessStatus};
use crate::views::MainView;

/// Format RFC3339 timestamp as relative time (e.g., "5m ago", "2h ago").
fn format_relative_time(timestamp: &str) -> String {
    let Ok(created) = DateTime::parse_from_rfc3339(timestamp) else {
        tracing::debug!(
            event = "ui.shard_list.timestamp_parse_failed",
            timestamp = timestamp,
            "Failed to parse timestamp - displaying raw value"
        );
        return timestamp.to_string();
    };

    let now = Utc::now();
    let duration = now.signed_duration_since(created.with_timezone(&Utc));

    let minutes = duration.num_minutes();
    let hours = duration.num_hours();
    let days = duration.num_days();

    if days > 0 {
        format!("{}d ago", days)
    } else if hours > 0 {
        format!("{}h ago", hours)
    } else if minutes > 0 {
        format!("{}m ago", minutes)
    } else {
        "just now".to_string()
    }
}

/// Render the shard list based on current state.
///
/// Handles three states:
/// - Error: Display error message
/// - Empty: Display "No active shards" message
/// - List: Display uniform_list of shards with Open/Stop and Destroy buttons
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
        let open_error = state.open_error.clone();
        let stop_error = state.stop_error.clone();

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
                                ProcessStatus::Unknown => rgb(0x888888), // Gray
                            };

                            // Check if this row has an open or stop error
                            let has_matching_error =
                                |err: &crate::state::OperationError| err.branch == branch;
                            let row_error = open_error
                                .as_ref()
                                .filter(|e| has_matching_error(e))
                                .or_else(|| stop_error.as_ref().filter(|e| has_matching_error(e)))
                                .map(|e| e.message.clone());

                            // Show Open button when stopped, Stop button when running
                            let is_running = display.status == ProcessStatus::Running;

                            // Clone branch for button closures
                            let branch_for_open = branch.clone();
                            let branch_for_stop = branch.clone();
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
                                        // Created at timestamp
                                        .child(div().text_color(rgb(0x555555)).text_sm().child(
                                            format_relative_time(&display.session.created_at),
                                        ))
                                        // Last activity timestamp (if available)
                                        .when_some(
                                            display.session.last_activity.clone(),
                                            |row, activity| {
                                                row.child(
                                                    div()
                                                        .text_color(rgb(0x666666))
                                                        .text_sm()
                                                        .child(format_relative_time(&activity)),
                                                )
                                            },
                                        )
                                        // Open button [▶] - shown when NOT running
                                        .when(!is_running, |row| {
                                            row.child(
                                                div()
                                                    .id(("open-btn", ix))
                                                    .px_2()
                                                    .py_1()
                                                    .bg(rgb(0x444444))
                                                    .hover(|style| style.bg(rgb(0x555555)))
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .on_mouse_up(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(move |view, _, _, cx| {
                                                            view.on_open_click(
                                                                &branch_for_open,
                                                                cx,
                                                            );
                                                        }),
                                                    )
                                                    .child(
                                                        div().text_color(rgb(0xffffff)).child("▶"),
                                                    ),
                                            )
                                        })
                                        // Stop button [⏹] - shown when running
                                        .when(is_running, |row| {
                                            row.child(
                                                div()
                                                    .id(("stop-btn", ix))
                                                    .px_2()
                                                    .py_1()
                                                    .bg(rgb(0x444488))
                                                    .hover(|style| style.bg(rgb(0x555599)))
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .on_mouse_up(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(move |view, _, _, cx| {
                                                            view.on_stop_click(
                                                                &branch_for_stop,
                                                                cx,
                                                            );
                                                        }),
                                                    )
                                                    .child(
                                                        div().text_color(rgb(0xffffff)).child("⏹"),
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
                                // Error message (if open/stop failed for this row)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_relative_time_invalid_timestamp() {
        assert_eq!(format_relative_time("not-a-timestamp"), "not-a-timestamp");
    }

    #[test]
    fn test_format_relative_time_just_now() {
        let now = Utc::now().to_rfc3339();
        assert_eq!(format_relative_time(&now), "just now");
    }

    #[test]
    fn test_format_relative_time_minutes_ago() {
        use chrono::Duration;
        let five_min_ago = (Utc::now() - Duration::minutes(5)).to_rfc3339();
        assert_eq!(format_relative_time(&five_min_ago), "5m ago");
    }

    #[test]
    fn test_format_relative_time_hours_ago() {
        use chrono::Duration;
        let two_hours_ago = (Utc::now() - Duration::hours(2)).to_rfc3339();
        assert_eq!(format_relative_time(&two_hours_ago), "2h ago");
    }

    #[test]
    fn test_format_relative_time_days_ago() {
        use chrono::Duration;
        let three_days_ago = (Utc::now() - Duration::days(3)).to_rfc3339();
        assert_eq!(format_relative_time(&three_days_ago), "3d ago");
    }
}
