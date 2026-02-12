//! Main view for kild-ui.
//!
//! Root view that composes header, kild list, create dialog, and confirm dialog.
//! Handles keyboard input and dialog state management.

// Allow dead_code — view methods are wired up incrementally as UI features are connected.
#![allow(dead_code)]

use gpui::{
    Context, FocusHandle, Focusable, FontWeight, IntoElement, KeyDownEvent, Render, Task, Window,
    div, prelude::*, px,
};

use crate::theme;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::InputState;
use tracing::{debug, warn};

use std::path::PathBuf;

use crate::actions;
use crate::state::AppState;
use crate::views::{
    add_project_dialog, confirm_dialog, create_dialog, kild_sidebar, minimized_bar, rail,
    status_bar, teammate_tabs,
};
use crate::watcher::{SessionWatcher, ShimWatcher};

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

/// Main application view — terminal multiplexer layout.
///
/// Composes: rail | sidebar | main terminal area + minimized bars + status bar.
/// Owns application state, terminal connections, and handles keyboard routing.
pub struct MainView {
    state: AppState,
    focus_handle: FocusHandle,
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
    /// Guard against concurrent daemon terminal creation from rapid sidebar clicks.
    attaching_kild: Option<String>,
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

        // Try to create file watchers
        let watcher = SessionWatcher::new(&sessions_dir);
        let shim_watcher = ShimWatcher::new();
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
            if watcher.is_none() && shim_watcher.is_none() {
                tracing::debug!(event = "ui.watcher_task.skipped", reason = "no watchers");
                return;
            }

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
                    // Check for session file events
                    if let Some(ref w) = watcher
                        && w.has_pending_events()
                    {
                        pending_refresh = true;
                    }

                    // Check for shim pane registry events (teammate changes)
                    if let Some(ref sw) = shim_watcher {
                        let changed = sw.drain_changed_sessions();
                        if !changed.is_empty() {
                            for session_id in &changed {
                                view.state.refresh_teammates(session_id);
                            }
                            cx.notify();
                        }
                    }

                    // Refresh sessions if we have pending events AND debounce period has passed
                    if pending_refresh && last_refresh.elapsed() > crate::refresh::DEBOUNCE_INTERVAL
                    {
                        tracing::info!(event = "ui.watcher.refresh_triggered");
                        view.state.refresh_sessions();
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

        Self {
            state: AppState::new(),
            focus_handle: cx.focus_handle(),
            _refresh_task: refresh_task,
            _watcher_task: watcher_task,
            branch_input: None,
            note_input: None,
            path_input: None,
            name_input: None,
            attaching_kild: None,
        }
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
                    Ok(events) => view.state.apply_events(&events),
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
        self.mutate_state(cx, |s| s.refresh_sessions());
    }

    /// Handle click on the destroy button [×] in a kild row.
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
                    Ok(events) => view.state.apply_events(&events),
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

    /// Handle kild row click - select for detail panel.
    pub fn on_kild_select(&mut self, session_id: &str, cx: &mut Context<Self>) {
        tracing::debug!(event = "ui.kild.selected", session_id = session_id);
        let id = session_id.to_string();
        self.mutate_state(cx, |s| s.select_kild(id));
    }

    /// Handle "back to list" / clear selection action.
    pub fn on_clear_selection(&mut self, cx: &mut Context<Self>) {
        tracing::debug!(event = "ui.kild.selection_cleared");
        self.mutate_state(cx, |s| s.clear_selection());
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
                    view.state.refresh_sessions();
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
            Ok(events) => self.state.apply_events(&events),
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
            Ok(events) => self.state.apply_events(&events),
            Err(e) => {
                tracing::error!(event = "ui.project_select_all.failed", error = %e);
                self.state
                    .push_error(format!("Failed to update project selection: {}", e));
            }
        }
        cx.notify();
    }

    /// Handle remove project from list.
    pub fn on_remove_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        tracing::info!(
            event = "ui.remove_project.started",
            path = %path.display()
        );

        match actions::dispatch_remove_project(path) {
            Ok(events) => self.state.apply_events(&events),
            Err(e) => {
                tracing::error!(event = "ui.remove_project.failed", error = %e);
                self.state
                    .push_error(format!("Failed to remove project: {}", e));
            }
        }
        cx.notify();
    }

    // --- Terminal multiplexer handlers ---

    /// Attach to a daemon session's terminal for a specific kild.
    ///
    /// Creates a daemon-backed terminal connection and stores it in the TerminalStore.
    /// Uses `cx.spawn_in(window, ...)` to get AsyncWindowContext for TerminalView creation.
    pub fn attach_to_kild(
        &mut self,
        kild_id: String,
        daemon_session_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Guard: already attached or currently attaching
        if self.state.has_terminal_for(&kild_id) {
            self.state.set_terminal_focus(&kild_id);
            cx.notify();
            return;
        }
        if self.attaching_kild.as_deref() == Some(kild_id.as_str()) {
            return;
        }

        self.attaching_kild = Some(kild_id.clone());
        tracing::info!(
            event = "ui.terminal.attach_to_kild_started",
            kild_id = %kild_id,
            daemon_session_id = %daemon_session_id,
        );

        let kild_id_for_task = kild_id.clone();
        let daemon_sid = daemon_session_id.clone();

        cx.spawn_in(
            window,
            async move |this, cx: &mut gpui::AsyncWindowContext| {
                // 1. Connect and attach to daemon session
                let conn = cx
                    .background_executor()
                    .spawn(async move {
                        crate::daemon_client::connect_for_attach(&daemon_sid, 24, 80).await
                    })
                    .await;
                let conn = match conn {
                    Ok(c) => c,
                    Err(e) => {
                        let Some(this) = this.upgrade() else { return };
                        let kid = kild_id_for_task.clone();
                        if let Err(update_err) =
                            cx.update_entity(&this, |view: &mut MainView, cx| {
                                view.attaching_kild = None;
                                view.state.push_error(format!("Daemon attach failed: {e}"));
                                cx.notify();
                            })
                        {
                            tracing::error!(
                                event = "ui.terminal.attach_error_display_failed",
                                kild_id = %kid,
                                error = %update_err,
                            );
                        }
                        return;
                    }
                };

                // 2. Create terminal and TerminalView
                let Some(this) = this.upgrade() else { return };
                let kid = kild_id_for_task.clone();
                let dsid = daemon_session_id.clone();
                if let Err(update_err) =
                    cx.update_window_entity(&this, |view: &mut MainView, window, cx| {
                        view.attaching_kild = None;
                        let sid = conn.session_id().to_string();
                        match crate::terminal::state::Terminal::from_daemon(sid, conn, cx) {
                            Ok(terminal) => {
                                let term_view = cx.new(|cx| {
                                    crate::terminal::TerminalView::from_terminal(
                                        terminal, window, cx,
                                    )
                                });
                                view.state.attach_terminal(kid.clone(), dsid, term_view);
                                view.state.set_terminal_focus(&kid);
                                // Focus the terminal
                                if let Some(v) = view.state.focused_terminal() {
                                    let h = v.read(cx).focus_handle(cx).clone();
                                    window.focus(&h);
                                }
                            }
                            Err(e) => {
                                view.state
                                    .push_error(format!("Daemon terminal failed: {e}"));
                            }
                        }
                        cx.notify();
                    })
                {
                    tracing::error!(
                        event = "ui.terminal.attach_view_update_failed",
                        kild_id = %kid,
                        error = %update_err,
                    );
                }
            },
        )
        .detach();

        cx.notify();
    }

    /// Handle kild click in sidebar — select and auto-attach if daemon + running.
    pub fn on_kild_sidebar_click(
        &mut self,
        session_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(event = "ui.kild.sidebar_clicked", session_id = session_id);
        let id = session_id.to_string();
        self.state.select_kild(id.clone());
        self.state.set_terminal_focus(&id);

        // Refresh teammate data for this kild
        self.state.refresh_teammates(&id);

        // Auto-attach if daemon + running + not already attached
        if !self.state.has_terminal_for(&id)
            && let Some(display) = self.state.selected_kild()
        {
            let is_daemon = display
                .session
                .runtime_mode
                .as_ref()
                .map(|m| matches!(m, kild_core::RuntimeMode::Daemon))
                .unwrap_or(false);
            let is_running = display.process_status == kild_core::ProcessStatus::Running;

            if is_daemon
                && is_running
                && let Some(agent) = display.session.latest_agent()
                && let Some(dsid) = agent.daemon_session_id()
            {
                let dsid = dsid.to_string();
                self.attach_to_kild(id, dsid, window, cx);
                return;
            }
        }

        cx.notify();
    }

    /// Handle teammate tab click — attach to the teammate's daemon session.
    pub fn on_teammate_tab_click(
        &mut self,
        daemon_session_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(
            event = "ui.teammate_tab.clicked",
            daemon_session_id = daemon_session_id,
        );

        // Use the daemon_session_id as a virtual kild ID for the terminal store
        // (teammate terminals are keyed by their daemon session ID)
        let virtual_id = format!("teammate:{}", daemon_session_id);
        let dsid = daemon_session_id.to_string();

        if self.state.has_terminal_for(&virtual_id) {
            self.state.set_terminal_focus(&virtual_id);
            cx.notify();
        } else {
            self.attach_to_kild(virtual_id, dsid, window, cx);
        }
    }

    /// Handle keyboard shortcuts.
    ///
    /// Text input is handled by gpui-component's Input widget internally.
    /// This handler manages: Escape (close dialog), Enter (submit), Tab (cycle),
    /// Cmd+J/K (navigate kilds), and terminal key propagation.
    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        use crate::state::DialogState;
        use crate::views::split_pane::SplitDirection;

        let key_str = event.keystroke.key.to_string();
        let cmd = event.keystroke.modifiers.platform;
        let shift = event.keystroke.modifiers.shift;

        // Cmd+J: select next kild in sidebar (wraps)
        if cmd && key_str == "j" {
            self.navigate_kild(1, cx);
            return;
        }
        // Cmd+K: select previous kild in sidebar (wraps)
        if cmd && key_str == "k" {
            self.navigate_kild(-1, cx);
            return;
        }

        // Cmd+\: split vertical (side-by-side)
        if cmd && !shift && key_str == "\\" {
            self.on_split(SplitDirection::Vertical, cx);
            return;
        }
        // Cmd+Shift+\: split horizontal (top-bottom)
        if cmd && shift && key_str == "\\" {
            self.on_split(SplitDirection::Horizontal, cx);
            return;
        }
        // Cmd+W: close split pane
        if cmd && key_str == "w" && self.state.layout_is_split() {
            self.state.layout_unsplit();
            cx.notify();
            return;
        }

        // When a terminal has focus, propagate all non-reserved keys to TerminalView
        if self.state.focused_terminal().is_some() {
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

    /// Handle split command — split the main area with a second pane.
    fn on_split(
        &mut self,
        direction: crate::views::split_pane::SplitDirection,
        cx: &mut Context<Self>,
    ) {
        // Need a focused kild to split from
        let Some(focused_id) = self.state.focused_kild_id().map(|s| s.to_string()) else {
            return;
        };

        // Already split — toggle direction or unsplit
        if self.state.layout_is_split() {
            self.state.layout_unsplit();
            cx.notify();
            return;
        }

        // Find the next running kild to show in the second pane
        let displays = self.state.filtered_displays();
        let next_kild = displays
            .iter()
            .filter(|d| {
                d.process_status == kild_core::ProcessStatus::Running && d.session.id != focused_id
            })
            .map(|d| d.session.id.clone())
            .next();

        if let Some(second_id) = next_kild {
            self.state.layout_split_with(direction, second_id);
            cx.notify();
        }
    }

    /// Navigate to next/previous kild in the filtered display list.
    fn navigate_kild(&mut self, direction: i32, cx: &mut Context<Self>) {
        let displays = self.state.filtered_displays();
        if displays.is_empty() {
            return;
        }

        let current_id = self.state.selected_id().map(|s| s.to_string());
        let current_idx = current_id
            .as_deref()
            .and_then(|id| displays.iter().position(|d| d.session.id == id));

        let next_idx = match current_idx {
            Some(idx) => {
                let len = displays.len() as i32;
                ((idx as i32 + direction).rem_euclid(len)) as usize
            }
            None => 0,
        };

        let next_id = displays[next_idx].session.id.clone();
        self.state.select_kild(next_id.clone());
        self.state.set_terminal_focus(&next_id);
        cx.notify();
    }
}

impl Focusable for MainView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MainView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex_col()
            .bg(theme::void())
            // Error banners (startup failures, bulk operation errors)
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
                        .children(errors.iter().map(|e| {
                            div()
                                .text_size(px(theme::TEXT_SM))
                                .text_color(theme::with_alpha(theme::ember(), 0.8))
                                .child(format!("• {}", e))
                        })),
                )
            })
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
                        .children(bulk_errors.iter().map(|e| {
                            div()
                                .text_size(px(theme::TEXT_SM))
                                .text_color(theme::with_alpha(theme::ember(), 0.8))
                                .child(format!("• {}: {}", e.branch, e.message))
                        })),
                )
            })
            // Main content: rail | sidebar | terminal main area
            .child(
                div()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    // Project rail (48px)
                    .child(rail::render_rail(&self.state, cx))
                    // Kild sidebar (220px)
                    .child(kild_sidebar::render_kild_sidebar(&self.state, cx))
                    // Main terminal area
                    .child(
                        div()
                            .flex_1()
                            .flex_col()
                            .overflow_hidden()
                            // Teammate tabs
                            .child(teammate_tabs::render_teammate_tabs(&self.state, cx))
                            // Terminal pane (focused kild's terminal or empty state)
                            .child(self.render_terminal_area(cx))
                            // Minimized session bars
                            .child(minimized_bar::render_minimized_bars(&self.state, cx)),
                    ),
            )
            // Status bar (bottom)
            .child(status_bar::render_status_bar(&self.state, cx))
            // Dialog overlays
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

impl MainView {
    /// Render the main terminal area: split panes, single terminal, or empty state.
    fn render_terminal_area(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::views::split_pane::{PaneContent, SplitPane, render_split};

        // Check for split mode
        if let Some(split_config) = self.state.layout_split() {
            let first = self
                .state
                .focused_terminal()
                .map(|e| PaneContent::Terminal(e.clone()))
                .unwrap_or(PaneContent::Empty);
            let second = self
                .state
                .get_terminal(split_config.second_id())
                .map(|e| PaneContent::Terminal(e.clone()))
                .unwrap_or(PaneContent::Empty);

            let split = SplitPane {
                direction: split_config.direction(),
                first,
                second,
                ratio: split_config.ratio(),
            };

            div()
                .flex_1()
                .overflow_hidden()
                .child(render_split(&split, cx))
        } else if let Some(terminal_entity) = self.state.focused_terminal() {
            div()
                .flex_1()
                .overflow_hidden()
                .child(terminal_entity.clone())
        } else {
            div().flex_1().flex().items_center().justify_center().child(
                div()
                    .text_color(theme::text_muted())
                    .text_size(px(theme::TEXT_BASE))
                    .child("Select a kild from the sidebar"),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
