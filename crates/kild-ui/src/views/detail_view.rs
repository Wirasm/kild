//! Detail view component for displaying selected kild information.
//!
//! Replaces the 320px detail panel with a sidebar-width (220px) view
//! that shows session details when a kild is selected. Includes a
//! "Back to list" button at the top for navigation.

// Allow dead_code — detail view helpers are wired up when sidebar detail mode is connected.
#![allow(dead_code)]

use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, prelude::*, px};

use gpui_component::button::{Button, ButtonVariants};

use crate::state::AppState;
use crate::theme;
use crate::views::main_view::MainView;
use kild_core::state::types::RuntimeMode;

/// Render the detail view for the selected kild.
///
/// Occupies the full sidebar width (220px) when toggled. Shows note,
/// session info, git stats, and worktree path with a "Back to list" button.
///
/// Returns an empty element if no kild is selected.
pub fn render_detail_view(state: &AppState, cx: &mut Context<MainView>) -> AnyElement {
    let Some(kild) = state.selected_kild() else {
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
    let runtime_mode_text = match session.runtime_mode {
        Some(RuntimeMode::Daemon) => "Daemon",
        Some(RuntimeMode::Terminal) => "Terminal",
        None => "Unknown",
    };

    // Git diff stats
    let diff_stats_display = kild.uncommitted_diff.as_ref().map(|s| {
        format!(
            "+{} -{} ({} files)",
            s.insertions, s.deletions, s.files_changed
        )
    });

    div()
        .h_full()
        .bg(theme::obsidian())
        .flex()
        .flex_col()
        .overflow_hidden()
        // Back button
        .child(
            div()
                .px(px(theme::SPACE_3))
                .py(px(theme::SPACE_2))
                .border_b_1()
                .border_color(theme::border_subtle())
                .child(
                    Button::new("detail-back")
                        .label("Back to list")
                        .ghost()
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.on_clear_selection(cx);
                        })),
                ),
        )
        // Header: branch name
        .child(
            div()
                .px(px(theme::SPACE_3))
                .py(px(theme::SPACE_3))
                .border_b_1()
                .border_color(theme::border_subtle())
                .child(
                    div()
                        .text_size(px(theme::TEXT_MD))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme::text_bright())
                        .overflow_hidden()
                        .child(branch),
                ),
        )
        // Scrollable content
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .px(px(theme::SPACE_3))
                .py(px(theme::SPACE_3))
                // Note section (if present)
                .when_some(note, |this, note_text| {
                    this.child(render_section(
                        "Note",
                        div()
                            .px(px(theme::SPACE_2))
                            .py(px(theme::SPACE_2))
                            .bg(theme::surface())
                            .rounded(px(theme::RADIUS_MD))
                            .text_size(px(theme::TEXT_SM))
                            .text_color(theme::text())
                            .child(note_text),
                    ))
                })
                // Session info section
                .child(render_section(
                    "Session",
                    div()
                        .flex()
                        .flex_col()
                        .child(render_detail_row("Agent", &agent, theme::text()))
                        .child(render_detail_row("Created", &created_at, theme::text()))
                        .child(render_detail_row("Mode", runtime_mode_text, theme::text())),
                ))
                // Git stats section
                .when_some(diff_stats_display, |this, stats| {
                    this.child(render_section(
                        "Git",
                        div().flex().flex_col().child(render_detail_row(
                            "Changes",
                            &stats,
                            theme::text(),
                        )),
                    ))
                })
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
        .into_any_element()
}

/// Render a section with a title and content.
fn render_section(title: &str, content: impl IntoElement) -> impl IntoElement {
    div()
        .mb(px(theme::SPACE_4))
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
        .py(px(theme::SPACE_1))
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
