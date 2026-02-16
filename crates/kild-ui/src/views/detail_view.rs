//! Detail view component for kild drill-down.
//!
//! Renders comprehensive kild information from a dashboard card click:
//! hero section, note, session info, git stats, terminals, path, and actions.

use gpui::{
    AnyElement, Context, IntoElement, ParentElement, SharedString, Styled, div, prelude::*, px,
};

use gpui_component::button::{Button, ButtonVariants};

use crate::components::{Status, StatusIndicator};
use crate::state::AppState;
use crate::theme;
use crate::views::helpers::format_relative_time;
use crate::views::main_view::MainView;
use crate::views::terminal_tabs::{TerminalBackend, TerminalTabs};
use kild_core::{GitStatus, ProcessStatus};

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

/// Render the detail drill-down view for the selected kild.
///
/// Returns an empty element if no kild is selected.
pub fn render_detail_view(
    state: &AppState,
    terminal_tabs: &std::collections::HashMap<String, TerminalTabs>,
    cx: &mut Context<MainView>,
) -> AnyElement {
    let Some(kild) = state.selected_kild() else {
        return div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme::text_subtle())
            .text_size(px(theme::TEXT_SM))
            .child("No kild selected")
            .into_any_element();
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
    let session_id = session.id.to_string();

    let status = match kild.process_status {
        ProcessStatus::Running => Status::Active,
        ProcessStatus::Stopped => Status::Stopped,
        ProcessStatus::Unknown => Status::Crashed,
    };

    let (git_status_text, git_status_color) = match kild.git_status {
        GitStatus::Clean => ("Clean", theme::aurora()),
        GitStatus::Dirty => ("Uncommitted", theme::copper()),
        GitStatus::Unknown => ("Unknown", theme::text_muted()),
    };

    let runtime_text = session
        .runtime_mode
        .as_ref()
        .map(|m| format!("{:?}", m).to_lowercase())
        .unwrap_or_else(|| "terminal".to_string());

    let worktree_path_for_copy = session.worktree_path.clone();
    let worktree_path_for_editor = session.worktree_path.clone();
    let branch_for_editor = branch.clone();
    let branch_for_action = branch.clone();
    let branch_for_destroy = branch.clone();
    let is_running = kild.process_status == ProcessStatus::Running;

    // Terminal list for this kild
    let tabs = terminal_tabs.get(&session_id);

    div()
        .id("detail-scroll")
        .flex_1()
        .flex()
        .flex_col()
        .overflow_y_scroll()
        // Back button
        .child(
            div()
                .id("detail-back")
                .flex()
                .items_center()
                .gap(px(theme::SPACE_1))
                .px(px(theme::SPACE_4))
                .py(px(theme::SPACE_2))
                .border_b_1()
                .border_color(theme::border_subtle())
                .text_size(px(theme::TEXT_XS))
                .text_color(theme::text_muted())
                .cursor_pointer()
                .hover(|d| d.text_color(theme::text_subtle()))
                .on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|view, _, _, cx| {
                        view.on_detail_back(cx);
                    }),
                )
                .child("\u{2190} Dashboard"),
        )
        // Hero section
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
                        .text_color(theme::text_white())
                        .mb(px(theme::SPACE_1))
                        .child(branch.to_string()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(theme::SPACE_2))
                        .child(StatusIndicator::badge(status))
                        .child(
                            div()
                                .text_size(px(theme::TEXT_XS))
                                .text_color(theme::text_muted())
                                .child(format_relative_time(&created_at)),
                        ),
                ),
        )
        // Body
        .child(
            div()
                .px(px(theme::SPACE_4))
                .py(px(theme::SPACE_4))
                // Note section
                .when_some(note, |this, note_text| {
                    this.child(render_section(
                        "Note",
                        div()
                            .px(px(theme::SPACE_2))
                            .py(px(theme::SPACE_2))
                            .bg(theme::surface())
                            .rounded(px(theme::RADIUS_SM))
                            .text_size(px(theme::TEXT_XS))
                            .text_color(theme::text())
                            .child(note_text),
                    ))
                })
                // Session section
                .child(render_section(
                    "Session",
                    div()
                        .flex()
                        .flex_col()
                        .child(render_detail_row("Agent", &agent, theme::text()))
                        .child(render_detail_row("Created", &created_at, theme::text()))
                        .child(render_detail_row(
                            "Branch",
                            &format!("kild/{}", branch),
                            theme::text(),
                        ))
                        .child(render_detail_row("Runtime", &runtime_text, theme::text())),
                ))
                // Git section
                .child(render_section(
                    "Git",
                    div()
                        .flex()
                        .flex_col()
                        .when_some(kild.uncommitted_diff, |this, stats| {
                            this.child(
                                div()
                                    .flex()
                                    .gap(px(theme::SPACE_3))
                                    .text_size(px(theme::TEXT_XS))
                                    .mb(px(theme::SPACE_1))
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
                                            .child(format!("{} files", stats.files_changed)),
                                    ),
                            )
                        })
                        .child(render_detail_row(
                            "Status",
                            git_status_text,
                            git_status_color,
                        )),
                ))
                // Terminals section
                .child(render_section(
                    "Terminals",
                    render_terminal_list(&session_id, tabs, cx),
                ))
                // Path section
                .child(render_section(
                    "Path",
                    div()
                        .px(px(theme::SPACE_2))
                        .py(px(theme::SPACE_2))
                        .bg(theme::surface())
                        .rounded(px(theme::RADIUS_SM))
                        .text_size(px(theme::TEXT_XS))
                        .text_color(theme::text_muted())
                        .child(worktree_path),
                ))
                // Actions footer
                .child(
                    div()
                        .flex()
                        .gap(px(theme::SPACE_2))
                        .mt(px(theme::SPACE_4))
                        .pt(px(theme::SPACE_4))
                        .border_t_1()
                        .border_color(theme::border_subtle())
                        .child({
                            let wt = worktree_path_for_editor.clone();
                            let br = branch_for_editor.clone();
                            Button::new("detail-editor")
                                .label("Open in editor")
                                .ghost()
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.on_open_editor_click(&wt, &br, cx);
                                }))
                        })
                        .child(
                            Button::new("detail-copy")
                                .label("Copy path")
                                .ghost()
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.on_copy_path_click(&worktree_path_for_copy, cx);
                                })),
                        )
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
                        })
                        .child(
                            Button::new("detail-destroy")
                                .label("Destroy")
                                .danger()
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.on_destroy_click(&branch_for_destroy, cx);
                                })),
                        ),
                ),
        )
        .into_any_element()
}

/// Render the terminal list for a kild in the detail view.
fn render_terminal_list(
    session_id: &str,
    tabs: Option<&TerminalTabs>,
    cx: &mut Context<MainView>,
) -> impl IntoElement {
    let Some(tabs) = tabs else {
        return div()
            .text_size(px(theme::TEXT_XS))
            .text_color(theme::text_muted())
            .child("No terminals open")
            .into_any_element();
    };

    if tabs.is_empty() {
        return div()
            .text_size(px(theme::TEXT_XS))
            .text_color(theme::text_muted())
            .child("No terminals open")
            .into_any_element();
    }

    let mut rows = Vec::new();
    for i in 0..tabs.len() {
        let Some(entry) = tabs.get(i) else {
            continue;
        };
        let label = entry.label().to_string();
        let mode = match entry.backend() {
            TerminalBackend::Daemon { .. } => "daemon",
            TerminalBackend::Local => "local",
            TerminalBackend::Teammate { .. } => "team",
        };
        let sid = session_id.to_string();
        let tab_idx = i;

        rows.push(
            div()
                .id(SharedString::from(format!("detail-term-{}", i)))
                .flex()
                .items_center()
                .gap(px(theme::SPACE_2))
                .px(px(theme::SPACE_2))
                .py(px(theme::SPACE_2))
                .bg(theme::surface())
                .rounded(px(theme::RADIUS_SM))
                .cursor_pointer()
                .hover(|d| d.bg(theme::elevated()))
                .on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(move |view, _, window, cx| {
                        view.on_detail_terminal_click(&sid, tab_idx, window, cx);
                    }),
                )
                .child(div().size(px(5.0)).rounded_full().bg(theme::aurora()))
                // Terminal name
                .child(
                    div()
                        .flex_1()
                        .text_size(px(theme::TEXT_XS))
                        .text_color(theme::text())
                        .child(label),
                )
                // Mode indicator
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(theme::text_muted())
                        .child(mode.to_string()),
                )
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(theme::ice())
                        .child("open \u{2192}"),
                )
                .into_any_element(),
        );
    }

    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .children(rows)
        .into_any_element()
}
