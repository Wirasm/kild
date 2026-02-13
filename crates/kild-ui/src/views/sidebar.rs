//! Kild navigation sidebar.
//!
//! Fixed left sidebar (200px) showing kilds grouped by Active/Stopped status
//! with nested terminal tab names.

use gpui::{Context, FontWeight, IntoElement, ParentElement, Styled, div, prelude::*, px};
use std::collections::HashMap;

use crate::components::{Status, StatusIndicator};
use crate::state::AppState;
use crate::theme;
use crate::views::main_view::MainView;
use crate::views::terminal_tabs::TerminalTabs;
use gpui_component::button::{Button, ButtonVariants};
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

    div()
        .w(px(SIDEBAR_WIDTH))
        .h_full()
        .bg(theme::obsidian())
        .border_r_1()
        .border_color(theme::border_subtle())
        .flex()
        .flex_col()
        // Header: active project name
        .child(
            div()
                .px(px(theme::SPACE_4))
                .py(px(theme::SPACE_3))
                .border_b_1()
                .border_color(theme::border_subtle())
                .text_size(px(theme::TEXT_XS))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text_muted())
                .overflow_hidden()
                .text_ellipsis()
                .child(active_project_name.to_uppercase()),
        )
        // Scrollable kild list
        .child(
            div()
                .id("sidebar-scroll")
                .flex_1()
                .overflow_y_scroll()
                // Active section
                .when(!active_kilds.is_empty(), |this| {
                    this.child(render_section_header("ACTIVE")).children(
                        active_kilds.iter().enumerate().map(|(ix, display)| {
                            let session_id = display.session.id.clone();
                            let branch = display.session.branch.clone();
                            let is_selected = selected_id.as_deref() == Some(&session_id);
                            let worktree = display.session.worktree_path.clone();
                            let branch_for_edit = branch.clone();
                            let branch_for_stop = branch.clone();
                            let session_id_for_click = session_id.clone();

                            let tabs_for_session = terminal_tabs.get(&session_id);

                            div()
                                .flex()
                                .flex_col()
                                // Kild row
                                .child(render_kild_row(
                                    ("active-kild", ix),
                                    &branch,
                                    Status::Active,
                                    is_selected,
                                    cx.listener(move |view, _, window, cx| {
                                        view.on_kild_select(&session_id_for_click, window, cx);
                                    }),
                                ))
                                // Inline actions (editor + stop) - shown when selected
                                .when(is_selected, |this| {
                                    this.child(render_actions_running(
                                        ix,
                                        worktree,
                                        branch_for_edit,
                                        branch_for_stop,
                                        cx,
                                    ))
                                })
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
                        }),
                    )
                })
                // Stopped section
                .when(!stopped_kilds.is_empty(), |this| {
                    this.child(render_section_header("STOPPED")).children(
                        stopped_kilds.iter().enumerate().map(|(ix, display)| {
                            let session_id = display.session.id.clone();
                            let branch = display.session.branch.clone();
                            let is_selected = selected_id.as_deref() == Some(&session_id);
                            let worktree = display.session.worktree_path.clone();
                            let branch_for_edit = branch.clone();
                            let branch_for_open = branch.clone();
                            let session_id_for_click = session_id.clone();

                            let status = match display.process_status {
                                ProcessStatus::Stopped => Status::Stopped,
                                _ => Status::Crashed,
                            };

                            div()
                                .flex()
                                .flex_col()
                                .child(render_kild_row(
                                    ("stopped-kild", ix),
                                    &branch,
                                    status,
                                    is_selected,
                                    cx.listener(move |view, _, window, cx| {
                                        view.on_kild_select(&session_id_for_click, window, cx);
                                    }),
                                ))
                                // Inline actions (editor + open) - shown when selected
                                .when(is_selected, |this| {
                                    this.child(render_actions_stopped(
                                        ix,
                                        worktree,
                                        branch_for_edit,
                                        branch_for_open,
                                        cx,
                                    ))
                                })
                        }),
                    )
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
        // Footer: Add Project button
        .child(
            div()
                .px(px(theme::SPACE_4))
                .py(px(theme::SPACE_3))
                .border_t_1()
                .border_color(theme::border_subtle())
                .child(
                    Button::new("sidebar-add-project")
                        .label("+ Add Project")
                        .ghost()
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.on_add_project_click(window, cx);
                        })),
                ),
        )
}

fn render_section_header(title: &str) -> impl IntoElement {
    div()
        .px(px(theme::SPACE_4))
        .py(px(theme::SPACE_2))
        .text_size(px(theme::TEXT_XS))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::text_muted())
        .child(title.to_string())
}

fn render_kild_row(
    id: impl Into<gpui::ElementId>,
    branch: &str,
    status: Status,
    is_selected: bool,
    on_click: impl Fn(&gpui::MouseUpEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .w_full()
        .flex()
        .items_center()
        .gap(px(theme::SPACE_2))
        .px(px(theme::SPACE_4))
        .py(px(theme::SPACE_2))
        .cursor_pointer()
        .hover(|style| style.bg(theme::surface()))
        .when(is_selected, |row| {
            row.border_l_2()
                .border_color(theme::ice())
                .bg(theme::surface())
                .pl(px(theme::SPACE_4 - SELECTED_PADDING_ADJUSTMENT))
        })
        .on_mouse_up(gpui::MouseButton::Left, on_click)
        .child(StatusIndicator::dot(status))
        .child(
            div()
                .flex_1()
                .text_size(px(theme::TEXT_SM))
                .text_color(theme::text())
                .overflow_hidden()
                .text_ellipsis()
                .child(branch.to_string()),
        )
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
        .gap(px(theme::SPACE_1))
        .pl(px(theme::SPACE_6))
        .pb(px(theme::SPACE_1))
        .child({
            let wt = worktree;
            let br = branch_for_edit;
            Button::new(("sidebar-edit-active", ix))
                .label("Edit")
                .ghost()
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.on_open_editor_click(&wt, &br, cx);
                }))
        })
        .child({
            let br = branch_for_stop;
            Button::new(("sidebar-stop", ix))
                .label("\u{23F9}")
                .warning()
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.on_stop_click(&br, cx);
                }))
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
        .gap(px(theme::SPACE_1))
        .pl(px(theme::SPACE_6))
        .pb(px(theme::SPACE_1))
        .child({
            let wt = worktree;
            let br = branch_for_edit;
            Button::new(("sidebar-edit-stopped", ix))
                .label("Edit")
                .ghost()
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.on_open_editor_click(&wt, &br, cx);
                }))
        })
        .child({
            let br = branch_for_open;
            Button::new(("sidebar-open", ix))
                .label("\u{25B6}")
                .success()
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.on_open_click(&br, cx);
                }))
        })
}
