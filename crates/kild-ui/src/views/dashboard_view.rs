//! Dashboard view component for fleet overview.
//!
//! Renders fleet summary bar and responsive grid of kild cards.

use gpui::{
    AnyElement, Context, IntoElement, ParentElement, SharedString, Styled, div, prelude::*, px,
};

use crate::components::{Status, StatusIndicator};
use crate::state::AppState;
use crate::theme;
use crate::views::helpers::format_relative_time;
use crate::views::main_view::MainView;
use crate::views::terminal_tabs::TerminalTabs;
use kild_core::ProcessStatus;

/// Max note length before truncating on cards (prevents card width overflow).
const MAX_NOTE_LENGTH: usize = 50;

/// Render the dashboard view with fleet summary and kild card grid.
pub fn render_dashboard(
    state: &AppState,
    terminal_tabs: &std::collections::HashMap<String, TerminalTabs>,
    team_manager: &crate::teams::TeamManager,
    cx: &mut Context<MainView>,
) -> AnyElement {
    let displays = state.filtered_displays();

    if displays.is_empty() {
        return div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme::text_subtle())
            .text_size(px(theme::TEXT_SM))
            .child("No kilds")
            .into_any_element();
    }

    let running_count = displays
        .iter()
        .filter(|d| d.process_status == ProcessStatus::Running)
        .count();
    let stopped_count = displays
        .iter()
        .filter(|d| d.process_status == ProcessStatus::Stopped)
        .count();
    let total_terminals: usize = displays
        .iter()
        .map(|d| {
            terminal_tabs
                .get(&d.session.id)
                .map(|t| t.len())
                .unwrap_or(0)
        })
        .sum();

    div()
        .id("dashboard-scroll")
        .flex_1()
        .flex()
        .flex_col()
        .overflow_y_scroll()
        .px(px(theme::SPACE_4))
        .py(px(theme::SPACE_4))
        // Fleet summary bar
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(theme::SPACE_4))
                .px(px(theme::SPACE_3))
                .py(px(theme::SPACE_2))
                .bg(theme::surface())
                .rounded(px(theme::RADIUS_MD))
                .mb(px(theme::SPACE_4))
                .text_size(px(theme::TEXT_SM))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(theme::SPACE_1))
                        .child(StatusIndicator::dot(Status::Active))
                        .child(
                            div()
                                .text_color(theme::text())
                                .child(format!("{} active", running_count)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(theme::SPACE_1))
                        .child(StatusIndicator::dot(Status::Stopped))
                        .child(
                            div()
                                .text_color(theme::text())
                                .child(format!("{} stopped", stopped_count)),
                        ),
                )
                .child(
                    div()
                        .text_color(theme::text_muted())
                        .child(format!("{} terminals", total_terminals)),
                ),
        )
        // Card grid
        .child({
            let mut cards = Vec::new();
            for (ix, display) in displays.iter().enumerate() {
                let teammate_count = team_manager
                    .teammates_for_session(&display.session.id)
                    .len();
                cards.push(render_card(display, ix, terminal_tabs, teammate_count, cx));
            }
            div()
                .flex()
                .flex_wrap()
                .gap(px(theme::SPACE_3))
                .children(cards)
        })
        .into_any_element()
}

/// Render a single kild card for the dashboard grid.
fn render_card(
    display: &kild_core::SessionInfo,
    ix: usize,
    terminal_tabs: &std::collections::HashMap<String, TerminalTabs>,
    teammate_count: usize,
    cx: &mut Context<MainView>,
) -> AnyElement {
    let session = &display.session;
    let is_stopped = display.process_status == ProcessStatus::Stopped;

    let status = match display.process_status {
        ProcessStatus::Running => Status::Active,
        ProcessStatus::Stopped => Status::Stopped,
        ProcessStatus::Unknown => Status::Crashed,
    };

    let branch = session.branch.clone();
    let agent = session.agent.clone();
    let note = session.note.clone();
    let created_at = session.created_at.clone();
    let terminal_count = terminal_tabs.get(&session.id).map(|t| t.len()).unwrap_or(0);
    let session_id = session.id.clone();

    div()
        .id(SharedString::from(format!("dashboard-card-{}", ix)))
        .min_w(px(260.0))
        .flex_1()
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border_subtle())
        .rounded(px(theme::RADIUS_LG))
        .px(px(theme::SPACE_3))
        .py(px(theme::SPACE_3))
        .cursor_pointer()
        .hover(|d| d.bg(theme::elevated()).border_color(theme::border()))
        .when(is_stopped, |d| d.opacity(0.65))
        .on_mouse_up(
            gpui::MouseButton::Left,
            cx.listener(move |view, _, _, cx| {
                view.on_dashboard_card_click(&session_id, cx);
            }),
        )
        .flex()
        .flex_col()
        .gap(px(theme::SPACE_2))
        // Row 1: status dot + branch + agent
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(theme::SPACE_2))
                .child(StatusIndicator::dot(status))
                .child(
                    div()
                        .flex_1()
                        .text_color(theme::text_white())
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_size(px(theme::TEXT_SM))
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(branch),
                )
                .child(
                    div()
                        .text_color(theme::kiri())
                        .text_size(px(theme::TEXT_XS))
                        .child(agent),
                ),
        )
        .when_some(note, |card, note_text| {
            let display_text = if note_text.chars().count() > MAX_NOTE_LENGTH {
                format!(
                    "{}...",
                    note_text.chars().take(MAX_NOTE_LENGTH).collect::<String>()
                )
            } else {
                note_text
            };
            card.child(
                div()
                    .text_color(theme::text_muted())
                    .text_size(px(theme::TEXT_XS))
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(display_text),
            )
        })
        // Row 3: git stats + files
        .when_some(display.uncommitted_diff, |card, stats| {
            card.child(
                div()
                    .flex()
                    .gap(px(theme::SPACE_1))
                    .text_size(px(theme::TEXT_SM))
                    .child(
                        div()
                            .text_color(theme::aurora())
                            .child(format!("+{}", stats.insertions)),
                    )
                    .child(
                        div()
                            .text_color(theme::ember())
                            .child(format!("-{}", stats.deletions)),
                    )
                    .child(
                        div()
                            .text_color(theme::text_muted())
                            .child(format!("{}f", stats.files_changed)),
                    ),
            )
        })
        // Row 4: time + terminal count
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(theme::TEXT_XS))
                .child(
                    div()
                        .text_color(theme::text_muted())
                        .child(format_relative_time(&created_at)),
                )
                .when(teammate_count > 0, |row| {
                    row.child(div().text_color(theme::kiri()).child(format!(
                        "{} agent{}",
                        teammate_count,
                        if teammate_count == 1 { "" } else { "s" }
                    )))
                })
                .when(terminal_count > 0, |row| {
                    row.child(
                        div()
                            .text_color(theme::text_muted())
                            .child(format!("{} term", terminal_count)),
                    )
                }),
        )
        .into_any_element()
}
