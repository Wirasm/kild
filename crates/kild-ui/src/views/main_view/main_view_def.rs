//! MainView struct definition and core initialization.

use gpui::{Context, FocusHandle, Task};

use crate::state::AppState;
use crate::watcher::SessionWatcher;

use super::super::terminal_tabs::TerminalTabs;
use super::keybindings::UiKeybindings;
use super::types::{ActiveView, FocusRegion};

/// Main application view that composes the kild list, header, and create dialog.
///
/// Owns application state and handles keyboard input for the create dialog.
pub struct MainView {
    pub(super) state: AppState,
    pub(super) focus_handle: FocusHandle,
    pub(super) focus_region: FocusRegion,
    pub(super) active_view: ActiveView,
    /// Handle to the background refresh task. Must be stored to prevent cancellation.
    pub(super) _refresh_task: Task<()>,
    /// Handle to the file watcher task. Must be stored to prevent cancellation.
    pub(super) _watcher_task: Task<()>,
    /// Input state for create dialog branch name field.
    pub(super) branch_input: Option<gpui::Entity<gpui_component::input::InputState>>,
    /// Input state for create dialog note field.
    pub(super) note_input: Option<gpui::Entity<gpui_component::input::InputState>>,
    /// Input state for add project dialog path field.
    pub(super) path_input: Option<gpui::Entity<gpui_component::input::InputState>>,
    /// Input state for add project dialog name field.
    pub(super) name_input: Option<gpui::Entity<gpui_component::input::InputState>>,
    /// Cached terminal tabs keyed by session ID. Each kild has its own set of tabs.
    pub(super) terminal_tabs: std::collections::HashMap<String, TerminalTabs>,
    /// Session ID of the kild whose terminal tabs are loaded. May be set while
    /// Dashboard view is active (terminal stays in memory but isn't visible).
    pub(super) active_terminal_id: Option<String>,
    /// Active tab rename: (session_id, tab_index, input entity). Set when user clicks the active tab.
    pub(super) renaming_tab: Option<(
        String,
        usize,
        gpui::Entity<gpui_component::input::InputState>,
    )>,
    /// Whether the daemon is available. None = unknown/not checked.
    pub(super) daemon_available: Option<bool>,
    /// Whether the "+" terminal create menu is open.
    pub(crate) show_add_menu: bool,
    /// Whether a daemon start operation is in progress.
    pub(super) daemon_starting: bool,
    /// Counter for generating unique daemon session IDs within this UI instance.
    #[allow(dead_code)]
    pub(super) daemon_session_counter: u64,
    /// Control view workspaces: each workspace holds a 2x2 pane grid.
    pub(super) workspaces: Vec<super::super::pane_grid::PaneGrid>,
    /// Index of the active workspace in the Control view.
    pub(super) active_workspace: usize,
    /// Parsed keybindings from `~/.kild/keybindings.toml` (or defaults).
    pub(super) keybindings: UiKeybindings,
    /// Agent team manager (owns watcher + cached team state).
    pub(super) team_manager: crate::teams::TeamManager,
    /// Handle to the team watcher task. Must be stored to prevent cancellation.
    pub(super) _team_watcher_task: Task<()>,
}

impl MainView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Get sessions directory for file watcher
        let config = kild_config::Config::new();
        let sessions_dir = config.sessions_dir();

        // Ensure sessions directory exists (create if needed for watcher)
        if !sessions_dir.exists()
            && let Err(e) = std::fs::create_dir_all(&sessions_dir)
        {
            tracing::warn!(
                event = "ui.sessions_dir.create_failed",
                path = %sessions_dir.display(),
                error = %e,
                "Failed to create sessions directory - file watcher may fail to initialize"
            );
        }

        // Try to create file watcher
        let watcher = SessionWatcher::new(&sessions_dir);
        let has_watcher = watcher.is_some();

        // Determine poll interval based on watcher availability
        let poll_interval = if has_watcher {
            crate::refresh::POLL_INTERVAL // 60s with watcher
        } else {
            crate::refresh::FAST_POLL_INTERVAL // 5s fallback
        };

        // Slow poll task (60s with watcher, 5s without)
        let refresh_task = cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            tracing::debug!(
                event = "ui.auto_refresh.started",
                interval_secs = poll_interval.as_secs()
            );

            loop {
                cx.background_executor().timer(poll_interval).await;

                if let Err(e) = this.update(cx, |view, cx| {
                    tracing::debug!(event = "ui.auto_refresh.tick");
                    view.state.update_statuses_only();
                    view.prune_terminal_cache();
                    cx.notify();
                }) {
                    tracing::debug!(
                        event = "ui.auto_refresh.stopped",
                        reason = "view_dropped",
                        error = ?e
                    );
                    break;
                }
            }
        });

        // File watcher task (checks for events frequently, cheap when no events)
        let watcher_task = cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let Some(watcher) = watcher else {
                tracing::debug!(event = "ui.watcher_task.skipped", reason = "no watcher");
                return;
            };

            tracing::debug!(event = "ui.watcher_task.started");
            let mut last_refresh = std::time::Instant::now();
            // Track if events were detected but debounced - ensures we refresh after debounce expires
            let mut pending_refresh = false;

            loop {
                // Check for events every 50ms (cheap - just channel poll)
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;

                if let Err(e) = this.update(cx, |view, cx| {
                    // Check for new events (this drains the queue)
                    if watcher.has_pending_events() {
                        pending_refresh = true;
                    }

                    // Refresh if we have pending events AND debounce period has passed
                    if pending_refresh && last_refresh.elapsed() > crate::refresh::DEBOUNCE_INTERVAL
                    {
                        tracing::info!(event = "ui.watcher.refresh_triggered");
                        view.refresh_and_prune();
                        last_refresh = std::time::Instant::now();
                        pending_refresh = false;
                        cx.notify();
                    }
                }) {
                    tracing::debug!(
                        event = "ui.watcher_task.stopped",
                        reason = "view_dropped",
                        error = ?e
                    );
                    break;
                }
            }
        });

        // Spike 1: Validate smol::Async<UnixStream> on GPUI's BackgroundExecutor
        let spike_task = cx.spawn(async move |_this, cx: &mut gpui::AsyncApp| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::daemon_client::ping_daemon_async().await })
                .await;
            match result {
                Ok(true) => {
                    tracing::info!(
                        event = "ui.daemon.spike_success",
                        "smol async IO works on GPUI executor"
                    );
                }
                Ok(false) => {
                    tracing::warn!(
                        event = "ui.daemon.spike_daemon_not_running",
                        "Daemon not running - spike inconclusive. Start daemon and retry."
                    );
                }
                Err(e) => {
                    tracing::error!(
                        event = "ui.daemon.spike_failed",
                        error = %e,
                        "smol async IO FAILED on GPUI executor - need fallback to dedicated thread"
                    );
                }
            }
        });
        spike_task.detach();

        // Team watcher task: polls TeamManager for file changes
        let team_watcher_task = cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            tracing::debug!(event = "ui.team_watcher_task.started");

            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(200))
                    .await;

                if let Err(e) = this.update(cx, |view, cx| {
                    if view.team_manager.has_pending_events() {
                        tracing::info!(event = "ui.teams.refresh_triggered");

                        // Collect session IDs for cross-referencing
                        let session_ids: Vec<(String, String)> = view
                            .state
                            .displays()
                            .iter()
                            .map(|d| (d.session.id.to_string(), d.session.branch.to_string()))
                            .collect();
                        let refs: Vec<(&str, &str)> = session_ids
                            .iter()
                            .map(|(id, branch)| (id.as_str(), branch.as_str()))
                            .collect();
                        view.team_manager.refresh(&refs);
                        view.sync_teammate_tabs(cx);
                        cx.notify();
                    }
                }) {
                    tracing::debug!(
                        event = "ui.team_watcher_task.stopped",
                        reason = "view_dropped",
                        error = ?e
                    );
                    break;
                }
            }
        });

        // Load keybindings from hierarchy (~/.kild/keybindings.toml → ./.kild/keybindings.toml)
        let raw = kild_core::Keybindings::load_hierarchy();
        let keybindings = UiKeybindings::from_config(&raw);

        let mut view = Self {
            state: AppState::new(),
            focus_handle: cx.focus_handle(),
            focus_region: FocusRegion::Dashboard,
            active_view: ActiveView::Control,
            _refresh_task: refresh_task,
            _watcher_task: watcher_task,
            branch_input: None,
            note_input: None,
            path_input: None,
            name_input: None,
            terminal_tabs: std::collections::HashMap::new(),
            active_terminal_id: None,
            renaming_tab: None,
            daemon_available: None,
            show_add_menu: false,
            daemon_starting: false,
            daemon_session_counter: 1,
            workspaces: vec![super::super::pane_grid::PaneGrid::new()],
            active_workspace: 0,
            keybindings,
            team_manager: crate::teams::TeamManager::new(),
            _team_watcher_task: team_watcher_task,
        };
        view.refresh_daemon_available(cx);
        view
    }

    /// Apply a state mutation and notify GPUI to re-render.
    ///
    /// Use for simple handlers where the entire body is a single state mutation.
    /// For handlers with branching logic, early returns, or multiple mutations,
    /// use explicit `cx.notify()`.
    pub(super) fn mutate_state(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut AppState)) {
        f(&mut self.state);
        cx.notify();
    }

    /// Drop all input state entities (called when any dialog closes).
    pub(super) fn clear_input_entities(&mut self) {
        self.branch_input = None;
        self.note_input = None;
        self.path_input = None;
        self.name_input = None;
    }

    /// Maximum number of workspaces to prevent unbounded creation.
    pub(super) const MAX_WORKSPACES: usize = 10;

    /// Get the active workspace's pane grid.
    pub(super) fn active_pane_grid(&self) -> &super::super::pane_grid::PaneGrid {
        debug_assert!(
            !self.workspaces.is_empty(),
            "workspaces must never be empty"
        );
        debug_assert!(
            self.active_workspace < self.workspaces.len(),
            "active_workspace {} out of bounds (len: {})",
            self.active_workspace,
            self.workspaces.len()
        );
        &self.workspaces[self.active_workspace.min(self.workspaces.len() - 1)]
    }

    /// Get the active workspace's pane grid mutably.
    pub(super) fn active_pane_grid_mut(&mut self) -> &mut super::super::pane_grid::PaneGrid {
        debug_assert!(
            !self.workspaces.is_empty(),
            "workspaces must never be empty"
        );
        debug_assert!(
            self.active_workspace < self.workspaces.len(),
            "active_workspace {} out of bounds (len: {})",
            self.active_workspace,
            self.workspaces.len()
        );
        let idx = self.active_workspace.min(self.workspaces.len() - 1);
        &mut self.workspaces[idx]
    }

    pub(super) fn active_terminal_view(
        &self,
    ) -> Option<&gpui::Entity<crate::terminal::TerminalView>> {
        self.active_terminal_id
            .as_ref()
            .and_then(|id| self.terminal_tabs.get(id))
            .and_then(|tabs| tabs.active_view())
    }

    pub(super) fn refresh_and_prune(&mut self) {
        self.state.refresh_sessions();
        self.prune_terminal_cache();
    }
}
