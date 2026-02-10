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
    add_project_dialog, confirm_dialog, create_dialog, detail_panel, kild_list, sidebar,
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

/// Main application view that composes the kild list, header, and create dialog.
///
/// Owns application state and handles keyboard input for the create dialog.
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
    /// Terminal view entity (lazily created on first Ctrl+T).
    terminal_view: Option<gpui::Entity<crate::terminal::TerminalView>>,
    /// Whether the terminal pane is currently shown (full-area, replacing 3-column layout).
    show_terminal: bool,
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

        Self {
            state: AppState::new(),
            focus_handle: cx.focus_handle(),
            _refresh_task: refresh_task,
            _watcher_task: watcher_task,
            branch_input: None,
            note_input: None,
            path_input: None,
            name_input: None,
            terminal_view: None,
            show_terminal: false,
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

    /// Handle keyboard shortcuts for dialogs.
    ///
    /// Text input is handled by gpui-component's Input widget internally.
    /// This handler manages Escape (close), Enter (submit), and Tab (agent cycling).
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        use crate::state::DialogState;

        let key_str = event.keystroke.key.to_string();

        // Ctrl+T: toggle terminal view
        if key_str == "t" && event.keystroke.modifiers.control {
            if self.show_terminal {
                self.show_terminal = false;
                tracing::info!(event = "ui.terminal.toggle_off");
                // Return focus to main view
                window.focus(&self.focus_handle);
            } else {
                // Drop dead terminal so a fresh one gets created
                if let Some(view) = &self.terminal_view
                    && view.read(cx).terminal().has_exited()
                {
                    tracing::info!(event = "ui.terminal.recycled");
                    self.terminal_view = None;
                }
                // Lazy-create terminal on first toggle (or after shell exit)
                if self.terminal_view.is_none() {
                    tracing::info!(event = "ui.terminal.create_started");
                    match crate::terminal::state::Terminal::new(cx) {
                        Ok(terminal) => {
                            let view = cx.new(|cx| {
                                crate::terminal::TerminalView::from_terminal(terminal, window, cx)
                            });
                            self.terminal_view = Some(view);
                        }
                        Err(e) => {
                            tracing::error!(event = "ui.terminal.create_failed", error = %e);
                            self.show_terminal = false;
                            self.state
                                .push_error(format!("Terminal creation failed: {}", e));
                            cx.notify();
                            return;
                        }
                    }
                }
                self.show_terminal = true;
                tracing::info!(event = "ui.terminal.toggle_on");
                // Focus the terminal view
                if let Some(view) = &self.terminal_view {
                    let handle = view.read(cx).focus_handle(cx).clone();
                    window.focus(&handle);
                }
            }
            cx.notify();
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
            // Main content: terminal (full-area) or 3-column layout
            .when(self.show_terminal, |this| {
                if let Some(terminal_view) = &self.terminal_view {
                    this.child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .child(terminal_view.clone()),
                    )
                } else {
                    this
                }
            })
            .when(!self.show_terminal, |this| {
                this.child(
                    div()
                        .flex_1()
                        .flex()
                        .overflow_hidden()
                        // Sidebar (200px fixed)
                        .child(sidebar::render_sidebar(&self.state, cx))
                        // Kild list (flex:1)
                        .child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .child(kild_list::render_kild_list(&self.state, cx)),
                        )
                        // Detail panel (320px, conditional)
                        .when(self.state.has_selection(), |this| {
                            this.child(detail_panel::render_detail_panel(&self.state, cx))
                        }),
                )
            })
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
