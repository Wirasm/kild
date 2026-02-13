//! Main view for kild-ui.
//!
//! Root view that composes header, kild list, create dialog, and confirm dialog.
//! Handles keyboard input and dialog state management.

use gpui::{
    Context, FocusHandle, Focusable, FontWeight, IntoElement, KeyDownEvent, Render, Task, Window,
    div, prelude::*, px,
};

use crate::theme;
use gpui_component::Disableable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::InputState;
use tracing::{debug, warn};

use std::path::PathBuf;

use crate::actions;
use crate::state::AppState;
use crate::views::{
    add_project_dialog, confirm_dialog, create_dialog, dashboard_view, detail_view, project_rail,
    sidebar,
    terminal_tabs::{TerminalBackend, TerminalTabs},
};
use crate::watcher::SessionWatcher;

/// Normalize user-entered path for project addition.
///
/// Handles:
/// - Whitespace trimming (leading/trailing spaces removed)
/// - Tilde expansion (~/ -> home directory, or ~ alone)
/// - Missing leading slash (users/... -> /users/... if valid directory)
/// - Path canonicalization (resolves symlinks, normalizes case on macOS)
///
/// # Errors
///
/// Returns an error if:
/// - Path starts with `~` but home directory cannot be determined
/// - Checking directory existence fails due to permission or I/O error
fn normalize_project_path(path_str: &str) -> Result<PathBuf, String> {
    let path_str = path_str.trim();

    // Handle tilde expansion
    if path_str.starts_with('~') {
        let Some(home) = dirs::home_dir() else {
            warn!(
                event = "ui.normalize_path.home_dir_unavailable",
                path = path_str,
                "dirs::home_dir() returned None - HOME environment variable may be unset"
            );
            return Err("Could not determine home directory. Is $HOME set?".to_string());
        };

        if let Some(rest) = path_str.strip_prefix("~/") {
            return canonicalize_path(home.join(rest));
        }
        if path_str == "~" {
            return canonicalize_path(home);
        }
        // Tilde in middle like "~project" - no expansion, fall through
    }

    // Handle missing leading slash - only if path looks absolute without the /
    // e.g., "users/rasmus/project" -> "/users/rasmus/project" (if that directory exists)
    if !path_str.starts_with('/') && !path_str.starts_with('~') && !path_str.is_empty() {
        let with_slash = PathBuf::from(format!("/{}", path_str));

        match std::fs::metadata(&with_slash) {
            Ok(meta) if meta.is_dir() => {
                debug!(
                    event = "ui.normalize_path.slash_prefix_applied",
                    original = path_str,
                    normalized = %with_slash.display()
                );
                return canonicalize_path(with_slash);
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                warn!(
                    event = "ui.normalize_path.slash_prefix_check_failed",
                    path = %with_slash.display(),
                    error = %e
                );
                return Err(format!("Cannot access '{}': {}", with_slash.display(), e));
            }
            _ => {
                // Path doesn't exist or exists but isn't a directory - fall through
            }
        }
    }

    canonicalize_path(PathBuf::from(path_str))
}

/// Canonicalize a path to ensure consistent hashing across UI and core.
///
/// This resolves symlinks and normalizes case on case-insensitive filesystems (macOS).
/// Canonicalization ensures that `/users/rasmus/project` and `/Users/rasmus/project`
/// produce the same hash value, which is critical for project filtering.
///
/// # Errors
/// Returns an error if the path doesn't exist or is inaccessible.
fn canonicalize_path(path: PathBuf) -> Result<PathBuf, String> {
    match path.canonicalize() {
        Ok(canonical) => {
            if canonical != path {
                debug!(
                    event = "ui.normalize_path.canonicalized",
                    original = %path.display(),
                    canonical = %canonical.display()
                );
            }
            Ok(canonical)
        }
        Err(e) => {
            warn!(
                event = "ui.normalize_path.canonicalize_failed",
                path = %path.display(),
                error = %e
            );
            Err(format!("Cannot access '{}': {}", path.display(), e))
        }
    }
}

/// Tracks which region of the UI currently has logical focus.
///
/// Used for keyboard routing — determines where key events are dispatched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusRegion {
    Dashboard,
    Terminal,
}

/// Which view is showing in the main area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveView {
    /// Terminal tabs per kild (default).
    Control,
    /// Fleet overview with kild cards.
    Dashboard,
    /// Kild detail drill-down (from dashboard card click).
    Detail,
}

/// Main application view that composes the kild list, header, and create dialog.
///
/// Owns application state and handles keyboard input for the create dialog.
pub struct MainView {
    state: AppState,
    focus_handle: FocusHandle,
    focus_region: FocusRegion,
    active_view: ActiveView,
    /// Handle to the background refresh task. Must be stored to prevent cancellation.
    _refresh_task: Task<()>,
    /// Handle to the file watcher task. Must be stored to prevent cancellation.
    _watcher_task: Task<()>,
    /// Input state for create dialog branch name field.
    branch_input: Option<gpui::Entity<InputState>>,
    /// Input state for create dialog note field.
    note_input: Option<gpui::Entity<InputState>>,
    /// Input state for add project dialog path field.
    path_input: Option<gpui::Entity<InputState>>,
    /// Input state for add project dialog name field.
    name_input: Option<gpui::Entity<InputState>>,
    /// Cached terminal tabs keyed by session ID. Each kild has its own set of tabs.
    terminal_tabs: std::collections::HashMap<String, TerminalTabs>,
    /// Session ID of the kild whose terminal tabs are loaded. May be set while
    /// Dashboard view is active (terminal stays in memory but isn't visible).
    active_terminal_id: Option<String>,
    /// Active tab rename: (session_id, tab_index, input entity). Set when user clicks the active tab.
    renaming_tab: Option<(String, usize, gpui::Entity<InputState>)>,
    /// Whether the daemon is available. None = unknown/not checked.
    daemon_available: Option<bool>,
    /// Whether the "+" terminal create menu is open.
    pub(crate) show_add_menu: bool,
    /// Whether a daemon start operation is in progress.
    daemon_starting: bool,
    /// Counter for generating unique daemon session IDs within this UI instance.
    #[allow(dead_code)]
    daemon_session_counter: u64,
    /// 2x2 pane grid for Control view multi-terminal layout.
    pane_grid: super::pane_grid::PaneGrid,
}

impl MainView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Get sessions directory for file watcher
        let config = kild_core::config::Config::new();
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
            pane_grid: super::pane_grid::PaneGrid::new(),
        };
        view.refresh_daemon_available(cx);
        view
    }

    /// Apply a state mutation and notify GPUI to re-render.
    ///
    /// Use for simple handlers where the entire body is a single state mutation.
    /// For handlers with branching logic, early returns, or multiple mutations,
    /// use explicit `cx.notify()`.
    fn mutate_state(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut AppState)) {
        f(&mut self.state);
        cx.notify();
    }

    /// Drop all input state entities (called when any dialog closes).
    fn clear_input_entities(&mut self) {
        self.branch_input = None;
        self.note_input = None;
        self.path_input = None;
        self.name_input = None;
    }

    fn active_terminal_view(&self) -> Option<&gpui::Entity<crate::terminal::TerminalView>> {
        self.active_terminal_id
            .as_ref()
            .and_then(|id| self.terminal_tabs.get(id))
            .and_then(|tabs| tabs.active_view())
    }

    fn prune_terminal_cache(&mut self) {
        let live_ids: std::collections::HashSet<&str> = self
            .state
            .displays()
            .iter()
            .map(|d| d.session.id.as_str())
            .collect();

        self.terminal_tabs
            .retain(|id, _| live_ids.contains(id.as_str()));

        self.pane_grid.prune(&live_ids);

        if self
            .active_terminal_id
            .as_deref()
            .is_some_and(|id| !live_ids.contains(id))
        {
            self.active_terminal_id = None;
            self.focus_region = FocusRegion::Dashboard;
        }
    }

    fn refresh_and_prune(&mut self) {
        self.state.refresh_sessions();
        self.prune_terminal_cache();
    }

    /// Handle click on the Create button in header.
    fn on_create_button_click(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.create_dialog.opened");
        self.state.open_create_dialog();

        let branch_pattern =
            regex::Regex::new(r"^[a-zA-Z0-9\-_/]*$").expect("branch name regex is valid");
        let branch_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Type branch name...")
                .pattern(branch_pattern)
        });
        self.branch_input = Some(branch_input);

        let note_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("What is this kild for?")
                .validate(|text, _| !text.chars().any(|c| c.is_control()))
        });
        self.note_input = Some(note_input);

        cx.notify();
    }

    /// Handle dialog cancel button click (create dialog).
    pub fn on_dialog_cancel(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.create_dialog.cancelled");
        self.clear_input_entities();
        self.mutate_state(cx, |s| s.close_dialog());
    }

    /// Handle dialog submit button click (create dialog).
    ///
    /// Spawns the blocking create_kild operation on the background executor
    /// so the UI remains responsive during git worktree creation and terminal spawn.
    pub fn on_dialog_submit(&mut self, cx: &mut Context<Self>) {
        if self.state.is_dialog_loading() {
            return;
        }

        // Extract agent from dialog state
        let crate::state::DialogState::Create { form, .. } = self.state.dialog() else {
            tracing::error!(
                event = "ui.dialog_submit.invalid_state",
                "on_dialog_submit called when Create dialog not open"
            );
            return;
        };
        let agent = form.selected_agent();

        // Read text values from InputState entities
        let branch = self
            .branch_input
            .as_ref()
            .map(|i| i.read(cx).value().to_string())
            .unwrap_or_default();
        let branch = branch.trim().to_string();
        let note_text = self
            .note_input
            .as_ref()
            .map(|i| i.read(cx).value().to_string())
            .unwrap_or_default();
        let note = if note_text.trim().is_empty() {
            None
        } else {
            Some(note_text.trim().to_string())
        };

        // Get active project path for kild creation context
        let project_path = self.state.active_project_path().map(|p| p.to_path_buf());

        // Warn if no project selected (shouldn't happen with current UI flow)
        if project_path.is_none() {
            tracing::warn!(
                event = "ui.dialog_submit.no_active_project",
                message = "Creating kild without active project - will will use cwd detection"
            );
        }

        self.state.set_dialog_loading();
        cx.notify();

        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let result = cx
                .background_executor()
                .spawn(async move { actions::create_kild(branch, agent, note, project_path) })
                .await;

            if let Err(e) = this.update(cx, |view, cx| {
                view.state.clear_dialog_loading();
                match result {
                    Ok(events) => {
                        view.state.apply_events(&events);
                        view.prune_terminal_cache();
                    }
                    Err(e) => {
                        tracing::warn!(event = "ui.dialog_submit.error_displayed", error = %e);
                        view.state.set_dialog_error(e);
                    }
                }
                cx.notify();
            }) {
                tracing::debug!(
                    event = "ui.dialog_submit.view_dropped",
                    error = ?e,
                );
            }
        })
        .detach();
    }

    /// Cycle to the next agent in the list.
    pub fn on_agent_cycle(&mut self, cx: &mut Context<Self>) {
        let agents = create_dialog::agent_options();
        if agents.is_empty() {
            tracing::error!(event = "ui.create_dialog.no_agents_available");
            self.state.set_dialog_error(
                "No agents available. Check kild-core configuration.".to_string(),
            );
            cx.notify();
            return;
        }

        // Update selected agent index in dialog state
        if let crate::state::DialogState::Create { form, .. } = self.state.dialog_mut() {
            let next_index = (form.selected_agent_index() + 1) % agents.len();
            form.set_selected_agent_index(next_index);
            tracing::info!(
                event = "ui.create_dialog.agent_changed",
                agent = %form.selected_agent()
            );
        }
        cx.notify();
    }

    /// Handle click on the Refresh button in header.
    fn on_refresh_click(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.refresh_clicked");
        self.refresh_and_prune();
        cx.notify();
    }

    /// Handle click on the destroy button [×] in a kild row.
    #[allow(dead_code)]
    pub fn on_destroy_click(&mut self, branch: &str, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.destroy_dialog.opened", branch = branch);
        let branch = branch.to_string();
        self.mutate_state(cx, |s| s.open_confirm_dialog(branch));
    }

    /// Handle confirm button click in destroy dialog.
    ///
    /// Spawns the blocking destroy_kild operation on the background executor
    /// so the UI remains responsive during worktree removal and process termination.
    pub fn on_confirm_destroy(&mut self, cx: &mut Context<Self>) {
        if self.state.is_dialog_loading() {
            return;
        }

        // Extract branch and safety_info from dialog state
        let crate::state::DialogState::Confirm {
            branch,
            safety_info,
            ..
        } = self.state.dialog()
        else {
            tracing::warn!(event = "ui.confirm_destroy.no_target");
            return;
        };
        let branch = branch.clone();

        // Use force=true if safety_info indicates blocking (user clicked "Force Destroy")
        let force = safety_info
            .as_ref()
            .map(|s| s.should_block())
            .unwrap_or(false);

        self.state.set_dialog_loading();
        cx.notify();

        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let result = cx
                .background_executor()
                .spawn(async move { actions::destroy_kild(branch, force) })
                .await;

            if let Err(e) = this.update(cx, |view, cx| {
                view.state.clear_dialog_loading();
                match result {
                    Ok(events) => {
                        view.state.apply_events(&events);
                        view.prune_terminal_cache();
                    }
                    Err(e) => {
                        tracing::warn!(event = "ui.confirm_destroy.error_displayed", error = %e);
                        view.state.set_dialog_error(e);
                    }
                }
                cx.notify();
            }) {
                tracing::debug!(
                    event = "ui.confirm_destroy.view_dropped",
                    error = ?e,
                );
            }
        })
        .detach();
    }

    /// Handle cancel button click in destroy dialog.
    pub fn on_confirm_cancel(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.confirm_dialog.cancelled");
        self.mutate_state(cx, |s| s.close_dialog());
    }

    fn add_terminal_tab(
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

    fn focus_active_terminal(&self, window: &mut Window, cx: &gpui::App) {
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

    fn add_daemon_terminal_tab(
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

    /// Stop a daemon session in the background (best-effort cleanup).
    fn stop_daemon_session_async(daemon_session_id: String, cx: &mut Context<MainView>) {
        cx.spawn(async move |_this, cx: &mut gpui::AsyncApp| {
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
                    "Best-effort daemon session cleanup failed"
                );
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
            .find(|d| d.session.id == session_id)
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
            .find(|d| d.session.id == session_id)
        else {
            return;
        };
        let daemon_session_id = display
            .session
            .agents()
            .iter()
            .find_map(|a| a.daemon_session_id().map(|s| s.to_string()));
        let worktree = display.session.worktree_path.clone();
        let kild_id = session_id.to_string();

        self.show_add_menu = false;

        if let Some(dsid) = daemon_session_id {
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

    /// Navigate to the next kild in the filtered list (wrapping).
    fn navigate_next_kild(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let displays = self.state.filtered_displays();
        if displays.is_empty() {
            return;
        }
        let current_idx = self
            .state
            .selected_id()
            .and_then(|id| displays.iter().position(|d| d.session.id == id));
        let next_idx = match current_idx {
            Some(idx) => (idx + 1) % displays.len(),
            None => 0,
        };
        tracing::debug!(
            event = "ui.kild.navigate_next",
            from = ?self.state.selected_id(),
            to_idx = next_idx
        );
        let next_id = displays[next_idx].session.id.clone();
        self.on_kild_select(&next_id, window, cx);
    }

    /// Navigate to the previous kild in the filtered list (wrapping).
    fn navigate_prev_kild(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let displays = self.state.filtered_displays();
        if displays.is_empty() {
            return;
        }
        let current_idx = self
            .state
            .selected_id()
            .and_then(|id| displays.iter().position(|d| d.session.id == id));
        let prev_idx = match current_idx {
            Some(0) | None => displays.len() - 1,
            Some(idx) => idx - 1,
        };
        tracing::debug!(
            event = "ui.kild.navigate_prev",
            from = ?self.state.selected_id(),
            to_idx = prev_idx
        );
        let prev_id = displays[prev_idx].session.id.clone();
        self.on_kild_select(&prev_id, window, cx);
    }

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
        let status = super::pane_grid::process_status_to_status(display.process_status);
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
            ) && let Some(ref dsid) = daemon_session_id
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
    /// Places the terminal in the pane grid and focuses it.
    pub fn on_sidebar_terminal_click(
        &mut self,
        session_id: &str,
        tab_idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.select_kild(session_id.to_string());
        self.active_view = ActiveView::Control;
        if let Some(tabs) = self.terminal_tabs.get_mut(session_id) {
            tabs.set_active(tab_idx);
        }

        // Get branch and status for pane grid
        let (branch, status) = self
            .state
            .displays()
            .iter()
            .find(|d| d.session.id == session_id)
            .map(|d| {
                (
                    d.session.branch.clone(),
                    super::pane_grid::process_status_to_status(d.process_status),
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

    /// Reset the pane grid and auto-populate from current displays.
    fn reset_pane_grid(&mut self) {
        self.pane_grid = super::pane_grid::PaneGrid::new();
        let displays = self.state.filtered_displays();
        let displays_owned: Vec<kild_core::SessionInfo> = displays.into_iter().cloned().collect();
        self.pane_grid
            .auto_populate(&displays_owned, &self.terminal_tabs);
    }

    /// Place a terminal in the pane grid. If already present, just focus it.
    /// If grid is full, replace the least-recently-focused pane.
    fn place_in_pane_grid(
        &mut self,
        session_id: &str,
        tab_idx: usize,
        branch: &str,
        status: crate::components::Status,
    ) {
        // Already in grid? Just focus it.
        if let Some(slot_idx) = self.pane_grid.find_slot(session_id, tab_idx) {
            self.pane_grid.set_focus(slot_idx);
            return;
        }

        // Try to add to an empty slot.
        if self
            .pane_grid
            .add_terminal(session_id.to_string(), tab_idx, branch.to_string(), status)
            .is_some()
        {
            return;
        }

        // Grid full — replace LRU slot.
        let lru = self.pane_grid.least_recently_focused();
        self.pane_grid.remove(lru);
        if self
            .pane_grid
            .add_terminal(session_id.to_string(), tab_idx, branch.to_string(), status)
            .is_none()
        {
            tracing::error!(
                event = "ui.pane_grid.add_after_lru_remove_failed",
                session_id = session_id,
                lru_slot = lru,
            );
        }
    }

    /// Handle click inside a pane to focus it.
    pub fn on_pane_focus(&mut self, slot_idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.pane_grid.set_focus(slot_idx);

        if let super::pane_grid::PaneSlot::Occupied {
            session_id,
            tab_idx,
            ..
        } = self.pane_grid.slot(slot_idx)
        {
            self.active_terminal_id = Some(session_id.clone());
            if let Some(tabs) = self.terminal_tabs.get_mut(session_id) {
                tabs.set_active(*tab_idx);
            }
            self.focus_region = FocusRegion::Terminal;
            self.focus_active_terminal(window, cx);
        }
        cx.notify();
    }

    /// Handle maximize/restore toggle on a pane.
    pub fn on_pane_maximize(&mut self, slot_idx: usize, cx: &mut Context<Self>) {
        self.pane_grid.toggle_maximize(slot_idx);
        cx.notify();
    }

    /// Handle close button on a pane.
    pub fn on_pane_close(&mut self, slot_idx: usize, cx: &mut Context<Self>) {
        self.pane_grid.remove(slot_idx);

        // If the closed pane was focused, move to next occupied or unfocus
        if self.pane_grid.focused_slot() == slot_idx {
            if let Some(next) = self.pane_grid.next_occupied_slot() {
                self.pane_grid.set_focus(next);
                if let super::pane_grid::PaneSlot::Occupied { session_id, .. } =
                    self.pane_grid.slot(next)
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

    /// Toggle between Control and Dashboard views.
    fn toggle_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                            crate::state::OperationError {
                                branch: branch.clone(),
                                message: e,
                            },
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
                            crate::state::OperationError {
                                branch: branch.clone(),
                                message: e,
                            },
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

    /// Execute a bulk operation on the background executor.
    ///
    /// Shared pattern for open-all and stop-all. Clears existing errors,
    /// runs the operation in the background, then updates state with results.
    fn execute_bulk_operation_async<F>(
        &mut self,
        cx: &mut Context<Self>,
        operation: F,
        error_event: &'static str,
    ) where
        F: FnOnce(&[kild_core::SessionInfo]) -> (usize, Vec<crate::state::OperationError>)
            + Send
            + 'static,
    {
        if self.state.is_bulk_loading() {
            return;
        }
        self.state.clear_bulk_errors();
        self.state.set_bulk_loading();
        cx.notify();
        let displays = self.state.displays().to_vec();

        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            let result = cx
                .background_executor()
                .spawn(async move { operation(&displays) })
                .await;

            if let Err(e) = this.update(cx, |view, cx| {
                view.state.clear_bulk_loading();
                let (count, errors) = result;
                for error in &errors {
                    tracing::warn!(
                        event = error_event,
                        branch = error.branch,
                        error = error.message
                    );
                }
                view.state.set_bulk_errors(errors);
                if count > 0 || view.state.has_bulk_errors() {
                    view.refresh_and_prune();
                }
                cx.notify();
            }) {
                tracing::debug!(
                    event = "ui.bulk_operation.view_dropped",
                    error = ?e,
                );
            }
        })
        .detach();
    }

    /// Handle click on the Open All button.
    fn on_open_all_click(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.open_all_clicked");
        self.execute_bulk_operation_async(
            cx,
            actions::open_all_stopped,
            "ui.open_all.partial_failure",
        );
    }

    /// Handle click on the Stop All button.
    fn on_stop_all_click(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.stop_all_clicked");
        self.execute_bulk_operation_async(
            cx,
            actions::stop_all_running,
            "ui.stop_all.partial_failure",
        );
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
            self.state.set_error(
                branch,
                crate::state::OperationError {
                    branch: branch.to_string(),
                    message: e,
                },
            );
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
    fn record_error(&mut self, branch: &str, message: &str, cx: &mut Context<Self>) {
        tracing::warn!(
            event = "ui.operation.error_displayed",
            branch = branch,
            error = message
        );
        self.state.set_error(
            branch,
            crate::state::OperationError {
                branch: branch.to_string(),
                message: message.to_string(),
            },
        );
        cx.notify();
    }

    /// Clear bulk operation errors (called when user dismisses the banner).
    fn on_dismiss_bulk_errors(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.bulk_errors.dismissed");
        self.mutate_state(cx, |s| s.clear_bulk_errors());
    }

    /// Clear startup errors (called when user dismisses the banner).
    fn on_dismiss_errors(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.errors.dismissed");
        self.mutate_state(cx, |s| s.dismiss_errors());
    }

    // --- Project management handlers ---

    /// Handle click on Add Project button.
    pub fn on_add_project_click(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.add_project_dialog.opened");
        self.state.open_add_project_dialog();

        let path_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("/path/to/repository"));
        self.path_input = Some(path_input);

        let name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Defaults to directory name"));
        self.name_input = Some(name_input);

        cx.notify();
    }

    /// Handle add project dialog cancel.
    pub fn on_add_project_cancel(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.add_project_dialog.cancelled");
        self.clear_input_entities();
        self.mutate_state(cx, |s| s.close_dialog());
    }

    /// Handle add project dialog submit.
    pub fn on_add_project_submit(&mut self, cx: &mut Context<Self>) {
        if !self.state.dialog().is_add_project() {
            tracing::error!(
                event = "ui.add_project_submit.invalid_state",
                "on_add_project_submit called when AddProject dialog not open"
            );
            return;
        }

        // Read text values from InputState entities
        let path_str = self
            .path_input
            .as_ref()
            .map(|i| i.read(cx).value().to_string())
            .unwrap_or_default();
        let path_str = path_str.trim().to_string();
        let name_str = self
            .name_input
            .as_ref()
            .map(|i| i.read(cx).value().to_string())
            .unwrap_or_default();
        let name = if name_str.trim().is_empty() {
            None
        } else {
            Some(name_str.trim().to_string())
        };

        if path_str.is_empty() {
            self.state
                .set_dialog_error("Path cannot be empty".to_string());
            cx.notify();
            return;
        }

        // Normalize path: expand ~ and ensure absolute path
        let path = match normalize_project_path(&path_str) {
            Ok(p) => p,
            Err(e) => {
                self.state.set_dialog_error(e);
                cx.notify();
                return;
            }
        };

        match actions::dispatch_add_project(path.clone(), name) {
            Ok(events) => {
                self.state.apply_events(&events);
                self.prune_terminal_cache();
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.add_project.error_displayed",
                    path = %path.display(),
                    error = %e
                );
                self.state.set_dialog_error(e);
            }
        }
        cx.notify();
    }

    /// Handle project selection from sidebar.
    pub fn on_project_select(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        tracing::info!(
            event = "ui.project_selected",
            path = %path.display()
        );

        match actions::dispatch_set_active_project(Some(path)) {
            Ok(events) => {
                self.state.apply_events(&events);
                self.prune_terminal_cache();
                self.reset_pane_grid();
            }
            Err(e) => {
                tracing::error!(event = "ui.project_select.failed", error = %e);
                self.state
                    .push_error(format!("Failed to select project: {}", e));
            }
        }
        cx.notify();
    }

    /// Handle "All Projects" selection from sidebar.
    pub fn on_project_select_all(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.project_selected_all");

        match actions::dispatch_set_active_project(None) {
            Ok(events) => {
                self.state.apply_events(&events);
                self.prune_terminal_cache();
                self.reset_pane_grid();
            }
            Err(e) => {
                tracing::error!(event = "ui.project_select_all.failed", error = %e);
                self.state
                    .push_error(format!("Failed to update project selection: {}", e));
            }
        }
        cx.notify();
    }

    /// Handle remove project from list.
    #[allow(dead_code)]
    pub fn on_remove_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        tracing::info!(
            event = "ui.remove_project.started",
            path = %path.display()
        );

        match actions::dispatch_remove_project(path) {
            Ok(events) => {
                self.state.apply_events(&events);
                self.prune_terminal_cache();
            }
            Err(e) => {
                tracing::error!(event = "ui.remove_project.failed", error = %e);
                self.state
                    .push_error(format!("Failed to remove project: {}", e));
            }
        }
        cx.notify();
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

    #[allow(dead_code)]
    fn start_rename(
        &mut self,
        session_id: &str,
        idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_label = self
            .terminal_tabs
            .get(session_id)
            .and_then(|tabs| tabs.get(idx))
            .map(|e| e.label().to_string())
            .unwrap_or_default();

        let input = cx.new(|cx| InputState::new(window, cx).default_value(current_label));
        let handle = input.read(cx).focus_handle(cx).clone();
        self.renaming_tab = Some((session_id.to_string(), idx, input));
        window.focus(&handle);
        cx.notify();
    }

    fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((session_id, idx, input)) = self.renaming_tab.take() {
            let new_name = input.read(cx).value().to_string();
            let new_name = new_name.trim();
            if !new_name.is_empty()
                && let Some(tabs) = self.terminal_tabs.get_mut(&session_id)
            {
                tabs.rename(idx, new_name.to_string());
            }
        }
        self.focus_active_terminal(window, cx);
        cx.notify();
    }

    fn cancel_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.renaming_tab = None;
        self.focus_active_terminal(window, cx);
        cx.notify();
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

    #[allow(dead_code)]
    fn render_tab_bar(&self, session_id: &str, cx: &mut Context<Self>) -> gpui::AnyElement {
        use super::terminal_tabs::{RenamingTab, TabBarContext, render_tab_bar};

        let Some(tabs) = self.terminal_tabs.get(session_id) else {
            return div().into_any_element();
        };

        let renaming = self.renaming_tab.as_ref().map(|(s, i, input)| RenamingTab {
            session_id: s.as_str(),
            tab_index: *i,
            input,
        });

        let ctx = TabBarContext {
            tabs,
            session_id,
            renaming_tab: renaming,
            show_add_menu: self.show_add_menu,
            daemon_available: self.daemon_available,
            daemon_starting: self.daemon_starting,
        };
        render_tab_bar(&ctx, cx)
    }

    /// Render the view tab bar: [Control] [Dashboard] with ⌘D hint.
    fn render_view_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_control = self.active_view == ActiveView::Control;
        let is_dashboard = matches!(self.active_view, ActiveView::Dashboard | ActiveView::Detail);

        div()
            .flex()
            .items_center()
            .px(px(theme::SPACE_2))
            .py(px(theme::SPACE_1))
            .border_b_1()
            .border_color(theme::border_subtle())
            .gap(px(theme::SPACE_1))
            .child(
                div()
                    .id("view-tab-control")
                    .px(px(theme::SPACE_3))
                    .py(px(theme::SPACE_1))
                    .rounded(px(theme::RADIUS_SM))
                    .cursor_pointer()
                    .text_size(px(theme::TEXT_SM))
                    .when(is_control, |d| {
                        d.bg(theme::elevated())
                            .text_color(theme::text_bright())
                            .border_b_2()
                            .border_color(theme::ice())
                    })
                    .when(!is_control, |d| {
                        d.text_color(theme::text_muted())
                            .hover(|d| d.text_color(theme::text()))
                    })
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|view, _, window, cx| {
                            if view.active_view != ActiveView::Control {
                                view.active_view = ActiveView::Control;
                                if view.active_terminal_id.is_some() {
                                    view.focus_region = FocusRegion::Terminal;
                                    view.focus_active_terminal(window, cx);
                                }
                                cx.notify();
                            }
                        }),
                    )
                    .child("Control"),
            )
            .child(
                div()
                    .id("view-tab-dashboard")
                    .px(px(theme::SPACE_3))
                    .py(px(theme::SPACE_1))
                    .rounded(px(theme::RADIUS_SM))
                    .cursor_pointer()
                    .text_size(px(theme::TEXT_SM))
                    .when(is_dashboard, |d| {
                        d.bg(theme::elevated())
                            .text_color(theme::text_bright())
                            .border_b_2()
                            .border_color(theme::ice())
                    })
                    .when(!is_dashboard, |d| {
                        d.text_color(theme::text_muted())
                            .hover(|d| d.text_color(theme::text()))
                    })
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|view, _, _, cx| {
                            if view.active_view != ActiveView::Dashboard {
                                view.active_view = ActiveView::Dashboard;
                                view.focus_region = FocusRegion::Dashboard;
                                cx.notify();
                            }
                        }),
                    )
                    .child("Dashboard"),
            )
            // Spacer
            .child(div().flex_1())
            // ⌘D hint
            .child(
                div()
                    .text_size(px(theme::TEXT_XS))
                    .text_color(theme::text_muted())
                    .child("\u{2318}D"),
            )
    }

    /// Render the main content area based on active view.
    fn render_main_content(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        match self.active_view {
            ActiveView::Control => {
                super::pane_grid::render_pane_grid(&self.pane_grid, &self.terminal_tabs, cx)
                    .into_any_element()
            }
            ActiveView::Dashboard => {
                dashboard_view::render_dashboard(&self.state, &self.terminal_tabs, cx)
            }
            ActiveView::Detail => {
                detail_view::render_detail_view(&self.state, &self.terminal_tabs, cx)
            }
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        use crate::state::DialogState;

        let key_str = event.keystroke.key.to_string();

        // Tab rename mode: Enter commits, Escape cancels, all other keys go to Input
        if self.renaming_tab.is_some() {
            if key_str == "enter" {
                self.commit_rename(window, cx);
            } else if key_str == "escape" {
                self.cancel_rename(window, cx);
            }
            return;
        }

        // Ctrl+Escape: move focus from terminal to sidebar (terminal stays rendered)
        if key_str == "escape"
            && event.keystroke.modifiers.control
            && self.focus_region == FocusRegion::Terminal
        {
            self.focus_region = FocusRegion::Dashboard;
            self.show_add_menu = false;
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        // Ctrl+Tab / Ctrl+Shift+Tab: cycle terminal tabs
        if key_str == "tab" && event.keystroke.modifiers.control {
            let should_focus = if let Some(id) = &self.active_terminal_id {
                if let Some(tabs) = self.terminal_tabs.get_mut(id) {
                    if tabs.len() > 1 {
                        if event.keystroke.modifiers.shift {
                            tabs.cycle_prev();
                        } else {
                            tabs.cycle_next();
                        }
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            if should_focus {
                self.focus_active_terminal(window, cx);
                cx.notify();
            }
            return;
        }

        // Ctrl+T: toggle terminal active/inactive within Control view
        if key_str == "t" && event.keystroke.modifiers.control {
            if matches!(self.active_view, ActiveView::Dashboard | ActiveView::Detail) {
                self.active_view = ActiveView::Control;
                if let Some(id) = self.state.selected_id().map(|s| s.to_string()) {
                    self.on_kild_select(&id, window, cx);
                    return;
                }
            } else if self.active_terminal_view().is_some() {
                self.active_terminal_id = None;
                self.focus_region = FocusRegion::Dashboard;
                window.focus(&self.focus_handle);
            } else if let Some(id) = self.state.selected_id().map(|s| s.to_string()) {
                self.on_kild_select(&id, window, cx);
                return;
            }
            cx.notify();
            return;
        }

        // Cmd+J/K/D: kild navigation (works in both dashboard and terminal mode)
        // Note: Cmd+1-9 index jumping deferred to #415 (configurable modifier)
        let cmd = event.keystroke.modifiers.platform;

        if cmd && key_str == "j" {
            self.navigate_next_kild(window, cx);
            cx.notify();
            return;
        }

        if cmd && key_str == "k" {
            self.navigate_prev_kild(window, cx);
            cx.notify();
            return;
        }

        if cmd && key_str == "d" {
            self.toggle_view(window, cx);
            return;
        }

        // Escape in Detail view: back to Dashboard
        if key_str == "escape" && self.active_view == ActiveView::Detail {
            self.active_view = ActiveView::Dashboard;
            self.focus_region = FocusRegion::Dashboard;
            window.focus(&self.focus_handle);
            cx.notify();
            return;
        }

        // Propagate keys to terminal only when Control view is active, terminal exists,
        // and terminal has focus. Without these guards, keys would reach a non-visible terminal.
        if self.active_view == ActiveView::Control
            && self.focus_region == FocusRegion::Terminal
            && self.active_terminal_view().is_some()
        {
            cx.propagate();
            return;
        }

        match self.state.dialog() {
            DialogState::None => {}
            DialogState::Confirm { .. } => {
                if key_str == "escape" {
                    self.on_confirm_cancel(cx);
                }
            }
            DialogState::AddProject { .. } => {
                if key_str == "escape" {
                    self.on_add_project_cancel(cx);
                }
            }
            DialogState::Create { .. } => match key_str.as_str() {
                "escape" => self.on_dialog_cancel(cx),
                "enter" => self.on_dialog_submit(cx),
                "tab" => self.on_agent_cycle(cx),
                _ => {}
            },
        }
    }
}

impl Focusable for MainView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MainView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let stopped_count = self.state.stopped_count();
        let running_count = self.state.running_count();

        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::void())
            // Header with title and action buttons
            .child(
                div()
                    .px(px(theme::SPACE_4))
                    .py(px(theme::SPACE_3))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(theme::TEXT_XL))
                            .text_color(theme::text_white())
                            .font_weight(FontWeight::BOLD)
                            .child("KILD"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(theme::SPACE_2))
                            // Open All button - Success variant
                            .child(
                                Button::new("open-all-btn")
                                    .label(format!("Open All ({})", stopped_count))
                                    .success()
                                    .disabled(stopped_count == 0 || self.state.is_bulk_loading())
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.on_open_all_click(cx);
                                    })),
                            )
                            // Stop All button - Warning variant
                            .child(
                                Button::new("stop-all-btn")
                                    .label(format!("Stop All ({})", running_count))
                                    .warning()
                                    .disabled(running_count == 0 || self.state.is_bulk_loading())
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.on_stop_all_click(cx);
                                    })),
                            )
                            // Start Daemon button - shown when daemon is not running
                            .when(self.daemon_available != Some(true), |this| {
                                this.child(
                                    Button::new("start-daemon-btn")
                                        .label(if self.daemon_starting {
                                            "Starting…"
                                        } else {
                                            "Start Daemon"
                                        })
                                        .ghost()
                                        .disabled(self.daemon_starting)
                                        .on_click(cx.listener(|view, _, _, cx| {
                                            view.on_start_daemon(cx);
                                        })),
                                )
                            })
                            // Refresh button - Ghost variant
                            .child(
                                Button::new("refresh-btn")
                                    .label("Refresh")
                                    .ghost()
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.on_refresh_click(cx);
                                    })),
                            )
                            // Create button - Primary variant
                            .child(
                                Button::new("create-header-btn")
                                    .label("+ Create")
                                    .primary()
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.on_create_button_click(window, cx);
                                    })),
                            ),
                    ),
            )
            // Error banner (shown for startup failures, project errors, state desync recovery)
            .when(self.state.has_banner_errors(), |this| {
                let errors = self.state.banner_errors();
                let error_count = errors.len();
                this.child(
                    div()
                        .mx(px(theme::SPACE_4))
                        .mt(px(theme::SPACE_2))
                        .px(px(theme::SPACE_4))
                        .py(px(theme::SPACE_2))
                        .bg(theme::with_alpha(theme::ember(), 0.15))
                        .rounded(px(theme::RADIUS_MD))
                        .flex()
                        .flex_col()
                        .gap(px(theme::SPACE_1))
                        // Header with dismiss button
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_color(theme::ember())
                                        .font_weight(FontWeight::BOLD)
                                        .child(format!(
                                            "Error{}:",
                                            if error_count == 1 { "" } else { "s" }
                                        )),
                                )
                                .child(Button::new("dismiss-errors").label("×").ghost().on_click(
                                    cx.listener(|view, _, _, cx| {
                                        view.on_dismiss_errors(cx);
                                    }),
                                )),
                        )
                        // Error list
                        .children(errors.iter().map(|e| {
                            div()
                                .text_size(px(theme::TEXT_SM))
                                .text_color(theme::with_alpha(theme::ember(), 0.8))
                                .child(format!("• {}", e))
                        })),
                )
            })
            // Bulk operation errors banner (dismissible)
            .when(self.state.has_bulk_errors(), |this| {
                let bulk_errors = self.state.bulk_errors();
                let error_count = bulk_errors.len();
                this.child(
                    div()
                        .mx(px(theme::SPACE_4))
                        .mt(px(theme::SPACE_2))
                        .px(px(theme::SPACE_4))
                        .py(px(theme::SPACE_2))
                        .bg(theme::with_alpha(theme::ember(), 0.15))
                        .rounded(px(theme::RADIUS_MD))
                        .flex()
                        .flex_col()
                        .gap(px(theme::SPACE_1))
                        // Header with dismiss button
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_color(theme::ember())
                                        .font_weight(FontWeight::BOLD)
                                        .child(format!(
                                            "{} operation{} failed:",
                                            error_count,
                                            if error_count == 1 { "" } else { "s" }
                                        )),
                                )
                                .child(
                                    Button::new("dismiss-bulk-errors")
                                        .label("×")
                                        .ghost()
                                        .on_click(cx.listener(|view, _, _, cx| {
                                            view.on_dismiss_bulk_errors(cx);
                                        })),
                                ),
                        )
                        // Error list
                        .children(bulk_errors.iter().map(|e| {
                            div()
                                .text_size(px(theme::TEXT_SM))
                                .text_color(theme::with_alpha(theme::ember(), 0.8))
                                .child(format!("• {}: {}", e.branch, e.message))
                        })),
                )
            })
            // Main content: Rail | Sidebar | Main area (always visible)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    // Project rail (48px)
                    .child(project_rail::render_project_rail(&self.state, cx))
                    // Sidebar (200px, kild navigation)
                    .child(sidebar::render_sidebar(
                        &self.state,
                        &self.terminal_tabs,
                        &self.pane_grid,
                        cx,
                    ))
                    // Main area (flex-1)
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            // View tab bar: [Control] [Dashboard]
                            .child(self.render_view_tab_bar(cx))
                            // View content
                            .child(self.render_main_content(cx)),
                    ),
            )
            // Dialog rendering (based on current dialog state)
            .when(self.state.dialog().is_create(), |this| {
                this.child(create_dialog::render_create_dialog(
                    self.state.dialog(),
                    self.state.is_dialog_loading(),
                    self.branch_input.as_ref(),
                    self.note_input.as_ref(),
                    cx,
                ))
            })
            .when(self.state.dialog().is_confirm(), |this| {
                this.child(confirm_dialog::render_confirm_dialog(
                    self.state.dialog(),
                    self.state.is_dialog_loading(),
                    cx,
                ))
            })
            .when(self.state.dialog().is_add_project(), |this| {
                this.child(add_project_dialog::render_add_project_dialog(
                    self.state.dialog(),
                    self.path_input.as_ref(),
                    self.name_input.as_ref(),
                    cx,
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_active_view_default_is_control() {
        assert_eq!(ActiveView::Control, ActiveView::Control);
        assert_ne!(ActiveView::Control, ActiveView::Dashboard);
        assert_ne!(ActiveView::Control, ActiveView::Detail);
        assert_ne!(ActiveView::Dashboard, ActiveView::Detail);
    }

    #[test]
    fn test_toggle_view_switches_control_to_dashboard() {
        let mut view = ActiveView::Control;
        view = match view {
            ActiveView::Control => ActiveView::Dashboard,
            ActiveView::Dashboard | ActiveView::Detail => ActiveView::Control,
        };
        assert_eq!(view, ActiveView::Dashboard);
    }

    #[test]
    fn test_toggle_view_switches_dashboard_to_control() {
        let mut view = ActiveView::Dashboard;
        view = match view {
            ActiveView::Control => ActiveView::Dashboard,
            ActiveView::Dashboard | ActiveView::Detail => ActiveView::Control,
        };
        assert_eq!(view, ActiveView::Control);
    }

    #[test]
    fn test_toggle_view_switches_detail_to_control() {
        let mut view = ActiveView::Detail;
        view = match view {
            ActiveView::Control => ActiveView::Dashboard,
            ActiveView::Dashboard | ActiveView::Detail => ActiveView::Control,
        };
        assert_eq!(view, ActiveView::Control);
    }

    #[test]
    fn test_dashboard_tab_active_in_detail_view() {
        let view = ActiveView::Detail;
        let is_dashboard = matches!(view, ActiveView::Dashboard | ActiveView::Detail);
        assert!(is_dashboard);

        let view = ActiveView::Dashboard;
        let is_dashboard = matches!(view, ActiveView::Dashboard | ActiveView::Detail);
        assert!(is_dashboard);

        let view = ActiveView::Control;
        let is_dashboard = matches!(view, ActiveView::Dashboard | ActiveView::Detail);
        assert!(!is_dashboard);
    }

    #[test]
    fn test_normalize_path_with_leading_slash_nonexistent() {
        // Nonexistent paths now return errors
        let result = normalize_project_path("/Users/test/project");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot access"));
    }

    #[test]
    fn test_normalize_path_tilde_expansion() {
        // Nonexistent paths now return errors
        let result = normalize_project_path("~/projects/test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot access"));
    }

    #[test]
    fn test_normalize_path_bare_tilde() {
        let result = normalize_project_path("~").unwrap();
        let expected_home = dirs::home_dir()
            .expect("test requires home dir")
            .canonicalize()
            .expect("home should be canonicalizable");
        assert_eq!(result, expected_home);
    }

    #[test]
    fn test_normalize_path_trims_whitespace() {
        // Nonexistent paths now return errors
        let result = normalize_project_path("  /Users/test/project  ");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot access"));
    }

    #[test]
    fn test_normalize_path_without_leading_slash_fallback() {
        // Nonexistent paths now return errors
        let result = normalize_project_path("nonexistent/path/here");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot access"));
    }

    #[test]
    fn test_normalize_path_empty_string() {
        // Empty paths now return errors
        let result = normalize_project_path("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot access"));
    }

    #[test]
    fn test_normalize_path_whitespace_only() {
        // Whitespace-only paths now return errors
        let result = normalize_project_path("   ");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot access"));
    }

    #[test]
    fn test_normalize_path_tilde_in_middle_not_expanded() {
        // Nonexistent paths now return errors
        let result = normalize_project_path("/Users/test/~project");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot access"));
    }

    #[test]
    fn test_normalize_path_canonicalizes_existing_path() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        let result = normalize_project_path(path.to_str().unwrap()).unwrap();
        let expected = path.canonicalize().unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_normalize_path_lowercase_canonicalized() {
        if let Some(home) = dirs::home_dir() {
            let lowercase_path = home.to_str().unwrap().to_lowercase();
            let result = normalize_project_path(&lowercase_path).unwrap();

            assert!(result.exists(), "Canonicalized path should exist");

            let expected = home.canonicalize().unwrap();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn test_canonicalize_path_existing() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        let result = canonicalize_path(path.clone()).unwrap();
        let expected = path.canonicalize().unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_canonicalize_path_nonexistent_returns_error() {
        let path = PathBuf::from("/nonexistent/path/that/does/not/exist");
        let result = canonicalize_path(path.clone());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot access"));
    }

    #[test]
    #[cfg(unix)]
    fn test_normalize_path_resolves_symlinks() {
        use std::os::unix::fs::symlink;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let real_path = temp_dir.path().join("real_dir");
        std::fs::create_dir(&real_path).unwrap();

        let symlink_path = temp_dir.path().join("symlink_dir");
        symlink(&real_path, &symlink_path).unwrap();

        let result = normalize_project_path(symlink_path.to_str().unwrap()).unwrap();

        // Should resolve symlink to the real path
        let expected = real_path.canonicalize().unwrap();
        assert_eq!(result, expected, "Symlinks should resolve to real path");
        assert_ne!(
            result, symlink_path,
            "Result should differ from symlink path"
        );
    }
}
