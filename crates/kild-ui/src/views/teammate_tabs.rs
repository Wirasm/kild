//! Horizontal tab bar for teammate switching within a kild.
//!
//! Shows one tab per teammate (or a single agent-named tab if no teammates).
//! Active tab has an ice-colored 2px bottom border.
//! (TODO: Tab click will switch which daemon session's terminal is rendered)

use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{Status, StatusIndicator};
use crate::state::AppState;
use crate::theme;
use crate::views::main_view::MainView;

/// Render a single teammate tab.
fn render_tab(
    name: &str,
    is_active: bool,
    daemon_session_id: Option<String>,
    cx: &mut Context<MainView>,
) -> impl IntoElement {
    let id = gpui::ElementId::Name(
        format!(
            "teammate-tab-{}",
            daemon_session_id.as_deref().unwrap_or(name)
        )
        .into(),
    );

    div()
        .id(id)
        .h_full()
        .flex()
        .items_center()
        .gap(px(theme::SPACE_1))
        .px(px(theme::SPACE_2))
        .cursor_pointer()
        .when(is_active, |this| {
            this.border_b_2().border_color(theme::ice())
        })
        .hover(|style| style.bg(theme::elevated()))
        .child(StatusIndicator::dot(Status::Active))
        .child(
            div()
                .text_size(px(theme::TEXT_SM))
                .text_color(if is_active {
                    theme::text_bright()
                } else {
                    theme::text_muted()
                })
                .child(name.to_string()),
        )
        .when_some(daemon_session_id, |this, dsid| {
            this.on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(move |view, _, window, cx| {
                    view.on_teammate_tab_click(&dsid, window, cx);
                }),
            )
        })
}

/// Render the teammate tab bar.
///
/// Displays a horizontal row of tabs. When the focused kild has teammates
/// (from the shim pane registry), shows one tab per teammate. Otherwise
/// shows a single tab with the kild's agent name.
pub fn render_teammate_tabs(state: &AppState, cx: &mut Context<MainView>) -> impl IntoElement {
    let focused_id = state.focused_kild_id();
    let selected_kild =
        focused_id.and_then(|id| state.displays().iter().find(|d| d.session.id == id));

    // Build tab list from teammates or fall back to single agent tab
    let tabs: Vec<gpui::AnyElement> = if let Some(kild_id) = focused_id {
        let teammates = state.get_teammates(kild_id);
        if teammates.is_empty() {
            // Single agent — show its name
            let agent_name = selected_kild
                .map(|d| d.session.agent.as_str())
                .unwrap_or("agent");
            vec![render_tab(agent_name, true, None, cx).into_any_element()]
        } else {
            // Multi-teammate — one tab per pane
            // TODO: Track which teammate's terminal is focused.
            // For now, always mark the leader as active when teammates exist.

            teammates
                .iter()
                .map(|tm| {
                    let name = if tm.title().is_empty() {
                        if tm.is_leader() { "lead" } else { tm.pane_id() }
                    } else {
                        tm.title()
                    };
                    let is_active = tm.is_leader();
                    render_tab(
                        name,
                        is_active,
                        Some(tm.daemon_session_id().to_string()),
                        cx,
                    )
                    .into_any_element()
                })
                .collect()
        }
    } else {
        // No kild focused — show placeholder
        vec![render_tab("—", false, None, cx).into_any_element()]
    };

    div()
        .w_full()
        .h(px(32.0))
        .flex()
        .items_center()
        .px(px(theme::SPACE_2))
        .gap(px(theme::SPACE_1))
        .bg(theme::surface())
        .border_b_1()
        .border_color(theme::border_subtle())
        .children(tabs)
}
