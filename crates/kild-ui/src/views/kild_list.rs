//! KILD list view component.
//!
//! Renders the list of kilds with status indicators, session info, and action buttons.

use chrono::{DateTime, Utc};
use gpui::{Context, IntoElement, div, prelude::*, px, uniform_list};

use gpui_component::Disableable;
use gpui_component::button::{Button, ButtonVariants};

use crate::components::{Status, StatusIndicator};
use crate::state::AppState;
use crate::theme;
use crate::views::MainView;
use kild_core::{GitStatus, ProcessStatus};

/// Format RFC3339 timestamp as relative time (e.g., "5m ago", "2h ago").
fn format_relative_time(timestamp: &str) -> String {
    let Ok(created) = DateTime::parse_from_rfc3339(timestamp) else {
        tracing::debug!(
            event = "ui.kild_list.timestamp_parse_failed",
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

/// Returns true if the welcome screen should be shown.
///
/// Welcome screen appears only when there are no projects AND no kilds.
/// This ensures CLI-created kilds are visible even without UI projects.
fn should_show_welcome(state: &AppState) -> bool {
    state.projects_is_empty() && state.sessions_is_empty()
}

/// Render the kild list based on current state.
///
/// Handles states:
/// - Error: Display error message
/// - No projects AND no kilds: Display welcome message with Add Project button
/// - Empty: Display "No active kilds" message for the current project
/// - List: Display uniform_list of kilds with Open/Stop and Destroy buttons
pub fn render_kild_list(state: &AppState, cx: &mut Context<MainView>) -> impl IntoElement {
    if let Some(error_msg) = state.load_error() {
        // Error state - show error message
        div()
            .flex()
            .flex_1()
            .justify_center()
            .items_center()
            .flex_col()
            .gap(px(theme::SPACE_2))
            .child(
                div()
                    .text_color(theme::ember())
                    .child("Error loading kilds"),
            )
            .child(
                div()
                    .text_color(theme::text_subtle())
                    .text_size(px(theme::TEXT_SM))
                    .child(error_msg.to_owned()),
            )
    } else if should_show_welcome(state) {
        // No projects AND no kilds - show welcome message
        div()
            .flex()
            .flex_1()
            .justify_center()
            .items_center()
            .flex_col()
            .gap(px(theme::SPACE_4))
            .child(
                div()
                    .text_size(px(theme::TEXT_XL))
                    .text_color(theme::text_white())
                    .child("Welcome to KILD!"),
            )
            .child(
                div()
                    .text_color(theme::text_subtle())
                    .child("Add a project to start creating kilds."),
            )
            .child(
                Button::new("empty-state-add-project")
                    .label("+ Add Project")
                    .primary()
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.on_add_project_click(window, cx);
                    })),
            )
    } else {
        // Filter displays by active project
        let filtered: Vec<_> = state.filtered_displays().into_iter().cloned().collect();

        if filtered.is_empty() {
            // Empty state - message depends on whether filtering is active
            let message = if state.active_project().is_some() {
                "No active kilds for this project"
            } else {
                "No active kilds"
            };
            div()
                .flex()
                .flex_1()
                .justify_center()
                .items_center()
                .text_color(theme::text_subtle())
                .child(message)
        } else {
            // List state - show kilds with action buttons
            let item_count = filtered.len();
            let displays = filtered;
            let errors = state.errors_clone();
            let selected_kild_id = state.selected_id().map(|s| s.to_string());

            // Pre-compute loading state for each branch to avoid repeated lookups in render loop
            let mut loading_branches = std::collections::HashSet::new();
            for display in &displays {
                if state.is_loading(&display.session.branch) {
                    loading_branches.insert(display.session.branch.clone());
                }
            }

            div().flex_1().h_full().child(
                uniform_list(
                    "kild-list",
                    item_count,
                    cx.processor(move |_view, range: std::ops::Range<usize>, _window, cx| {
                        range
                            .map(|ix| {
                                let display = &displays[ix];
                                let branch = display.session.branch.clone();

                                // Map ProcessStatus to Status for StatusIndicator
                                let status = match display.process_status {
                                    ProcessStatus::Running => Status::Active,
                                    ProcessStatus::Stopped => Status::Stopped,
                                    ProcessStatus::Unknown => Status::Crashed,
                                };

                                // Check if this row has any operation error
                                let row_error = errors.get(&branch).map(|e| e.message.clone());

                                // Show Open button when stopped, Stop button when running
                                let is_running = display.process_status == ProcessStatus::Running;
                                let is_loading = loading_branches.contains(&branch);

                                // Clone data for use in closures
                                let branch_for_open = branch.clone();
                                let branch_for_stop = branch.clone();
                                let branch_for_destroy = branch.clone();
                                let git_status = display.git_status;
                                let note = display.session.note.clone();

                                // Quick actions button clones
                                let worktree_path_for_copy = display.session.worktree_path.clone();
                                let worktree_path_for_edit = display.session.worktree_path.clone();
                                let branch_for_edit = branch.clone();
                                let terminal_type_for_focus = display
                                    .session
                                    .latest_agent()
                                    .and_then(|a| a.terminal_type().cloned());
                                let window_id_for_focus = display
                                    .session
                                    .latest_agent()
                                    .and_then(|a| a.terminal_window_id().map(|s| s.to_string()));
                                let branch_for_focus = branch.clone();

                                // Selection state
                                let session_id = display.session.id.clone();
                                let is_selected = selected_kild_id.as_ref() == Some(&session_id);
                                let session_id_for_click = session_id.clone();

                                div()
                                    .id(ix)
                                    .w_full()
                                    .flex()
                                    .flex_col()
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.on_kild_select(&session_id_for_click, cx);
                                    }))
                                    // Selected state styling (ice left border)
                                    .when(is_selected, |row| {
                                        row.border_l_2()
                                            .border_color(theme::ice())
                                            .bg(theme::surface())
                                    })
                                    // Main row
                                    .child(
                                        div()
                                            .px(px(theme::SPACE_4))
                                            .py(px(theme::SPACE_2))
                                            .flex()
                                            .items_center()
                                            .gap(px(theme::SPACE_3))
                                            // Status indicator (dot with optional glow)
                                            .child(StatusIndicator::dot(status))
                                            // Git diff stats (when dirty) or unknown indicator
                                            .when_some(display.diff_stats, |row, stats| {
                                                row.child(
                                                    div()
                                                        .flex()
                                                        .gap(px(theme::SPACE_1))
                                                        .text_size(px(theme::TEXT_SM))
                                                        .child(
                                                            div()
                                                                .text_color(theme::aurora())
                                                                .child(format!(
                                                                    "+{}",
                                                                    stats.insertions
                                                                )),
                                                        )
                                                        .child(
                                                            div().text_color(theme::ember()).child(
                                                                format!("-{}", stats.deletions),
                                                            ),
                                                        ),
                                                )
                                            })
                                            // Fallback: dirty but no stats (shouldn't happen often)
                                            .when(
                                                git_status == GitStatus::Dirty
                                                    && display.diff_stats.is_none(),
                                                |row| {
                                                    row.child(
                                                        div()
                                                            .text_color(theme::copper())
                                                            .child("●"),
                                                    )
                                                },
                                            )
                                            // Unknown git status
                                            .when(git_status == GitStatus::Unknown, |row| {
                                                row.child(
                                                    div()
                                                        .text_color(theme::text_muted())
                                                        .child("?"),
                                                )
                                            })
                                            // Branch name
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .text_color(theme::text_white())
                                                    .child(branch.clone()),
                                            )
                                            // Agent name
                                            .child(
                                                div()
                                                    .text_color(theme::kiri())
                                                    .child(display.session.agent.clone()),
                                            )
                                            // Project ID
                                            .child(
                                                div()
                                                    .text_color(theme::text_muted())
                                                    .child(display.session.project_id.clone()),
                                            )
                                            // Created at timestamp
                                            .child(
                                                div()
                                                    .text_color(theme::text_muted())
                                                    .text_size(px(theme::TEXT_SM))
                                                    .child(format_relative_time(
                                                        &display.session.created_at,
                                                    )),
                                            )
                                            // Last activity timestamp (if available)
                                            .when_some(
                                                display.session.last_activity.clone(),
                                                |row, activity| {
                                                    row.child(
                                                        div()
                                                            .text_color(theme::text_muted())
                                                            .text_size(px(theme::TEXT_SM))
                                                            .child(format_relative_time(&activity)),
                                                    )
                                                },
                                            )
                                            // Note column (truncated to 25 characters - uses char count, not bytes)
                                            .when_some(note, |row, note_text| {
                                                let display_text = if note_text.chars().count() > 25
                                                {
                                                    format!(
                                                        "{}...",
                                                        note_text
                                                            .chars()
                                                            .take(25)
                                                            .collect::<String>()
                                                    )
                                                } else {
                                                    note_text
                                                };
                                                row.child(
                                                    div()
                                                        .text_color(theme::text_subtle())
                                                        .text_size(px(theme::TEXT_SM))
                                                        .child(display_text),
                                                )
                                            })
                                            // Copy Path button [Copy] - Ghost variant
                                            .child(
                                                Button::new(("copy-btn", ix))
                                                    .label("Copy")
                                                    .ghost()
                                                    .on_click(cx.listener(
                                                        move |view, _, _, cx| {
                                                            view.on_copy_path_click(
                                                                &worktree_path_for_copy,
                                                                cx,
                                                            );
                                                        },
                                                    )),
                                            )
                                            // Open in Editor button [Edit] - Ghost variant
                                            .child({
                                                let wt = worktree_path_for_edit.clone();
                                                let br = branch_for_edit.clone();
                                                Button::new(("edit-btn", ix))
                                                    .label("Edit")
                                                    .ghost()
                                                    .on_click(cx.listener(move |view, _, _, cx| {
                                                        view.on_open_editor_click(&wt, &br, cx);
                                                    }))
                                            })
                                            // Focus Terminal button [Focus] - only show when running
                                            .when(is_running, |row| {
                                                let tt = terminal_type_for_focus.clone();
                                                let wid = window_id_for_focus.clone();
                                                let br = branch_for_focus.clone();
                                                row.child(
                                                    Button::new(("focus-btn", ix))
                                                        .label("Focus")
                                                        .on_click(cx.listener(
                                                            move |view, _, _, cx| {
                                                                view.on_focus_terminal_click(
                                                                    tt.as_ref(),
                                                                    wid.as_deref(),
                                                                    &br,
                                                                    cx,
                                                                );
                                                            },
                                                        )),
                                                )
                                            })
                                            // Open button [▶] - shown when NOT running - Success variant
                                            .when(!is_running, |row| {
                                                let br = branch_for_open.clone();
                                                let label = if is_loading { "..." } else { "▶" };
                                                row.child(
                                                    Button::new(("open-btn", ix))
                                                        .label(label)
                                                        .success()
                                                        .disabled(is_loading)
                                                        .on_click(cx.listener(
                                                            move |view, _, _, cx| {
                                                                view.on_open_click(&br, cx);
                                                            },
                                                        )),
                                                )
                                            })
                                            // Stop button [⏹] - shown when running - Warning variant
                                            .when(is_running, |row| {
                                                let br = branch_for_stop.clone();
                                                let label = if is_loading { "..." } else { "⏹" };
                                                row.child(
                                                    Button::new(("stop-btn", ix))
                                                        .label(label)
                                                        .warning()
                                                        .disabled(is_loading)
                                                        .on_click(cx.listener(
                                                            move |view, _, _, cx| {
                                                                view.on_stop_click(&br, cx);
                                                            },
                                                        )),
                                                )
                                            })
                                            // Destroy button [×] - Danger variant
                                            .child({
                                                let br = branch_for_destroy.clone();
                                                Button::new(("destroy-btn", ix))
                                                    .label("×")
                                                    .danger()
                                                    .disabled(is_loading)
                                                    .on_click(cx.listener(move |view, _, _, cx| {
                                                        view.on_destroy_click(&br, cx);
                                                    }))
                                            }),
                                    )
                                    // Error message (if open/stop failed for this row)
                                    .when_some(row_error, |this, error| {
                                        this.child(
                                            div()
                                                .px(px(theme::SPACE_4))
                                                .pb(px(theme::SPACE_2))
                                                .child(
                                                    div()
                                                        .text_size(px(theme::TEXT_SM))
                                                        .text_color(theme::ember())
                                                        .child(error),
                                                ),
                                        )
                                    })
                            })
                            .collect()
                    }),
                )
                .h_full(),
            )
        }
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

    #[test]
    fn test_should_show_welcome_when_no_projects_and_no_displays() {
        use crate::state::AppState;

        let state = AppState::test_new();

        assert!(should_show_welcome(&state));
    }

    #[test]
    fn test_should_not_show_welcome_when_displays_exist_without_projects() {
        use crate::state::AppState;
        use kild_core::sessions::types::{Session, SessionStatus};
        use kild_core::{GitStatus, ProcessStatus, SessionInfo};
        use std::path::PathBuf;

        // Create a kild display (simulating CLI-created kild)
        let session = Session::new(
            "test-id".to_string(),
            "test-project".to_string(),
            "test-branch".to_string(),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            None,
            None,
            vec![],
            None,
            None,
        );

        let state = AppState::test_with_displays(vec![SessionInfo {
            session,
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            diff_stats: None,
        }]);

        assert!(!should_show_welcome(&state));
    }
}
