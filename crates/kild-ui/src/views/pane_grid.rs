//! Pane grid for the Control view.
//!
//! Provides a 2x2 grid of terminal panes. Each pane slot is either occupied
//! (showing a terminal from a kild session) or empty (placeholder). Tracks
//! focus order for LRU replacement when the grid is full.

use gpui::{IntoElement, SharedString, div, prelude::*, px};
use gpui_component::resizable::{h_resizable, resizable_panel, v_resizable};

use crate::components::Status;
use crate::theme;
use crate::views::main_view::MainView;
use crate::views::terminal_tabs::TerminalTabs;
use kild_core::ProcessStatus;

/// Maximum number of pane slots in the grid.
const SLOT_COUNT: usize = 4;

/// A single slot in the pane grid.
#[derive(Clone, Debug)]
pub enum PaneSlot {
    Occupied {
        session_id: String,
        tab_idx: usize,
        kild_branch: String,
        kild_status: Status,
    },
    Empty,
}

/// 2x2 pane grid tracking which terminals are visible and focus order.
pub struct PaneGrid {
    slots: [PaneSlot; SLOT_COUNT],
    focused_slot: usize,
    maximized_slot: Option<usize>,
    /// Focus history: most-recently-focused at the end.
    focus_order: Vec<usize>,
}

impl PaneGrid {
    pub fn new() -> Self {
        Self {
            slots: [
                PaneSlot::Empty,
                PaneSlot::Empty,
                PaneSlot::Empty,
                PaneSlot::Empty,
            ],
            focused_slot: 0,
            maximized_slot: None,
            focus_order: Vec::new(),
        }
    }

    /// Add a terminal to the first empty slot. Returns the slot index, or `None` if full
    /// or if the (session_id, tab_idx) pair is already in the grid.
    pub fn add_terminal(
        &mut self,
        session_id: String,
        tab_idx: usize,
        branch: String,
        status: Status,
    ) -> Option<usize> {
        // Prevent duplicates
        if self.find_slot(&session_id, tab_idx).is_some() {
            return None;
        }
        let empty = self.slots.iter().position(|s| matches!(s, PaneSlot::Empty));
        if let Some(idx) = empty {
            self.slots[idx] = PaneSlot::Occupied {
                session_id,
                tab_idx,
                kild_branch: branch,
                kild_status: status,
            };
            self.set_focus(idx);
            Some(idx)
        } else {
            None
        }
    }

    /// Remove the terminal from a slot, making it empty.
    pub fn remove(&mut self, slot_idx: usize) {
        if slot_idx < SLOT_COUNT {
            self.slots[slot_idx] = PaneSlot::Empty;
            self.focus_order.retain(|&i| i != slot_idx);
            if self.maximized_slot == Some(slot_idx) {
                self.maximized_slot = None;
            }
        }
    }

    /// Set focus to the given slot index. Only focuses occupied slots.
    pub fn set_focus(&mut self, slot_idx: usize) {
        if slot_idx < SLOT_COUNT && matches!(self.slots[slot_idx], PaneSlot::Occupied { .. }) {
            self.focused_slot = slot_idx;
            self.focus_order.retain(|&i| i != slot_idx);
            self.focus_order.push(slot_idx);
        }
    }

    pub fn focused_slot(&self) -> usize {
        self.focused_slot
    }

    pub fn maximized_slot(&self) -> Option<usize> {
        self.maximized_slot
    }

    /// Toggle maximize for the given slot. If already maximized, restore.
    /// Only allows maximizing occupied slots within bounds.
    pub fn toggle_maximize(&mut self, slot_idx: usize) {
        if self.maximized_slot == Some(slot_idx) {
            self.maximized_slot = None;
        } else if slot_idx < SLOT_COUNT && matches!(self.slots[slot_idx], PaneSlot::Occupied { .. })
        {
            self.maximized_slot = Some(slot_idx);
        }
    }

    /// Find a slot containing the given session and tab index.
    pub fn find_slot(&self, session_id: &str, tab_idx: usize) -> Option<usize> {
        self.slots.iter().position(|s| {
            matches!(s, PaneSlot::Occupied { session_id: sid, tab_idx: ti, .. }
                     if sid == session_id && *ti == tab_idx)
        })
    }

    /// Return the least-recently-focused occupied slot for replacement.
    /// Only call when at least one slot is occupied (i.e., grid is full).
    pub fn least_recently_focused(&self) -> usize {
        // Walk focus_order from oldest to newest, return first occupied.
        for &idx in &self.focus_order {
            if matches!(self.slots[idx], PaneSlot::Occupied { .. }) {
                return idx;
            }
        }
        // Fallback: first occupied slot.
        self.slots
            .iter()
            .position(|s| matches!(s, PaneSlot::Occupied { .. }))
            .expect("least_recently_focused called with no occupied slots")
    }

    /// Get a reference to a slot. Panics if `idx >= SLOT_COUNT`.
    pub fn slot(&self, idx: usize) -> &PaneSlot {
        self.slots
            .get(idx)
            .expect("slot index must be < SLOT_COUNT")
    }

    /// Get all slots as a slice (used in tests).
    #[cfg(test)]
    pub fn slots(&self) -> &[PaneSlot; SLOT_COUNT] {
        &self.slots
    }

    /// Auto-populate empty slots from active displays that have terminals.
    pub fn auto_populate(
        &mut self,
        displays: &[kild_core::SessionSnapshot],
        terminal_tabs: &std::collections::HashMap<String, TerminalTabs>,
    ) {
        for display in displays {
            // Only auto-populate from sessions that have terminal tabs
            if !terminal_tabs.contains_key(&*display.session.id) {
                continue;
            }
            // Skip if already in the grid
            if self.find_slot(&display.session.id, 0).is_some() {
                continue;
            }
            let status = process_status_to_status(display.process_status);
            if self
                .add_terminal(
                    display.session.id.to_string(),
                    0,
                    display.session.branch.to_string(),
                    status,
                )
                .is_none()
            {
                break; // Grid full
            }
        }
    }

    /// Remove any slots referencing sessions not in the live set.
    /// Updates focused_slot if the focused slot was pruned.
    pub fn prune(&mut self, live_ids: &std::collections::HashSet<&str>) {
        for idx in 0..SLOT_COUNT {
            if let PaneSlot::Occupied { session_id, .. } = &self.slots[idx]
                && !live_ids.contains(session_id.as_str())
            {
                self.remove(idx);
            }
        }
        // If focused slot was pruned (now empty), move to next occupied
        if matches!(self.slots[self.focused_slot], PaneSlot::Empty)
            && let Some(next) = self.next_occupied_slot()
        {
            self.focused_slot = next;
        }
    }

    /// Return the next occupied slot after the focused one (wrapping).
    pub fn next_occupied_slot(&self) -> Option<usize> {
        for offset in 1..SLOT_COUNT {
            let idx = (self.focused_slot + offset) % SLOT_COUNT;
            if matches!(self.slots[idx], PaneSlot::Occupied { .. }) {
                return Some(idx);
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the 2x2 pane grid (or maximized single pane).
pub fn render_pane_grid(
    pane_grid: &PaneGrid,
    terminal_tabs: &std::collections::HashMap<String, TerminalTabs>,
    cx: &mut gpui::Context<MainView>,
) -> impl IntoElement {
    if let Some(max_idx) = pane_grid.maximized_slot() {
        // Maximized: single pane full-size
        div()
            .flex_1()
            .overflow_hidden()
            .child(render_pane_cell(max_idx, pane_grid, terminal_tabs, cx))
            .into_any_element()
    } else {
        // 2x2 grid using resizable panels
        div()
            .flex_1()
            .overflow_hidden()
            .child(
                v_resizable("pane-grid-v")
                    .child(
                        h_resizable("pane-grid-row-0")
                            .child(resizable_panel().child(render_pane_cell(
                                0,
                                pane_grid,
                                terminal_tabs,
                                cx,
                            )))
                            .child(resizable_panel().child(render_pane_cell(
                                1,
                                pane_grid,
                                terminal_tabs,
                                cx,
                            ))),
                    )
                    .child(
                        h_resizable("pane-grid-row-1")
                            .child(resizable_panel().child(render_pane_cell(
                                2,
                                pane_grid,
                                terminal_tabs,
                                cx,
                            )))
                            .child(resizable_panel().child(render_pane_cell(
                                3,
                                pane_grid,
                                terminal_tabs,
                                cx,
                            ))),
                    ),
            )
            .into_any_element()
    }
}

/// Render a single pane cell (occupied or empty).
fn render_pane_cell(
    slot_idx: usize,
    pane_grid: &PaneGrid,
    terminal_tabs: &std::collections::HashMap<String, TerminalTabs>,
    cx: &mut gpui::Context<MainView>,
) -> impl IntoElement {
    let is_focused = pane_grid.focused_slot() == slot_idx;
    let is_maximized = pane_grid.maximized_slot() == Some(slot_idx);
    let group_name: SharedString = format!("pane-{slot_idx}").into();

    match pane_grid.slot(slot_idx) {
        PaneSlot::Occupied {
            session_id,
            tab_idx,
            kild_branch,
            kild_status,
        } => {
            let terminal_view = terminal_tabs
                .get(session_id)
                .and_then(|tabs| tabs.get(*tab_idx))
                .map(|entry| entry.view().clone());

            let header = render_pane_header(
                slot_idx,
                kild_branch,
                *kild_status,
                is_focused,
                is_maximized,
                group_name.clone(),
                cx,
            );

            let cell = div()
                .id(SharedString::from(format!("pane-cell-{slot_idx}")))
                .group(group_name)
                .flex_1()
                .flex()
                .flex_col()
                .overflow_hidden()
                .border_1()
                .border_color(if is_focused {
                    theme::ice_dim()
                } else {
                    theme::border_subtle()
                })
                .child(header)
                .when_some(terminal_view, |this, view| {
                    this.child(div().flex_1().overflow_hidden().child(view))
                })
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |view, _, window, cx| {
                        view.on_pane_focus(slot_idx, window, cx);
                    }),
                );

            cell.into_any_element()
        }
        PaneSlot::Empty => render_empty_slot(slot_idx, cx).into_any_element(),
    }
}

/// Render the header bar for an occupied pane.
fn render_pane_header(
    slot_idx: usize,
    branch: &str,
    status: Status,
    is_focused: bool,
    is_maximized: bool,
    group_name: SharedString,
    cx: &mut gpui::Context<MainView>,
) -> impl IntoElement {
    let maximize_label = if is_maximized { "Restore" } else { "Maximize" };

    div()
        .flex()
        .items_center()
        .justify_between()
        .px(px(theme::SPACE_2))
        .py(px(2.0))
        .bg(theme::surface())
        .border_b_1()
        .border_color(theme::border_subtle())
        // Left: status dot + branch name
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(theme::SPACE_1))
                .overflow_hidden()
                .child(
                    div()
                        .size(px(5.0))
                        .rounded_full()
                        .bg(status.color())
                        .flex_shrink_0(),
                )
                .child(
                    div()
                        .text_size(px(theme::TEXT_XS))
                        .text_color(if is_focused {
                            theme::text_bright()
                        } else {
                            theme::text_subtle()
                        })
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(SharedString::from(branch.to_string())),
                ),
        )
        // Right: maximize + close buttons (visible on hover)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(theme::SPACE_1))
                .opacity(0.0)
                .group_hover(group_name.clone(), |s| s.opacity(1.0))
                .child(
                    div()
                        .id(SharedString::from(format!("pane-max-{slot_idx}")))
                        .cursor_pointer()
                        .text_size(px(theme::TEXT_XS))
                        .text_color(theme::text_muted())
                        .hover(|s| s.text_color(theme::text()))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |view, _, _window, cx| {
                                view.on_pane_maximize(slot_idx, cx);
                            }),
                        )
                        .child(maximize_label),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("pane-close-{slot_idx}")))
                        .cursor_pointer()
                        .text_size(px(theme::TEXT_XS))
                        .text_color(theme::text_muted())
                        .hover(|s| s.text_color(theme::ember()))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |view, _, _window, cx| {
                                view.on_pane_close(slot_idx, cx);
                            }),
                        )
                        .child("\u{00D7}"), // × symbol
                ),
        )
}

/// Render an empty pane slot with placeholder text.
fn render_empty_slot(slot_idx: usize, _cx: &mut gpui::Context<MainView>) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("pane-empty-{slot_idx}")))
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border_subtle())
        .child(
            div()
                .text_size(px(theme::TEXT_SM))
                .text_color(theme::text_subtle())
                .child("Click a terminal in the sidebar"),
        )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert `ProcessStatus` to `Status` for the status indicator.
pub fn process_status_to_status(ps: ProcessStatus) -> Status {
    match ps {
        ProcessStatus::Running => Status::Active,
        ProcessStatus::Stopped => Status::Stopped,
        ProcessStatus::Unknown => Status::Stopped,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_grid_all_empty() {
        let grid = PaneGrid::new();
        for slot in grid.slots() {
            assert!(matches!(slot, PaneSlot::Empty));
        }
    }

    #[test]
    fn test_add_terminal_fills_first_empty() {
        let mut grid = PaneGrid::new();
        let idx = grid.add_terminal("s1".into(), 0, "auth".into(), Status::Active);
        assert_eq!(idx, Some(0));
        assert!(matches!(
            grid.slot(0),
            PaneSlot::Occupied { session_id, .. } if session_id == "s1"
        ));
    }

    #[test]
    fn test_add_terminal_skips_occupied() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s1".into(), 0, "auth".into(), Status::Active);
        let idx = grid.add_terminal("s2".into(), 0, "api".into(), Status::Active);
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn test_add_terminal_all_full_returns_none() {
        let mut grid = PaneGrid::new();
        for i in 0..4 {
            grid.add_terminal(format!("s{i}"), 0, format!("b{i}"), Status::Active);
        }
        let idx = grid.add_terminal("s4".into(), 0, "extra".into(), Status::Active);
        assert_eq!(idx, None);
    }

    #[test]
    fn test_remove_clears_slot() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s1".into(), 0, "auth".into(), Status::Active);
        grid.remove(0);
        assert!(matches!(grid.slot(0), PaneSlot::Empty));
    }

    #[test]
    fn test_find_slot_returns_correct_index() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s1".into(), 0, "auth".into(), Status::Active);
        grid.add_terminal("s2".into(), 1, "api".into(), Status::Active);
        assert_eq!(grid.find_slot("s2", 1), Some(1));
    }

    #[test]
    fn test_find_slot_returns_none_for_missing() {
        let grid = PaneGrid::new();
        assert_eq!(grid.find_slot("nope", 0), None);
    }

    #[test]
    fn test_toggle_maximize() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s1".into(), 0, "auth".into(), Status::Active);
        assert_eq!(grid.maximized_slot(), None);

        grid.toggle_maximize(0);
        assert_eq!(grid.maximized_slot(), Some(0));

        grid.toggle_maximize(0);
        assert_eq!(grid.maximized_slot(), None);
    }

    #[test]
    fn test_focus_tracking_and_lru_order() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s1".into(), 0, "a".into(), Status::Active);
        grid.add_terminal("s2".into(), 0, "b".into(), Status::Active);
        grid.add_terminal("s3".into(), 0, "c".into(), Status::Active);

        // After adds, focus order is [0, 1, 2]. LRU = 0.
        assert_eq!(grid.focused_slot(), 2);
        assert_eq!(grid.least_recently_focused(), 0);

        // Focus slot 0 → order becomes [1, 2, 0]. LRU = 1.
        grid.set_focus(0);
        assert_eq!(grid.focused_slot(), 0);
        assert_eq!(grid.least_recently_focused(), 1);

        // Focus slot 1 → order becomes [2, 0, 1]. LRU = 2.
        grid.set_focus(1);
        assert_eq!(grid.least_recently_focused(), 2);
    }

    #[test]
    fn test_least_recently_focused() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s0".into(), 0, "a".into(), Status::Active);
        grid.add_terminal("s1".into(), 0, "b".into(), Status::Active);
        grid.add_terminal("s2".into(), 0, "c".into(), Status::Active);
        grid.add_terminal("s3".into(), 0, "d".into(), Status::Active);

        // Focus order: [0, 1, 2, 3] — 0 is LRU
        // But set_focus was called by add_terminal for each, so order is [0, 1, 2, 3]
        // LRU = 0
        assert_eq!(grid.least_recently_focused(), 0);

        // Focus slot 0 → order becomes [1, 2, 3, 0], LRU = 1
        grid.set_focus(0);
        assert_eq!(grid.least_recently_focused(), 1);
    }

    #[test]
    fn test_prune_removes_stale_sessions() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s1".into(), 0, "auth".into(), Status::Active);
        grid.add_terminal("s2".into(), 0, "api".into(), Status::Active);

        let live: std::collections::HashSet<&str> = ["s1"].into_iter().collect();
        grid.prune(&live);

        assert!(matches!(grid.slot(0), PaneSlot::Occupied { .. }));
        assert!(matches!(grid.slot(1), PaneSlot::Empty));
    }

    #[test]
    fn test_next_occupied_slot() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s0".into(), 0, "a".into(), Status::Active);
        // Slot 1 empty
        grid.add_terminal("s2".into(), 0, "c".into(), Status::Active); // goes to slot 1 actually

        grid.set_focus(0);
        let next = grid.next_occupied_slot();
        assert_eq!(next, Some(1));
    }

    #[test]
    fn test_next_occupied_slot_wraps_around() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s0".into(), 0, "a".into(), Status::Active); // slot 0
        grid.add_terminal("s1".into(), 0, "b".into(), Status::Active); // slot 1

        grid.set_focus(1); // Focus last occupied
        let next = grid.next_occupied_slot();
        assert_eq!(next, Some(0)); // Wraps to slot 0
    }

    #[test]
    fn test_next_occupied_slot_none_when_only_one() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s0".into(), 0, "a".into(), Status::Active);

        grid.set_focus(0);
        assert_eq!(grid.next_occupied_slot(), None);
    }

    #[test]
    fn test_add_duplicate_session_tab_returns_none() {
        let mut grid = PaneGrid::new();
        let idx1 = grid.add_terminal("s1".into(), 0, "auth".into(), Status::Active);
        assert_eq!(idx1, Some(0));

        // Same session_id + tab_idx → rejected
        let idx2 = grid.add_terminal("s1".into(), 0, "auth".into(), Status::Active);
        assert_eq!(idx2, None);

        // Same session_id, different tab_idx → allowed
        let idx3 = grid.add_terminal("s1".into(), 1, "auth".into(), Status::Active);
        assert_eq!(idx3, Some(1));
    }

    #[test]
    fn test_toggle_maximize_empty_slot_is_noop() {
        let mut grid = PaneGrid::new();
        grid.toggle_maximize(0); // Slot 0 is empty
        assert_eq!(grid.maximized_slot(), None); // Not maximized
    }

    #[test]
    fn test_toggle_maximize_out_of_bounds_is_noop() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s1".into(), 0, "a".into(), Status::Active);
        grid.toggle_maximize(99); // Way out of bounds
        assert_eq!(grid.maximized_slot(), None);
    }

    #[test]
    fn test_set_focus_empty_slot_is_noop() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s1".into(), 0, "a".into(), Status::Active); // slot 0, focused
        grid.set_focus(1); // Slot 1 is empty — should not change focus
        assert_eq!(grid.focused_slot(), 0);
    }

    #[test]
    fn test_set_focus_out_of_bounds_is_noop() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s1".into(), 0, "a".into(), Status::Active);
        grid.set_focus(99);
        assert_eq!(grid.focused_slot(), 0);
    }

    #[test]
    fn test_prune_moves_focus_from_pruned_slot() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s1".into(), 0, "auth".into(), Status::Active);
        grid.add_terminal("s2".into(), 0, "api".into(), Status::Active);
        grid.set_focus(1); // Focus s2

        let live: std::collections::HashSet<&str> = ["s1"].into_iter().collect();
        grid.prune(&live);

        // s2 was pruned → focus should move to s1 (slot 0)
        assert!(matches!(grid.slot(1), PaneSlot::Empty));
        assert_eq!(grid.focused_slot(), 0);
    }

    #[test]
    fn test_remove_out_of_bounds_is_noop() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s1".into(), 0, "auth".into(), Status::Active);
        grid.remove(99); // Should not panic or change state
        assert!(matches!(grid.slot(0), PaneSlot::Occupied { .. }));
    }

    #[test]
    fn test_remove_maximized_slot_clears_maximize() {
        let mut grid = PaneGrid::new();
        grid.add_terminal("s1".into(), 0, "auth".into(), Status::Active);
        grid.toggle_maximize(0);
        assert_eq!(grid.maximized_slot(), Some(0));

        grid.remove(0);
        assert_eq!(grid.maximized_slot(), None);
    }
}
