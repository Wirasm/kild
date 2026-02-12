//! Kild sidebar component.
//!
//! 220px wide sidebar with status-grouped kild list.

// Allow dead_code — scaffolding for the multiplexer sidebar, wired up in later tasks.
#![allow(dead_code)]

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement, Styled, div, prelude::*, px,
};

use crate::components::{Status, StatusIndicator};
use crate::state::AppState;
use crate::theme;
use crate::views::main_view::MainView;
use kild_core::ProcessStatus;

/// Width of the kild sidebar in pixels.
pub const KILD_SIDEBAR_WIDTH: f32 = 220.0;

/// Render the kild sidebar with status-grouped sections.
pub fn render_kild_sidebar(state: &AppState, cx: &mut Context<MainView>) -> impl IntoElement {
    let filtered: Vec<_> = state.filtered_displays().into_iter().cloned().collect();

    let mut active_displays = Vec::new();
    let mut stopped_displays = Vec::new();

    for display in &filtered {
        match display.process_status {
            ProcessStatus::Running => active_displays.push(display),
            ProcessStatus::Stopped | ProcessStatus::Unknown => stopped_displays.push(display),
        }
    }

    // Pre-build rows to avoid borrowing cx inside closures
    let active_count = active_displays.len();
    let stopped_count = stopped_displays.len();

    let active_rows: Vec<AnyElement> = active_displays
        .iter()
        .enumerate()
        .map(|(ix, display)| render_kild_row(display, ix, "active", cx).into_any_element())
        .collect();

    let stopped_rows: Vec<AnyElement> = stopped_displays
        .iter()
        .enumerate()
        .map(|(ix, display)| render_kild_row(display, ix, "stopped", cx).into_any_element())
        .collect();

    let is_empty = filtered.is_empty();

    div()
        .w(px(KILD_SIDEBAR_WIDTH))
        .h_full()
        .flex_col()
        .bg(theme::obsidian())
        .border_r_1()
        .border_color(theme::border_subtle())
        // Scrollable content
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                // Active section
                .when(active_count > 0, |this| {
                    this.child(render_section_header("ACTIVE", active_count))
                })
                .children(active_rows)
                // Stopped section
                .when(stopped_count > 0, |this| {
                    this.child(render_section_header("STOPPED", stopped_count))
                })
                .children(stopped_rows)
                // Empty state
                .when(is_empty, |this| {
                    this.child(
                        div()
                            .px(px(theme::SPACE_3))
                            .py(px(theme::SPACE_4))
                            .text_size(px(theme::TEXT_SM))
                            .text_color(theme::text_muted())
                            .child("No kilds"),
                    )
                }),
        )
}

/// Render a section header (e.g., "ACTIVE (3)").
fn render_section_header(label: &str, count: usize) -> impl IntoElement {
    div()
        .px(px(theme::SPACE_3))
        .py(px(theme::SPACE_2))
        .text_size(px(theme::TEXT_XS))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::text_muted())
        .child(format!("{} ({})", label, count))
}

/// Render a single kild row in the sidebar.
fn render_kild_row(
    display: &kild_core::SessionInfo,
    ix: usize,
    section: &'static str,
    cx: &mut Context<MainView>,
) -> impl IntoElement {
    let status = match display.process_status {
        ProcessStatus::Running => Status::Active,
        ProcessStatus::Stopped | ProcessStatus::Unknown => Status::Stopped,
    };

    let session_id = display.session.id.clone();
    let branch = display.session.branch.clone();
    let agent = display.session.agent.clone();

    div()
        .id((section, ix))
        .flex()
        .items_center()
        .gap(px(theme::SPACE_2))
        .px(px(theme::SPACE_3))
        .py(px(theme::SPACE_1))
        .cursor_pointer()
        .hover(|style| style.bg(theme::surface()))
        .on_mouse_up(
            gpui::MouseButton::Left,
            cx.listener(move |view, _, window, cx| {
                view.on_kild_sidebar_click(&session_id, window, cx);
            }),
        )
        // Status dot
        .child(StatusIndicator::dot(status))
        // Branch name + agent
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .flex_col()
                .child(
                    div()
                        .text_size(px(theme::TEXT_SM))
                        .text_color(theme::text())
                        .text_ellipsis()
                        .child(branch),
                )
                .child(
                    div()
                        .text_size(px(theme::TEXT_XS))
                        .text_color(theme::text_muted())
                        .child(agent),
                ),
        )
}
