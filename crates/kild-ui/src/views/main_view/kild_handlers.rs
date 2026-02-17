//! Kild selection, open, stop, and action handlers for MainView.

use gpui::{Context, Window};

use crate::actions;

use super::main_view_def::MainView;
use super::types::{ActiveView, FocusRegion};

impl MainView {
    /// Handle kild row click - select and open its terminal in Control view.
    pub fn on_kild_select(
        &mut self,
        session_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(event = "ui.kild.selected", session_id = session_id);
        let id = session_id.to_string();
        self.state.select_kild(id.clone());
        self.active_view = ActiveView::Control;

        let Some(display) = self.state.selected_kild() else {
            cx.notify();
            return;
        };
        let worktree = display.session.worktree_path.clone();
        let runtime_mode = display.session.runtime_mode.clone();
        let branch = display.session.branch.clone();
        let status = super::super::pane_grid::process_status_to_status(display.process_status);
        let daemon_session_id = display
            .session
            .agents()
            .iter()
            .find_map(|a| a.daemon_session_id().map(|s| s.to_string()));

        if let Some(tabs) = self.terminal_tabs.get_mut(&id)
            && tabs.has_exited_active(cx)
            && let Some(daemon_id) = tabs.close(tabs.active_index())
        {
            Self::stop_daemon_session_async(daemon_id, cx);
        }

        if self.terminal_tabs.get(&id).is_none_or(|t| t.is_empty()) {
            if matches!(
                runtime_mode,
                Some(kild_core::state::types::RuntimeMode::Daemon)
            ) && display.process_status == kild_core::ProcessStatus::Running
                && let Some(ref dsid) = daemon_session_id
            {
                self.active_terminal_id = Some(id.clone());
                self.focus_region = FocusRegion::Terminal;
                self.add_daemon_terminal_tab(&id, dsid, cx);
                // Place in pane grid (will be available once daemon attach completes)
                self.place_in_pane_grid(&id, 0, &branch, status);
                cx.notify();
                return;
            }
            if !self.add_terminal_tab(&id, worktree, window, cx) {
                cx.notify();
                return;
            }
        }

        // Place terminal in pane grid
        self.place_in_pane_grid(&id, 0, &branch, status);

        self.active_terminal_id = Some(id);
        self.focus_region = FocusRegion::Terminal;
        self.focus_active_terminal(window, cx);
        cx.notify();
    }

    /// Handle click on a terminal name nested under a kild in the sidebar.
    /// Toggles: if already in pane grid, remove it; otherwise place it.
    pub fn on_sidebar_terminal_click(
        &mut self,
        session_id: &str,
        tab_idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.select_kild(session_id.to_string());
        self.active_view = ActiveView::Control;

        // Toggle: if already in grid, remove it
        if let Some(slot_idx) = self.active_pane_grid().find_slot(session_id, tab_idx) {
            self.active_pane_grid_mut().remove(slot_idx);

            // Move focus to next occupied pane or unfocus
            if let Some(next) = self.active_pane_grid().next_occupied_slot() {
                self.active_pane_grid_mut().set_focus(next);
                if let super::super::pane_grid::PaneSlot::Occupied {
                    session_id: next_sid,
                    ..
                } = self.active_pane_grid().slot(next)
                {
                    self.active_terminal_id = Some(next_sid.clone());
                }
            } else {
                self.active_terminal_id = None;
                self.active_view = ActiveView::Dashboard;
                self.focus_region = FocusRegion::Dashboard;
                window.focus(&self.focus_handle);
            }
            cx.notify();
            return;
        }

        // Not in grid — place it
        if let Some(tabs) = self.terminal_tabs.get_mut(session_id) {
            tabs.set_active(tab_idx);
        }

        let (branch, status) = self
            .state
            .displays()
            .iter()
            .find(|d| &*d.session.id == session_id)
            .map(|d| {
                (
                    d.session.branch.to_string(),
                    super::super::pane_grid::process_status_to_status(d.process_status),
                )
            })
            .unwrap_or_else(|| (String::new(), crate::components::Status::Stopped));

        self.place_in_pane_grid(session_id, tab_idx, &branch, status);

        self.active_terminal_id = Some(session_id.to_string());
        self.focus_region = FocusRegion::Terminal;
        self.focus_active_terminal(window, cx);
        cx.notify();
    }

    /// Handle dashboard card click — select kild and switch to Detail view.
    pub(crate) fn on_dashboard_card_click(&mut self, session_id: &str, cx: &mut Context<Self>) {
        tracing::debug!(event = "ui.dashboard.card_clicked", session_id = session_id);
        self.state.select_kild(session_id.to_string());
        self.active_view = ActiveView::Detail;
        self.focus_region = FocusRegion::Dashboard;
        cx.notify();
    }

    /// Handle Detail view back button — return to Dashboard.
    pub(crate) fn on_detail_back(&mut self, cx: &mut Context<Self>) {
        tracing::debug!(event = "ui.detail.back_clicked");
        self.active_view = ActiveView::Dashboard;
        cx.notify();
    }

    /// Handle terminal click in Detail view — switch to Control with terminal focused.
    pub(crate) fn on_detail_terminal_click(
        &mut self,
        session_id: &str,
        tab_idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.on_sidebar_terminal_click(session_id, tab_idx, window, cx);
    }

    /// Handle click on the Open button [▶] in a kild row.
    ///
    /// Spawns the blocking open_kild operation on the background executor.
    pub fn on_open_click(&mut self, branch: &str, cx: &mut Context<Self>) {
        if self.state.is_loading(branch) {
            return;
        }
        tracing::info!(event = "ui.open_clicked", branch = branch);
        self.state.clear_error(branch);
        self.state.set_loading(branch);
        cx.notify();
        let branch = branch.to_string();

        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let branch_for_action = branch.clone();
            let result = cx
                .background_executor()
                .spawn(async move { actions::open_kild(branch_for_action, None) })
                .await;

            if let Err(e) = this.update(cx, |view, cx| {
                view.state.clear_loading(&branch);
                match result {
                    Ok(events) => {
                        view.state.apply_events(&events);
                        view.prune_terminal_cache();
                    }
                    Err(e) => {
                        tracing::warn!(event = "ui.open_click.error_displayed", branch = %branch, error = %e);
                        view.state.set_error(
                            &branch,
                            crate::state::OperationError { message: e },
                        );
                    }
                }
                cx.notify();
            }) {
                tracing::debug!(
                    event = "ui.open_click.view_dropped",
                    error = ?e,
                );
            }
        })
        .detach();
    }

    /// Handle click on the Stop button [⏹] in a kild row.
    ///
    /// Spawns the blocking stop_kild operation on the background executor.
    pub fn on_stop_click(&mut self, branch: &str, cx: &mut Context<Self>) {
        if self.state.is_loading(branch) {
            return;
        }
        tracing::info!(event = "ui.stop_clicked", branch = branch);
        self.state.clear_error(branch);
        self.state.set_loading(branch);
        cx.notify();
        let branch = branch.to_string();

        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let branch_for_action = branch.clone();
            let result = cx
                .background_executor()
                .spawn(async move { actions::stop_kild(branch_for_action) })
                .await;

            if let Err(e) = this.update(cx, |view, cx| {
                view.state.clear_loading(&branch);
                match result {
                    Ok(events) => {
                        view.state.apply_events(&events);
                        view.prune_terminal_cache();
                    }
                    Err(e) => {
                        tracing::warn!(event = "ui.stop_click.error_displayed", branch = %branch, error = %e);
                        view.state.set_error(
                            &branch,
                            crate::state::OperationError { message: e },
                        );
                    }
                }
                cx.notify();
            }) {
                tracing::debug!(
                    event = "ui.stop_click.view_dropped",
                    error = ?e,
                );
            }
        })
        .detach();
    }

    /// Handle click on the Copy Path button in a kild row.
    ///
    /// Copies the worktree path to the system clipboard.
    #[allow(dead_code)]
    pub fn on_copy_path_click(&mut self, worktree_path: &std::path::Path, cx: &mut Context<Self>) {
        tracing::info!(
            event = "ui.copy_path_clicked",
            path = %worktree_path.display()
        );
        let path_str = worktree_path.display().to_string();
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(path_str));
    }

    /// Handle click on the Open Editor button in a kild row.
    ///
    /// Opens the worktree in the user's preferred editor ($EDITOR or zed).
    /// Surfaces any errors inline in the kild row.
    pub fn on_open_editor_click(
        &mut self,
        worktree_path: &std::path::Path,
        branch: &str,
        cx: &mut Context<Self>,
    ) {
        tracing::info!(
            event = "ui.open_editor_clicked",
            path = %worktree_path.display()
        );
        self.state.clear_error(branch);

        if let Err(e) = actions::open_in_editor(worktree_path) {
            tracing::warn!(
                event = "ui.open_editor_click.error_displayed",
                branch = branch,
                error = %e
            );
            self.state
                .set_error(branch, crate::state::OperationError { message: e });
        }
        cx.notify();
    }

    /// Handle click on the Focus Terminal button in a kild row.
    ///
    /// Requires both `terminal_type` and `window_id` to be present. If either is
    /// missing (e.g., session started before window tracking was implemented),
    /// surfaces an error to the user explaining the limitation.
    ///
    /// Also surfaces any errors from the underlying `focus_terminal` operation.
    #[allow(dead_code)]
    pub fn on_focus_terminal_click(
        &mut self,
        terminal_type: Option<&kild_core::terminal::types::TerminalType>,
        window_id: Option<&str>,
        branch: &str,
        cx: &mut Context<Self>,
    ) {
        tracing::info!(
            event = "ui.focus_terminal_clicked",
            branch = branch,
            terminal_type = ?terminal_type,
            window_id = ?window_id
        );
        self.state.clear_error(branch);

        // Validate we have both terminal type and window ID
        let Some(tt) = terminal_type else {
            self.record_error(branch, "Terminal window info not available. This session was created before window tracking was added.", cx);
            return;
        };

        let Some(wid) = window_id else {
            self.record_error(branch, "Terminal window info not available. This session was created before window tracking was added.", cx);
            return;
        };

        // Both fields present - attempt to focus terminal
        if let Err(e) = kild_core::terminal_ops::focus_terminal(tt, wid) {
            let message = format!("Failed to focus terminal: {}", e);
            self.record_error(branch, &message, cx);
        }
    }

    /// Record an operation error for a branch and notify the UI.
    #[allow(dead_code)]
    pub(super) fn record_error(&mut self, branch: &str, message: &str, cx: &mut Context<Self>) {
        tracing::warn!(
            event = "ui.operation.error_displayed",
            branch = branch,
            error = message
        );
        self.state.set_error(
            branch,
            crate::state::OperationError {
                message: message.to_string(),
            },
        );
        cx.notify();
    }

    /// Clear startup errors (called when user dismisses the banner).
    pub(super) fn on_dismiss_errors(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.errors.dismissed");
        self.mutate_state(cx, |s| s.dismiss_errors());
    }
}
