//! Terminal tab management handlers for MainView.

use gpui::{Context, Focusable, Window, prelude::*};

use super::main_view_def::MainView;
use super::types::FocusRegion;
use crate::views::terminal_tabs::TerminalBackend;

impl MainView {
    pub(super) fn add_terminal_tab(
        &mut self,
        session_id: &str,
        worktree: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match crate::terminal::state::Terminal::new(Some(worktree), cx) {
            Ok(terminal) => {
                let view =
                    cx.new(|cx| crate::terminal::TerminalView::from_terminal(terminal, window, cx));
                let tabs = self
                    .terminal_tabs
                    .entry(session_id.to_string())
                    .or_default();
                tabs.push(view, TerminalBackend::Local);
                true
            }
            Err(e) => {
                tracing::error!(event = "ui.terminal.create_failed", error = %e);
                self.state
                    .push_error(format!("Terminal creation failed: {}", e));
                false
            }
        }
    }

    pub(super) fn focus_active_terminal(&self, window: &mut Window, cx: &gpui::App) {
        if let Some(view) = self.active_terminal_view() {
            let h = view.read(cx).focus_handle(cx).clone();
            window.focus(&h);
        }
    }

    pub(crate) fn refresh_daemon_available(&mut self, cx: &mut Context<Self>) {
        self.daemon_available = None;
        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let available = match cx
                .background_executor()
                .spawn(async { crate::daemon_client::ping_daemon_async().await })
                .await
            {
                Ok(true) => true,
                Ok(false) => false,
                Err(e) => {
                    tracing::warn!(event = "ui.daemon.ping_failed", error = %e);
                    false
                }
            };
            if let Err(e) = this.update(cx, |view, cx| {
                view.daemon_available = Some(available);
                tracing::debug!(
                    event = "ui.daemon.availability_checked",
                    available = available
                );
                cx.notify();
            }) {
                tracing::debug!(event = "ui.refresh_daemon_available.view_dropped", error = ?e);
            }
        })
        .detach();
    }

    pub(super) fn add_daemon_terminal_tab(
        &mut self,
        kild_session_id: &str,
        daemon_session_id: &str,
        cx: &mut Context<Self>,
    ) {
        let kild_id = kild_session_id.to_string();
        let daemon_id = daemon_session_id.to_string();

        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let conn = match cx
                .background_executor()
                .spawn({
                    let daemon_id = daemon_id.clone();
                    async move {
                        crate::daemon_client::connect_for_attach(&daemon_id, 24, 80).await
                    }
                })
                .await
            {
                Ok(conn) => conn,
                Err(e) => {
                    tracing::error!(
                        event = "ui.terminal.daemon_attach_failed",
                        daemon_session_id = daemon_id,
                        error = %e,
                    );
                    if let Err(e) = this.update(cx, |view, cx| {
                        view.state.push_error(format!("Daemon attach failed: {e}"));
                        cx.notify();
                    }) {
                        tracing::debug!(event = "ui.add_daemon_terminal_tab.error_view_dropped", error = ?e);
                    }
                    return;
                }
            };

            if let Err(e) = this.update(cx, |view, cx| {
                let daemon_id_clone = daemon_id.clone();
                match crate::terminal::state::Terminal::from_daemon(daemon_id.clone(), conn, cx) {
                    Ok(terminal) => {
                        let entity = cx.new(|cx| {
                            crate::terminal::TerminalView::from_terminal_unfocused(terminal, cx)
                        });
                        let tabs = view
                            .terminal_tabs
                            .entry(kild_id.clone())
                            .or_default();
                        tabs.push(
                            entity,
                            TerminalBackend::Daemon {
                                daemon_session_id: daemon_id_clone,
                            },
                        );
                        view.active_terminal_id = Some(kild_id);
                        view.focus_region = FocusRegion::Terminal;
                    }
                    Err(e) => {
                        tracing::error!(
                            event = "ui.terminal.daemon_create_failed",
                            error = %e,
                        );
                        view.state
                            .push_error(format!("Daemon terminal failed: {e}"));
                    }
                }
                cx.notify();
            }) {
                tracing::debug!(event = "ui.add_daemon_terminal_tab.view_dropped", error = ?e);
            }
        })
        .detach();
    }

    /// Sync teammate terminal tabs with team manager state.
    ///
    /// For each session with an active team, adds missing teammate tabs
    /// and removes tabs for teammates that are no longer present.
    pub(super) fn sync_teammate_tabs(&mut self, cx: &mut Context<Self>) {
        // First pass: collect all (session_id, teammate) pairs that need new tabs.
        // Must collect all data upfront to avoid borrow conflicts.
        let mut to_add: Vec<(String, String, String, kild_teams::TeamColor)> = Vec::new();

        for display in self.state.displays() {
            let session_id = &display.session.id;
            let teammates = self.team_manager.teammates_for_session(session_id);
            if teammates.is_empty() {
                continue;
            }

            let tabs = self.terminal_tabs.get(&**session_id);

            for member in teammates {
                let Some(daemon_session_id) = &member.daemon_session_id else {
                    continue;
                };
                let already_exists = tabs.is_some_and(|t| t.has_daemon_session(daemon_session_id));
                if already_exists {
                    continue;
                }
                to_add.push((
                    session_id.to_string(),
                    daemon_session_id.clone(),
                    member.name.clone(),
                    member.color,
                ));
            }
        }

        // Second pass: add the tabs
        for (session_id, daemon_session_id, name, color) in to_add {
            tracing::info!(
                event = "ui.teams.adding_teammate_tab",
                session_id = session_id,
                teammate = name,
                daemon_session_id = daemon_session_id
            );
            self.add_teammate_terminal_tab(&session_id, &daemon_session_id, name, color, cx);
        }
    }

    /// Add a teammate terminal tab (attaches to daemon PTY, doesn't steal focus).
    fn add_teammate_terminal_tab(
        &mut self,
        kild_session_id: &str,
        daemon_session_id: &str,
        teammate_name: String,
        color: kild_teams::TeamColor,
        cx: &mut Context<Self>,
    ) {
        let kild_id = kild_session_id.to_string();
        let daemon_id = daemon_session_id.to_string();

        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let conn = match cx
                .background_executor()
                .spawn({
                    let daemon_id = daemon_id.clone();
                    async move {
                        crate::daemon_client::connect_for_attach(&daemon_id, 24, 80).await
                    }
                })
                .await
            {
                Ok(conn) => conn,
                Err(e) => {
                    tracing::warn!(
                        event = "ui.teams.teammate_attach_failed",
                        daemon_session_id = daemon_id,
                        teammate = teammate_name,
                        error = %e,
                    );
                    return;
                }
            };

            if let Err(e) = this.update(cx, |view, cx| {
                let daemon_id_clone = daemon_id.clone();
                let name = teammate_name.clone();
                match crate::terminal::state::Terminal::from_daemon(daemon_id, conn, cx) {
                    Ok(terminal) => {
                        let entity = cx.new(|cx| {
                            crate::terminal::TerminalView::from_terminal_unfocused(terminal, cx)
                        });
                        let tabs = view.terminal_tabs.entry(kild_id.clone()).or_default();
                        tabs.push_teammate(entity, name, color, daemon_id_clone);
                    }
                    Err(e) => {
                        tracing::warn!(
                            event = "ui.teams.teammate_terminal_failed",
                            error = %e,
                        );
                    }
                }
                cx.notify();
            }) {
                tracing::debug!(event = "ui.teams.add_teammate_tab.view_dropped", error = ?e);
            }
        })
        .detach();
    }

    /// Stop a daemon session in the background.
    pub(super) fn stop_daemon_session_async(daemon_session_id: String, cx: &mut Context<MainView>) {
        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let dsid = daemon_session_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { crate::daemon_client::stop_session_async(&dsid).await })
                .await;
            if let Err(e) = result {
                tracing::warn!(
                    event = "ui.terminal.daemon_session_stop_failed",
                    daemon_session_id = daemon_session_id,
                    error = %e,
                );
                let _ = this.update(cx, |view, cx| {
                    view.state
                        .push_error(format!("Failed to clean up daemon session: {e}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    #[allow(dead_code)]
    pub(crate) fn on_add_local_tab(
        &mut self,
        session_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(display) = self
            .state
            .displays()
            .iter()
            .find(|d| &*d.session.id == session_id)
        else {
            return;
        };
        let worktree = display.session.worktree_path.clone();
        self.add_terminal_tab(session_id, worktree, window, cx);
        self.focus_active_terminal(window, cx);
        self.show_add_menu = false;
        cx.notify();
    }

    #[allow(dead_code)]
    pub(crate) fn on_add_daemon_tab(&mut self, session_id: &str, cx: &mut Context<Self>) {
        let Some(display) = self
            .state
            .displays()
            .iter()
            .find(|d| &*d.session.id == session_id)
        else {
            return;
        };
        let is_running = display.process_status == kild_core::ProcessStatus::Running;
        let daemon_session_id = display
            .session
            .agents()
            .iter()
            .find_map(|a| a.daemon_session_id().map(|s| s.to_string()));
        let worktree = display.session.worktree_path.clone();
        let kild_id = session_id.to_string();

        self.show_add_menu = false;

        if is_running && let Some(dsid) = daemon_session_id {
            self.add_daemon_terminal_tab(session_id, &dsid, cx);
        } else {
            // No existing daemon session — create one on the fly
            let worktree_str = worktree.display().to_string();
            let counter = self.daemon_session_counter;
            self.daemon_session_counter += 1;
            let daemon_session_id = format!("{}_ui_shell_{}", kild_id, counter);
            let kild_id_for_tab = kild_id.clone();

            cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
                let result = cx
                    .background_executor()
                    .spawn({
                        let dsid = daemon_session_id.clone();
                        let wd = worktree_str.clone();
                        async move { crate::daemon_client::create_session_async(&dsid, &wd).await }
                    })
                    .await;

                match result {
                    Ok(created_dsid) => {
                        if let Err(e) = this.update(cx, |view, cx| {
                            view.add_daemon_terminal_tab(&kild_id_for_tab, &created_dsid, cx);
                        }) {
                            tracing::debug!(event = "ui.on_add_daemon_tab.ok_view_dropped", error = ?e);
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            event = "ui.terminal.daemon_create_session_failed",
                            error = %e,
                        );
                        if let Err(e) = this.update(cx, |view, cx| {
                            view.state
                                .push_error(format!("Failed to create daemon session: {e}"));
                            cx.notify();
                        }) {
                            tracing::debug!(event = "ui.on_add_daemon_tab.err_view_dropped", error = ?e);
                        }
                    }
                }
            })
            .detach();
        }
        cx.notify();
    }

    pub(crate) fn on_start_daemon(&mut self, cx: &mut Context<Self>) {
        if self.daemon_starting || self.daemon_available == Some(true) {
            return;
        }
        tracing::info!(event = "ui.daemon.start_requested");
        self.daemon_starting = true;
        self.daemon_available = None;
        cx.notify();

        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let result = cx
                .background_executor()
                .spawn(async {
                    let config = match kild_core::config::KildConfig::load_hierarchy() {
                        Ok(cfg) => cfg,
                        Err(e) => {
                            tracing::warn!(
                                event = "ui.daemon.config_load_failed",
                                error = %e,
                                "Using default config"
                            );
                            kild_core::config::KildConfig::default()
                        }
                    };
                    kild_core::daemon::autostart::ensure_daemon_running(&config)
                })
                .await;

            if let Err(e) = this.update(cx, |view, cx| {
                view.daemon_starting = false;
                match result {
                    Ok(()) => {
                        tracing::info!(event = "ui.daemon.start_completed");
                        view.daemon_available = Some(true);
                    }
                    Err(e) => {
                        tracing::error!(event = "ui.daemon.start_failed", error = %e);
                        view.daemon_available = Some(false);
                        view.state
                            .push_error(format!("Failed to start daemon: {e}"));
                    }
                }
                cx.notify();
            }) {
                tracing::debug!(event = "ui.on_start_daemon.view_dropped", error = ?e);
            }
        })
        .detach();
    }

    pub(super) fn prune_terminal_cache(&mut self) {
        let live_ids: std::collections::HashSet<&str> = self
            .state
            .displays()
            .iter()
            .map(|d| &*d.session.id)
            .collect();

        self.terminal_tabs
            .retain(|id, _| live_ids.contains(id.as_str()));

        for ws in &mut self.workspaces {
            ws.prune(&live_ids);
        }

        if self
            .active_terminal_id
            .as_deref()
            .is_some_and(|id| !live_ids.contains(id))
        {
            self.active_terminal_id = None;
            self.focus_region = FocusRegion::Dashboard;
        }
    }

    #[allow(dead_code)]
    pub(crate) fn on_select_tab(
        &mut self,
        session_id: &str,
        idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_add_menu = false;
        let is_already_active = self
            .terminal_tabs
            .get(session_id)
            .is_some_and(|tabs| tabs.active_index() == idx);

        if is_already_active {
            self.start_rename(session_id, idx, window, cx);
            return;
        }

        if let Some(tabs) = self.terminal_tabs.get_mut(session_id) {
            tabs.set_active(idx);
        }
        self.focus_active_terminal(window, cx);
        cx.notify();
    }

    /// Remove terminal from pane grid without killing it.
    pub(crate) fn on_minimize_tab(
        &mut self,
        session_id: &str,
        tab_idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(slot_idx) = self.active_pane_grid().find_slot(session_id, tab_idx) {
            self.active_pane_grid_mut().remove(slot_idx);

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
                self.active_view = super::types::ActiveView::Dashboard;
                self.focus_region = FocusRegion::Dashboard;
                window.focus(&self.focus_handle);
            }
            cx.notify();
        }
    }

    #[allow(dead_code)]
    pub(crate) fn on_close_tab(
        &mut self,
        session_id: &str,
        idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_add_menu = false;
        if let Some(tabs) = self.terminal_tabs.get_mut(session_id) {
            if let Some(daemon_id) = tabs.close(idx) {
                Self::stop_daemon_session_async(daemon_id, cx);
            }
            if tabs.is_empty() {
                self.active_terminal_id = None;
                self.focus_region = FocusRegion::Dashboard;
                window.focus(&self.focus_handle);
                cx.notify();
                return;
            }
        }
        self.focus_active_terminal(window, cx);
        cx.notify();
    }
}
