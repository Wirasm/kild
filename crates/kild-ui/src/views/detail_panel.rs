// Phase 2.7 will repurpose this as the Detail view. Suppressing dead_code until then.
#![allow(dead_code)]
//! Detail panel component for displaying selected kild information.
//!
//! Renders a 320px wide panel on the right side showing comprehensive
//! details about the selected kild including note, status, git info, and actions.

use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, prelude::*, px};

use gpui_component::button::{Button, ButtonVariants};

use crate::components::{Status, StatusIndicator};
use crate::state::AppState;
use crate::theme;
use crate::views::main_view::MainView;
use kild_core::{GitStatus, ProcessStatus};

/// Width of the detail panel in pixels (from mockup).
pub const DETAIL_PANEL_WIDTH: f32 = 320.0;

/// Render the detail panel for the selected kild.
///
/// Returns an empty element if no kild is selected.
pub fn render_detail_panel(state: &AppState, cx: &mut Context<MainView>) -> AnyElement {
    let Some(kild) = state.selected_kild() else {
        // Should not happen if called correctly, but handle gracefully
        return div().into_any_element();
    };

    let session = &kild.session;
    let branch = session.branch.clone();
    let agent = if session.agent_count() > 1 {
        session
            .agents()
            .iter()
            .map(|a| a.agent())
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        session.agent.clone()
    };
    let note = session.note.clone();
    let worktree_path = session.worktree_path.display().to_string();
    let created_at = session.created_at.clone();

    // Map process status to display values
    let (status, status_text, status_color) = match kild.process_status {
        ProcessStatus::Running => (Status::Active, "Running", theme::aurora()),
        ProcessStatus::Stopped => (Status::Stopped, "Stopped", theme::copper()),
        ProcessStatus::Unknown => (Status::Crashed, "Unknown", theme::ember()),
    };

    // Git status info
    let (git_status_text, git_status_color) = match kild.git_status {
        GitStatus::Clean => ("Clean", theme::aurora()),
        GitStatus::Dirty => ("Uncommitted", theme::copper()),
        GitStatus::Unknown => ("Unknown", theme::text_muted()),
    };
    let diff_stats_display = kild.uncommitted_diff.as_ref().map(|s| {
        format!(
            "+{} -{} ({} files)",
            s.insertions, s.deletions, s.files_changed
        )
    });

    // Variables for action handlers (clone before moving into closures)
    let worktree_path_for_copy = session.worktree_path.clone();
    let worktree_path_for_editor = session.worktree_path.clone();
    let branch_for_editor = branch.clone();
    let branch_for_focus = branch.clone();
    let terminal_type_for_focus = session
        .latest_agent()
        .and_then(|a| a.terminal_type().cloned());
    let window_id_for_focus = session
        .latest_agent()
        .and_then(|a| a.terminal_window_id().map(|s| s.to_string()));
    let branch_for_action = branch.clone();
    let branch_for_destroy = branch.clone();
    let is_running = kild.process_status == ProcessStatus::Running;

    div()
        .w(px(DETAIL_PANEL_WIDTH))
        .h_full()
        .bg(theme::obsidian())
        .border_l_1()
        .border_color(theme::border_subtle())
        .flex()
        .flex_col()
        // Header
        .child(
            div()
                .px(px(theme::SPACE_4))
                .py(px(theme::SPACE_4))
                .border_b_1()
                .border_color(theme::border_subtle())
                .child(
                    div()
                        .text_size(px(theme::TEXT_MD))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme::text_bright())
                        .child(branch.clone()),
                )
                .child(
                    div()
                        .mt(px(theme::SPACE_1))
                        .child(StatusIndicator::badge(status)),
                ),
        )
        // Content
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .px(px(theme::SPACE_4))
                .py(px(theme::SPACE_4))
                // Note section (if present)
                .when_some(note, |this, note_text| {
                    this.child(render_section(
                        "Note",
                        div()
                            .px(px(theme::SPACE_3))
                            .py(px(theme::SPACE_3))
                            .bg(theme::surface())
                            .rounded(px(theme::RADIUS_MD))
                            .text_size(px(theme::TEXT_SM))
                            .text_color(theme::text())
                            .child(note_text),
                    ))
                })
                // Details section
                .child(render_section(
                    "Details",
                    div()
                        .flex()
                        .flex_col()
                        .child(render_detail_row("Agent", &agent, theme::text()))
                        .child(render_detail_row("Status", status_text, status_color))
                        .child(render_detail_row("Created", &created_at, theme::text()))
                        .child(render_detail_row("Branch", &session.id, theme::text())),
                ))
                // Git Status section
                .child(render_section(
                    "Git Status",
                    div()
                        .flex()
                        .flex_col()
                        .child(render_detail_row(
                            "Changes",
                            git_status_text,
                            git_status_color,
                        ))
                        .when_some(diff_stats_display, |this, stats| {
                            this.child(render_detail_row("Files", &stats, theme::text()))
                        }),
                ))
                // Path section
                .child(render_section(
                    "Path",
                    div()
                        .px(px(theme::SPACE_2))
                        .py(px(theme::SPACE_2))
                        .bg(theme::surface())
                        .rounded(px(theme::RADIUS_MD))
                        .text_size(px(theme::TEXT_XS))
                        .text_color(theme::text_subtle())
                        .child(worktree_path),
                )),
        )
        // Actions footer
        .child(
            div()
                .px(px(theme::SPACE_4))
                .py(px(theme::SPACE_4))
                .border_t_1()
                .border_color(theme::border_subtle())
                .flex()
                .flex_col()
                .gap(px(theme::SPACE_2))
                // Row 1: Copy Path, Open Editor
                .child(
                    div()
                        .flex()
                        .gap(px(theme::SPACE_2))
                        .child(Button::new("detail-copy-path").label("Copy Path").on_click(
                            cx.listener(move |view, _, _, cx| {
                                view.on_copy_path_click(&worktree_path_for_copy, cx);
                            }),
                        ))
                        .child({
                            let wt = worktree_path_for_editor.clone();
                            let br = branch_for_editor.clone();
                            Button::new("detail-open-editor")
                                .label("Open Editor")
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.on_open_editor_click(&wt, &br, cx);
                                }))
                        }),
                )
                // Row 2: Focus Terminal, Open/Stop
                .child(
                    div()
                        .flex()
                        .gap(px(theme::SPACE_2))
                        .when(is_running, |row| {
                            let tt = terminal_type_for_focus.clone();
                            let wid = window_id_for_focus.clone();
                            let br = branch_for_focus.clone();
                            row.child(
                                Button::new("detail-focus-terminal")
                                    .label("Focus")
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.on_focus_terminal_click(
                                            tt.as_ref(),
                                            wid.as_deref(),
                                            &br,
                                            cx,
                                        );
                                    })),
                            )
                        })
                        .when(is_running, |row| {
                            let br = branch_for_action.clone();
                            row.child(Button::new("detail-stop").label("Stop").warning().on_click(
                                cx.listener(move |view, _, _, cx| {
                                    view.on_stop_click(&br, cx);
                                }),
                            ))
                        })
                        .when(!is_running, |row| {
                            let br = branch_for_action.clone();
                            row.child(Button::new("detail-open").label("Open").success().on_click(
                                cx.listener(move |view, _, _, cx| {
                                    view.on_open_click(&br, cx);
                                }),
                            ))
                        }),
                )
                // Row 3: Destroy
                .child(
                    Button::new("detail-destroy")
                        .label("Destroy Kild")
                        .danger()
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.on_destroy_click(&branch_for_destroy, cx);
                        })),
                ),
        )
        .into_any_element()
}

/// Render a section with a title and content.
fn render_section(title: &str, content: impl IntoElement) -> impl IntoElement {
    div()
        .mb(px(theme::SPACE_5))
        .child(
            div()
                .text_size(px(theme::TEXT_XS))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme::text_muted())
                .mb(px(theme::SPACE_2))
                .child(title.to_uppercase()),
        )
        .child(content)
}

/// Render a detail row with label and value.
fn render_detail_row(label: &str, value: &str, value_color: gpui::Rgba) -> impl IntoElement {
    div()
        .flex()
        .justify_between()
        .py(px(theme::SPACE_2))
        .text_size(px(theme::TEXT_SM))
        .child(
            div()
                .text_color(theme::text_subtle())
                .child(label.to_string()),
        )
        .child(
            div()
                .text_color(value_color)
                .text_size(px(theme::TEXT_XS))
                .child(value.to_string()),
        )
}
