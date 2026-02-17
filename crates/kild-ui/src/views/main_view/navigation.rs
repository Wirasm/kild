//! Kild and terminal tab navigation handlers for MainView.

use gpui::{Context, Window};

use super::main_view_def::MainView;
use super::types::{ActiveView, FocusRegion};

impl MainView {
    /// Navigate to the next kild in the filtered list (wrapping).
    pub(super) fn navigate_next_kild(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let displays = self.state.filtered_displays();
        if displays.is_empty() {
            return;
        }
        let current_idx = self
            .state
            .selected_id()
            .and_then(|id| displays.iter().position(|d| &*d.session.id == id));
        let next_idx = match current_idx {
            Some(idx) => (idx + 1) % displays.len(),
            None => 0,
        };
        tracing::debug!(
            event = "ui.kild.navigate_next",
            from = ?self.state.selected_id(),
            to_idx = next_idx
        );
        let next_id = displays[next_idx].session.id.to_string();
        self.on_kild_select(&next_id, window, cx);
    }

    /// Navigate to the previous kild in the filtered list (wrapping).
    pub(super) fn navigate_prev_kild(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let displays = self.state.filtered_displays();
        if displays.is_empty() {
            return;
        }
        let current_idx = self
            .state
            .selected_id()
            .and_then(|id| displays.iter().position(|d| &*d.session.id == id));
        let prev_idx = match current_idx {
            Some(0) | None => displays.len() - 1,
            Some(idx) => idx - 1,
        };
        tracing::debug!(
            event = "ui.kild.navigate_prev",
            from = ?self.state.selected_id(),
            to_idx = prev_idx
        );
        let prev_id = displays[prev_idx].session.id.to_string();
        self.on_kild_select(&prev_id, window, cx);
    }

    /// Jump to kild by index (0-based) in the filtered display list.
    pub(super) fn navigate_to_kild_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let displays = self.state.filtered_displays();
        if let Some(info) = displays.get(index) {
            tracing::debug!(
                event = "ui.kild.navigate_index",
                index = index,
                target = %info.session.id,
            );
            let id = info.session.id.clone();
            self.on_kild_select(&id, window, cx);
        }
    }

    /// Navigate to the next teammate terminal tab within the current kild.
    pub(super) fn navigate_next_teammate_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = &self.active_terminal_id
            && let Some(tabs) = self.terminal_tabs.get_mut(id)
            && tabs.len() > 1
        {
            tabs.cycle_next();
            self.focus_active_terminal(window, cx);
        }
    }

    /// Navigate to the previous teammate terminal tab within the current kild.
    pub(super) fn navigate_prev_teammate_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = &self.active_terminal_id
            && let Some(tabs) = self.terminal_tabs.get_mut(id)
            && tabs.len() > 1
        {
            tabs.cycle_prev();
            self.focus_active_terminal(window, cx);
        }
    }

    /// Toggle between Control and Dashboard views.
    pub(super) fn toggle_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active_view = match self.active_view {
            ActiveView::Control => ActiveView::Dashboard,
            ActiveView::Dashboard | ActiveView::Detail => ActiveView::Control,
        };
        tracing::debug!(event = "ui.view.toggled", view = ?self.active_view);
        if self.active_view == ActiveView::Control && self.active_terminal_id.is_some() {
            self.focus_region = FocusRegion::Terminal;
            self.focus_active_terminal(window, cx);
        } else {
            self.focus_region = FocusRegion::Dashboard;
            window.focus(&self.focus_handle);
        }
        cx.notify();
    }
}
