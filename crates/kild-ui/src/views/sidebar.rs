//! Kild navigation sidebar.
//!
//! Fixed left sidebar (200px) showing kilds grouped by Active/Stopped status
//! with nested terminal tab names. Hover actions appear on kild rows.

use gpui::{Context, FontWeight, IntoElement, ParentElement, Styled, div, prelude::*, px};
use std::collections::HashMap;

use crate::components::{Status, StatusIndicator};
use crate::state::AppState;
use crate::theme;
use crate::views::helpers::format_relative_time;
use crate::views::main_view::MainView;
use crate::views::terminal_tabs::TerminalTabs;
use gpui::Rgba;
use kild_core::ProcessStatus;

/// Width of the sidebar in pixels.
pub const SIDEBAR_WIDTH: f32 = 200.0;

/// Padding adjustment when selected. Reduces left padding by 2px to account
/// for the 2px left border, keeping text alignment consistent.
const SELECTED_PADDING_ADJUSTMENT: f32 = 2.0;

/// Render the navigation sidebar with kilds grouped by status.
pub fn render_sidebar(
    state: &AppState,
    terminal_tabs: &HashMap<String, TerminalTabs>,
    pane_grid: &super::pane_grid::PaneGrid,
    cx: &mut Context<MainView>,
) -> impl IntoElement {
    let active_project_name = state
        .active_project()
        .map(|p| p.name().to_string())
        .unwrap_or_else(|| "All Projects".to_string());

    let filtered = state.filtered_displays();
    let selected_id = state.selected_id().map(|s| s.to_string());

    let mut active_kilds = Vec::new();
    let mut stopped_kilds = Vec::new();

    for display in &filtered {
        match display.process_status {
            ProcessStatus::Running => active_kilds.push(display),
            ProcessStatus::Stopped | ProcessStatus::Unknown => stopped_kilds.push(display),
        }
    }

    let active_count = active_kilds.len();
    let stopped_count = stopped_kilds.len();
    let total_count = active_count + stopped_count;

    div()
        .w(px(SIDEBAR_WIDTH))
        .h_full()
        .bg(theme::obsidian())
        .border_r_1()
        .border_color(theme::border_subtle())
        .flex()
        .flex_col()
        // Header: project name + kild count
        .child(
            div()
                .px(px(theme::SPACE_3))
                .py(px(theme::SPACE_2))
                .border_b_1()
                .border_color(theme::border_subtle())
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(theme::TEXT_SM))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text_bright())
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(active_project_name),
                )
                .child(
                    div()
                        .text_size(px(theme::TEXT_XS))
                        .text_color(theme::text_muted())
                        .flex_shrink_0()
                        .child(format!(
                            "{} kild{}",
                            total_count,
                            if total_count == 1 { "" } else { "s" }
                        )),
                ),
        )
        // Scrollable kild list
        .child(
            div()
                .id("sidebar-scroll")
                .flex_1()
                .overflow_y_scroll()
                // Active section
                .when(!active_kilds.is_empty(), |this| {
                    this.child(render_section_header(
                        "Active",
                        active_count,
                        theme::aurora(),
                    ))
                    .children(active_kilds.iter().enumerate().map(|(ix, display)| {
                        let session_id = display.session.id.clone();
                        let branch = display.session.branch.clone();
                        let is_selected = selected_id.as_deref() == Some(&session_id);
                        let worktree = display.session.worktree_path.clone();
                        let branch_for_edit = branch.clone();
                        let branch_for_stop = branch.clone();
                        let session_id_for_click = session_id.clone();
                        let time_meta = format_relative_time(&display.session.created_at);

                        let tabs_for_session = terminal_tabs.get(&session_id);

                        div()
                            .flex()
                            .flex_col()
                            // Kild row with hover actions
                            .child(render_kild_row_with_actions(
                                ("active-kild", ix),
                                &branch,
                                Status::Active,
                                is_selected,
                                &time_meta,
                                cx.listener(move |view, _, window, cx| {
                                    view.on_kild_select(&session_id_for_click, window, cx);
                                }),
                                render_actions_running(
                                    ix,
                                    worktree,
                                    branch_for_edit,
                                    branch_for_stop,
                                    cx,
                                ),
                            ))
                            // Nested terminal tabs
                            .when_some(tabs_for_session, |this, tabs| {
                                let sid = session_id.clone();
                                this.children((0..tabs.len()).map(|tab_idx| {
                                    let tab_label = tabs
                                        .get(tab_idx)
                                        .map(|e| e.label().to_string())
                                        .unwrap_or_default();
                                    let in_grid = pane_grid.find_slot(&sid, tab_idx).is_some();
                                    let sid = sid.clone();
                                    div()
                                        .id(gpui::SharedString::from(format!(
                                            "sidebar-tab-{}-{}",
                                            sid, tab_idx
                                        )))
                                        .pl(px(theme::SPACE_6 + theme::SPACE_2))
                                        .pr(px(theme::SPACE_2))
                                        .py(px(2.0))
                                        .cursor_pointer()
                                        .text_size(px(theme::TEXT_XS))
                                        .text_color(if in_grid {
                                            theme::ice_dim()
                                        } else {
                                            theme::text_muted()
                                        })
                                        .hover(|s| s.text_color(theme::text()))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .on_mouse_up(
                                            gpui::MouseButton::Left,
                                            cx.listener(move |view, _, window, cx| {
                                                view.on_sidebar_terminal_click(
                                                    &sid, tab_idx, window, cx,
                                                );
                                            }),
                                        )
                                        .child(format!("\u{2514} {}", tab_label))
                                }))
                            })
                    }))
                })
                // Stopped section
                .when(!stopped_kilds.is_empty(), |this| {
                    this.child(render_section_header(
                        "Stopped",
                        stopped_count,
                        theme::copper(),
                    ))
                    .children(stopped_kilds.iter().enumerate().map(|(ix, display)| {
                        let session_id = display.session.id.clone();
                        let branch = display.session.branch.clone();
                        let is_selected = selected_id.as_deref() == Some(&session_id);
                        let worktree = display.session.worktree_path.clone();
                        let branch_for_edit = branch.clone();
                        let branch_for_open = branch.clone();
                        let session_id_for_click = session_id.clone();
                        let time_meta = format_relative_time(&display.session.created_at);

                        let status = match display.process_status {
                            ProcessStatus::Stopped => Status::Stopped,
                            _ => Status::Crashed,
                        };

                        div().flex().flex_col().child(render_kild_row_with_actions(
                            ("stopped-kild", ix),
                            &branch,
                            status,
                            is_selected,
                            &time_meta,
                            cx.listener(move |view, _, window, cx| {
                                view.on_kild_select(&session_id_for_click, window, cx);
                            }),
                            render_actions_stopped(
                                ix,
                                worktree,
                                branch_for_edit,
                                branch_for_open,
                                cx,
                            ),
                        ))
                    }))
                })
                // Empty state
                .when(
                    active_kilds.is_empty() && stopped_kilds.is_empty(),
                    |this| {
                        this.child(
                            div()
                                .px(px(theme::SPACE_4))
                                .py(px(theme::SPACE_6))
                                .text_size(px(theme::TEXT_SM))
                                .text_color(theme::text_subtle())
                                .child("No kilds"),
                        )
                    },
                ),
        )
        // Footer: + Create kild + Add Project
        .child(
            div()
                .px(px(theme::SPACE_3))
                .py(px(theme::SPACE_2))
                .border_t_1()
                .border_color(theme::border_subtle())
                .flex()
                .flex_col()
                .gap(px(theme::SPACE_1))
                // + Create kild
                .child(
                    div()
                        .id("sidebar-create-kild")
                        .py(px(2.0))
                        .cursor_pointer()
                        .text_size(px(theme::TEXT_XS))
                        .text_color(theme::text_muted())
                        .hover(|s| s.text_color(theme::text_subtle()))
                        .on_mouse_up(
                            gpui::MouseButton::Left,
                            cx.listener(|view, _, window, cx| {
                                view.on_create_button_click(window, cx);
                            }),
                        )
                        .child("+ Create kild"),
                )
                // + Add Project
                .child(
                    div()
                        .id("sidebar-add-project")
                        .py(px(2.0))
                        .cursor_pointer()
                        .text_size(px(theme::TEXT_XS))
                        .text_color(theme::text_muted())
                        .hover(|s| s.text_color(theme::text_subtle()))
                        .on_mouse_up(
                            gpui::MouseButton::Left,
                            cx.listener(|view, _, window, cx| {
                                view.on_add_project_click(window, cx);
                            }),
                        )
                        .child("+ Add Project"),
                ),
        )
}

fn render_section_header(title: &str, count: usize, count_color: Rgba) -> impl IntoElement {
    div()
        .px(px(theme::SPACE_3))
        .py(px(theme::SPACE_1))
        .mt(px(theme::SPACE_2))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(px(10.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text_muted())
                .child(title.to_uppercase()),
        )
        .child(
            div()
                .text_size(px(10.0))
                .text_color(count_color)
                .child(count.to_string()),
        )
}

/// Render a kild row with time meta and hover-revealed action buttons.
///
/// Uses GPUI `group()` to show actions and hide meta text on hover.
fn render_kild_row_with_actions(
    id: impl Into<gpui::ElementId>,
    branch: &str,
    status: Status,
    is_selected: bool,
    time_meta: &str,
    on_click: impl Fn(&gpui::MouseUpEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    actions: impl IntoElement,
) -> impl IntoElement {
    div()
        .id(id.into())
        .group("kild-row")
        .relative()
        .w_full()
        .flex()
        .items_center()
        .gap(px(theme::SPACE_2))
        .px(px(theme::SPACE_3))
        .py(px(3.0))
        .cursor_pointer()
        .border_l_2()
        .hover(|style| style.bg(theme::surface()))
        .when(is_selected, |row| {
            row.border_color(theme::ice())
                .bg(theme::surface())
                .pl(px(theme::SPACE_3 - SELECTED_PADDING_ADJUSTMENT))
        })
        .when(!is_selected, |row| row.border_color(theme::transparent()))
        .on_mouse_up(gpui::MouseButton::Left, on_click)
        .child(StatusIndicator::dot(status))
        .child(
            div()
                .flex_1()
                .text_size(px(theme::TEXT_SM))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if is_selected {
                    theme::text_bright()
                } else {
                    theme::text()
                })
                .overflow_hidden()
                .text_ellipsis()
                .min_w(px(0.0))
                .child(branch.to_string()),
        )
        // Time meta — hidden on hover via opacity
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(10.0))
                .text_color(theme::text_muted())
                .group_hover("kild-row", |s| s.opacity(0.0))
                .child(time_meta.to_string()),
        )
        // Hover actions — shown on hover via opacity
        .child(
            div()
                .absolute()
                .right(px(theme::SPACE_1))
                .top_0()
                .bottom_0()
                .flex()
                .items_center()
                .gap(px(2.0))
                .pl(px(theme::SPACE_1))
                .bg(theme::obsidian())
                .opacity(0.0)
                .group_hover("kild-row", |s| s.opacity(1.0))
                .child(actions),
        )
}

/// Tiny ghost action button matching mockup's `.kild-action-btn`.
fn render_action_btn(
    id: impl Into<gpui::ElementId>,
    label: &str,
    on_click: impl Fn(&gpui::MouseUpEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .px(px(4.0))
        .py(px(2.0))
        .cursor_pointer()
        .text_size(px(9.0))
        .text_color(theme::text_muted())
        .border_1()
        .border_color(theme::border())
        .rounded(px(3.0))
        .hover(|s| {
            s.text_color(theme::text_subtle())
                .border_color(theme::border_strong())
                .bg(theme::elevated())
        })
        .on_mouse_up(gpui::MouseButton::Left, on_click)
        .child(label.to_string())
}

fn render_actions_running(
    ix: usize,
    worktree: std::path::PathBuf,
    branch_for_edit: String,
    branch_for_stop: String,
    cx: &mut Context<MainView>,
) -> impl IntoElement {
    div()
        .flex()
        .gap(px(2.0))
        .child({
            let wt = worktree;
            let br = branch_for_edit;
            render_action_btn(
                ("sidebar-edit-active", ix),
                "editor",
                cx.listener(move |view, _, _, cx| {
                    view.on_open_editor_click(&wt, &br, cx);
                }),
            )
        })
        .child({
            let br = branch_for_stop;
            render_action_btn(
                ("sidebar-stop", ix),
                "stop",
                cx.listener(move |view, _, _, cx| {
                    view.on_stop_click(&br, cx);
                }),
            )
        })
}

fn render_actions_stopped(
    ix: usize,
    worktree: std::path::PathBuf,
    branch_for_edit: String,
    branch_for_open: String,
    cx: &mut Context<MainView>,
) -> impl IntoElement {
    div()
        .flex()
        .gap(px(2.0))
        .child({
            let br = branch_for_open;
            render_action_btn(
                ("sidebar-open", ix),
                "open",
                cx.listener(move |view, _, _, cx| {
                    view.on_open_click(&br, cx);
                }),
            )
        })
        .child({
            let wt = worktree;
            let br = branch_for_edit;
            render_action_btn(
                ("sidebar-edit-stopped", ix),
                "editor",
                cx.listener(move |view, _, _, cx| {
                    view.on_open_editor_click(&wt, &br, cx);
                }),
            )
        })
}
