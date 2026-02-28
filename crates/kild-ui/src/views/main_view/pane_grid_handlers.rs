//! Pane grid management handlers for MainView.

use gpui::{Context, Window};

use super::main_view_def::MainView;
use super::types::FocusRegion;

impl MainView {
    /// Reset workspaces to a single pane grid and auto-populate from current displays.
    pub(super) fn reset_pane_grid(&mut self) {
        self.workspaces = vec![super::super::pane_grid::PaneGrid::new()];
        self.active_workspace = 0;
        let displays = self.state.filtered_displays();
        let displays_owned: Vec<kild_core::SessionSnapshot> =
            displays.into_iter().cloned().collect();
        self.workspaces[0].auto_populate(&displays_owned, &self.terminal_tabs);
    }

    /// Place a terminal in the active workspace's pane grid. If already present, just focus it.
    /// If grid is full, replace the least-recently-focused pane.
    pub(super) fn place_in_pane_grid(
        &mut self,
        session_id: &str,
        tab_idx: usize,
        branch: &str,
        status: crate::components::Status,
    ) {
        // Already in grid? Just focus it.
        if let Some(slot_idx) = self.active_pane_grid().find_slot(session_id, tab_idx) {
            self.active_pane_grid_mut().set_focus(slot_idx);
            return;
        }

        // Try to add to an empty slot.
        if self
            .active_pane_grid_mut()
            .add_terminal(session_id.to_string(), tab_idx, branch.to_string(), status)
            .is_some()
        {
            return;
        }

        // Grid full — replace LRU slot.
        let lru = self.active_pane_grid().least_recently_focused();
        self.active_pane_grid_mut().remove(lru);
        if self
            .active_pane_grid_mut()
            .add_terminal(session_id.to_string(), tab_idx, branch.to_string(), status)
            .is_none()
        {
            tracing::error!(
                event = "ui.pane_grid.add_after_lru_remove_failed",
                session_id = session_id,
                lru_slot = lru,
            );
            self.state.push_error(
                "Failed to place terminal in pane grid — this is a bug, please report it."
                    .to_string(),
            );
        }
    }

    /// Handle click inside a pane to focus it.
    pub fn on_pane_focus(&mut self, slot_idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.active_pane_grid_mut().set_focus(slot_idx);

        let slot_data = if let super::super::pane_grid::PaneSlot::Occupied {
            session_id,
            tab_idx,
            ..
        } = self.active_pane_grid().slot(slot_idx)
        {
            Some((session_id.clone(), *tab_idx))
        } else {
            None
        };

        if let Some((sid, tidx)) = slot_data {
            self.active_terminal_id = Some(sid.clone());
            if let Some(tabs) = self.terminal_tabs.get_mut(&sid) {
                tabs.set_active(tidx);
            }
            self.focus_region = FocusRegion::Terminal;
            self.focus_active_terminal(window, cx);
        }
        cx.notify();
    }

    /// Handle maximize/restore toggle on a pane.
    pub fn on_pane_maximize(&mut self, slot_idx: usize, cx: &mut Context<Self>) {
        self.active_pane_grid_mut().toggle_maximize(slot_idx);
        cx.notify();
    }

    /// Handle close button on a pane.
    pub fn on_pane_close(&mut self, slot_idx: usize, cx: &mut Context<Self>) {
        self.active_pane_grid_mut().remove(slot_idx);

        // If the closed pane was focused, move to next occupied or unfocus
        if self.active_pane_grid().focused_slot() == slot_idx {
            if let Some(next) = self.active_pane_grid().next_occupied_slot() {
                self.active_pane_grid_mut().set_focus(next);
                if let super::super::pane_grid::PaneSlot::Occupied { session_id, .. } =
                    self.active_pane_grid().slot(next)
                {
                    self.active_terminal_id = Some(session_id.clone());
                }
            } else {
                self.active_terminal_id = None;
                self.focus_region = FocusRegion::Dashboard;
            }
        }
        cx.notify();
    }
}
