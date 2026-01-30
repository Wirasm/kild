//! Main view for kild-ui.
//!
//! Root view that composes header, kild list, create dialog, and confirm dialog.
//! Handles keyboard input and dialog state management.

use gpui::{
    Context, FocusHandle, Focusable, FontWeight, IntoElement, KeyDownEvent, Render, Task, Window,
    div, prelude::*, px,
};

use crate::components::{Button, ButtonVariant};
use crate::theme;
use tracing::{debug, warn};

use std::path::PathBuf;

use crate::actions;
use crate::state::{AddProjectDialogField, AppState, CreateDialogField};
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
        }
    }

    /// Handle click on the Create button in header.
    fn on_create_button_click(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.create_dialog.opened");
        self.state.open_create_dialog();
        cx.notify();
    }

    /// Handle dialog cancel button click (create dialog).
    pub fn on_dialog_cancel(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.create_dialog.cancelled");
        self.state.close_dialog();
        cx.notify();
    }

    /// Handle dialog submit button click (create dialog).
    pub fn on_dialog_submit(&mut self, cx: &mut Context<Self>) {
        // Extract form data from dialog state
        let crate::state::DialogState::Create { form, .. } = self.state.dialog() else {
            tracing::error!(
                event = "ui.dialog_submit.invalid_state",
                "on_dialog_submit called when Create dialog not open"
            );
            return;
        };

        let branch = form.branch_name.trim().to_string();
        let agent = form.selected_agent();
        let note = if form.note.trim().is_empty() {
            None
        } else {
            Some(form.note.trim().to_string())
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

        match actions::create_kild(&branch, &agent, note, project_path) {
            Ok(_session) => {
                // Success - close dialog and refresh list
                self.state.close_dialog();
                self.state.refresh_sessions();
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.dialog_submit.error_displayed",
                    branch = %branch,
                    agent = %agent,
                    error = %e
                );
                self.state.set_dialog_error(e);
            }
        }
        cx.notify();
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
            let next_index = (form.selected_agent_index + 1) % agents.len();
            form.selected_agent_index = next_index;
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
        self.state.refresh_sessions();
        cx.notify();
    }

    /// Handle click on the destroy button [×] in a kild row.
    pub fn on_destroy_click(&mut self, branch: &str, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.destroy_dialog.opened", branch = branch);
        self.state.open_confirm_dialog(branch.to_string());
        cx.notify();
    }

    /// Handle confirm button click in destroy dialog.
    pub fn on_confirm_destroy(&mut self, cx: &mut Context<Self>) {
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

        match actions::destroy_kild(&branch, force) {
            Ok(()) => {
                // Clear selection if the destroyed kild was selected
                // After refresh, the selected kild won't exist in the list anyway,
                // but clearing explicitly ensures the panel disappears immediately
                if let Some(selected) = self.state.selected_kild()
                    && selected.session.branch == branch
                {
                    self.state.clear_selection();
                }
                self.state.close_dialog();
                self.state.refresh_sessions();
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.confirm_destroy.error_displayed",
                    branch = %branch,
                    error = %e
                );
                self.state.set_dialog_error(e);
            }
        }
        cx.notify();
    }

    /// Handle cancel button click in destroy dialog.
    pub fn on_confirm_cancel(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.confirm_dialog.cancelled");
        self.state.close_dialog();
        cx.notify();
    }

    /// Handle kild row click - select for detail panel.
    pub fn on_kild_select(&mut self, session_id: &str, cx: &mut Context<Self>) {
        tracing::debug!(event = "ui.kild.selected", session_id = session_id);
        self.state.select_kild(session_id.to_string());
        cx.notify();
    }

    /// Handle click on the Open button [▶] in a kild row.
    pub fn on_open_click(&mut self, branch: &str, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.open_clicked", branch = branch);
        self.state.clear_error(branch);

        match actions::open_kild(branch, None) {
            Ok(_session) => {
                self.state.refresh_sessions();
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.open_click.error_displayed",
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
        }
        cx.notify();
    }

    /// Handle click on the Stop button [⏹] in a kild row.
    pub fn on_stop_click(&mut self, branch: &str, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.stop_clicked", branch = branch);
        self.state.clear_error(branch);

        match actions::stop_kild(branch) {
            Ok(()) => {
                self.state.refresh_sessions();
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.stop_click.error_displayed",
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
        }
        cx.notify();
    }

    /// Handle click on the Open All button.
    fn on_open_all_click(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.open_all_clicked");
        self.handle_bulk_operation(actions::open_all_stopped, "ui.open_all.partial_failure", cx);
    }

    /// Handle click on the Stop All button.
    fn on_stop_all_click(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.stop_all_clicked");
        self.handle_bulk_operation(actions::stop_all_running, "ui.stop_all.partial_failure", cx);
    }

    /// Common handler for bulk operations (open all / stop all).
    fn handle_bulk_operation(
        &mut self,
        operation: impl Fn(&[kild_core::SessionInfo]) -> (usize, Vec<crate::state::OperationError>),
        error_event: &str,
        cx: &mut Context<Self>,
    ) {
        self.state.clear_bulk_errors();

        let (count, errors) = operation(self.state.displays());

        for error in &errors {
            tracing::warn!(
                event = error_event,
                branch = error.branch,
                error = error.message
            );
        }
        self.state.set_bulk_errors(errors);

        if count > 0 || self.state.has_bulk_errors() {
            self.state.refresh_sessions();
        }
        cx.notify();
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
        cx.notify();
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
        } else {
            cx.notify();
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
        self.state.clear_bulk_errors();
        cx.notify();
    }

    /// Clear startup errors (called when user dismisses the banner).
    fn on_dismiss_errors(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.errors.dismissed");
        self.state.dismiss_errors();
        cx.notify();
    }

    // --- Project management handlers ---

    /// Handle click on Add Project button.
    pub fn on_add_project_click(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.add_project_dialog.opened");
        self.state.open_add_project_dialog();
        cx.notify();
    }

    /// Handle add project dialog cancel.
    pub fn on_add_project_cancel(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.add_project_dialog.cancelled");
        self.state.close_dialog();
        cx.notify();
    }

    /// Handle add project dialog submit.
    pub fn on_add_project_submit(&mut self, cx: &mut Context<Self>) {
        // Extract form data from dialog state
        let crate::state::DialogState::AddProject { form, .. } = self.state.dialog() else {
            tracing::error!(
                event = "ui.add_project_submit.invalid_state",
                "on_add_project_submit called when AddProject dialog not open"
            );
            return;
        };

        let path_str = form.path.trim().to_string();
        let name = if form.name.trim().is_empty() {
            None
        } else {
            Some(form.name.trim().to_string())
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

        match actions::add_project(path.clone(), name) {
            Ok(project) => {
                tracing::info!(
                    event = "ui.add_project.succeeded",
                    path = %path.display(),
                    name = %project.name()
                );
                // Update local state with new project
                // Note: ProjectManager.add() auto-selects first project
                if let Err(e) = self.state.add_project(project) {
                    tracing::error!(
                        event = "ui.add_project.state_sync_failed",
                        path = %path.display(),
                        error = %e,
                        "Project persisted but local state update failed, reloading from disk"
                    );
                    self.state.reload_projects();
                }
                self.state.close_dialog();
                // Refresh sessions to filter by new active project
                self.state.refresh_sessions();
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

        if let Err(e) = actions::set_active_project(Some(path.clone())) {
            tracing::error!(event = "ui.project_select.failed", error = %e);
            self.state
                .push_error(format!("Failed to select project: {}", e));
            cx.notify();
            return;
        }

        // Update local state - select() should not fail since persistence succeeded
        if let Err(e) = self.state.select_project(&path) {
            tracing::error!(
                event = "ui.project_select.state_sync_failed",
                path = %path.display(),
                error = %e,
                "Project selection persisted but local state update failed, reloading from disk"
            );
            self.state.reload_projects();
        }
        cx.notify();
    }

    /// Handle "All Projects" selection from sidebar.
    pub fn on_project_select_all(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.project_selected_all");

        if let Err(e) = actions::set_active_project(None) {
            tracing::error!(event = "ui.project_select_all.failed", error = %e);
            self.state
                .push_error(format!("Failed to update project selection: {}", e));
            cx.notify();
            return;
        }

        self.state.select_all_projects();
        cx.notify();
    }

    /// Handle remove project from list.
    pub fn on_remove_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        tracing::info!(
            event = "ui.remove_project.started",
            path = %path.display()
        );

        if let Err(e) = actions::remove_project(&path) {
            tracing::error!(event = "ui.remove_project.failed", error = %e);
            self.state
                .push_error(format!("Failed to remove project: {}", e));
            cx.notify();
            return;
        }

        // Update local state - remove() handles active_index adjustment
        if let Err(e) = self.state.remove_project(&path) {
            tracing::error!(
                event = "ui.remove_project.state_sync_failed",
                path = %path.display(),
                error = %e,
                "Project removal persisted but local state update failed, reloading from disk"
            );
            self.state.reload_projects();
        }
        cx.notify();
    }

    /// Handle keyboard input for dialogs.
    ///
    /// When create dialog is open: handles branch name input (alphanumeric, -, _, /, space converts to hyphen),
    /// form submission (Enter), dialog dismissal (Escape), and agent cycling (Tab).
    /// When confirm dialog is open: handles dialog dismissal (Escape).
    /// When add project dialog is open: handles path/name input, submission, and dismissal.
    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        use crate::state::DialogState;

        let key_str = event.keystroke.key.to_string();

        // Handle keyboard input based on current dialog state
        match self.state.dialog_mut() {
            DialogState::None => {
                // No dialog open - ignore keyboard input
            }

            DialogState::Confirm { .. } => {
                // Confirm dialog only responds to Escape
                if key_str == "escape" {
                    self.on_confirm_cancel(cx);
                }
            }

            DialogState::AddProject { form, .. } => {
                match key_str.as_str() {
                    "backspace" => {
                        match form.focused_field {
                            AddProjectDialogField::Path => {
                                form.path.pop();
                            }
                            AddProjectDialogField::Name => {
                                form.name.pop();
                            }
                        }
                        cx.notify();
                    }
                    "enter" => {
                        self.on_add_project_submit(cx);
                    }
                    "escape" => {
                        self.on_add_project_cancel(cx);
                    }
                    "tab" => {
                        // Cycle focus between fields
                        form.focused_field = match form.focused_field {
                            AddProjectDialogField::Path => AddProjectDialogField::Name,
                            AddProjectDialogField::Name => AddProjectDialogField::Path,
                        };
                        cx.notify();
                    }
                    "space" => {
                        // Allow spaces in both path and name
                        match form.focused_field {
                            AddProjectDialogField::Path => {
                                form.path.push(' ');
                            }
                            AddProjectDialogField::Name => {
                                form.name.push(' ');
                            }
                        }
                        cx.notify();
                    }
                    key if key.len() == 1 => {
                        if let Some(c) = key.chars().next() {
                            // Path and name fields accept most characters
                            if !c.is_control() {
                                match form.focused_field {
                                    AddProjectDialogField::Path => {
                                        form.path.push(c);
                                    }
                                    AddProjectDialogField::Name => {
                                        form.name.push(c);
                                    }
                                }
                                cx.notify();
                            }
                        }
                    }
                    _ => {
                        // Ignore other keys
                    }
                }
            }

            DialogState::Create { form, .. } => {
                match key_str.as_str() {
                    "backspace" => {
                        match form.focused_field {
                            CreateDialogField::BranchName => {
                                form.branch_name.pop();
                            }
                            CreateDialogField::Note => {
                                form.note.pop();
                            }
                            CreateDialogField::Agent => {}
                        }
                        cx.notify();
                    }
                    "enter" => {
                        self.on_dialog_submit(cx);
                    }
                    "escape" => {
                        self.on_dialog_cancel(cx);
                    }
                    "space" => {
                        match form.focused_field {
                            CreateDialogField::BranchName => {
                                // Convert spaces to hyphens for branch names
                                form.branch_name.push('-');
                            }
                            CreateDialogField::Note => {
                                // Allow actual spaces in notes
                                form.note.push(' ');
                            }
                            CreateDialogField::Agent => {}
                        }
                        cx.notify();
                    }
                    "tab" => {
                        // Cycle focus between fields
                        form.focused_field = match form.focused_field {
                            CreateDialogField::BranchName => CreateDialogField::Agent,
                            CreateDialogField::Agent => CreateDialogField::Note,
                            CreateDialogField::Note => CreateDialogField::BranchName,
                        };
                        cx.notify();
                    }
                    key if key.len() == 1 => {
                        if let Some(c) = key.chars().next() {
                            match form.focused_field {
                                CreateDialogField::BranchName => {
                                    // Branch names: alphanumeric, -, _, /
                                    if c.is_alphanumeric() || c == '-' || c == '_' || c == '/' {
                                        form.branch_name.push(c);
                                        cx.notify();
                                    }
                                }
                                CreateDialogField::Note => {
                                    // Notes: any non-control character
                                    if !c.is_control() {
                                        form.note.push(c);
                                        cx.notify();
                                    }
                                }
                                CreateDialogField::Agent => {
                                    // Agent field uses click/tab to cycle, not typed input
                                }
                            }
                        }
                    }
                    _ => {
                        // Ignore other keys
                    }
                }
            }
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
                                Button::new(
                                    "open-all-btn",
                                    format!("Open All ({})", stopped_count),
                                )
                                .variant(ButtonVariant::Success)
                                .disabled(stopped_count == 0)
                                .on_click(cx.listener(
                                    |view, _, _, cx| {
                                        view.on_open_all_click(cx);
                                    },
                                )),
                            )
                            // Stop All button - Warning variant
                            .child(
                                Button::new(
                                    "stop-all-btn",
                                    format!("Stop All ({})", running_count),
                                )
                                .variant(ButtonVariant::Warning)
                                .disabled(running_count == 0)
                                .on_click(cx.listener(
                                    |view, _, _, cx| {
                                        view.on_stop_all_click(cx);
                                    },
                                )),
                            )
                            // Refresh button - Ghost variant
                            .child(
                                Button::new("refresh-btn", "Refresh")
                                    .variant(ButtonVariant::Ghost)
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.on_refresh_click(cx);
                                    })),
                            )
                            // Create button - Primary variant
                            .child(
                                Button::new("create-header-btn", "+ Create")
                                    .variant(ButtonVariant::Primary)
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.on_create_button_click(cx);
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
                                .child(
                                    Button::new("dismiss-errors", "×")
                                        .variant(ButtonVariant::Ghost)
                                        .on_click(cx.listener(|view, _, _, cx| {
                                            view.on_dismiss_errors(cx);
                                        })),
                                ),
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
                                    Button::new("dismiss-bulk-errors", "×")
                                        .variant(ButtonVariant::Ghost)
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
            // Main content: 3-column layout (sidebar | kild list | detail panel)
            .child(
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
            // Dialog rendering (based on current dialog state)
            .when(self.state.dialog().is_create(), |this| {
                this.child(create_dialog::render_create_dialog(self.state.dialog(), cx))
            })
            .when(self.state.dialog().is_confirm(), |this| {
                this.child(confirm_dialog::render_confirm_dialog(
                    self.state.dialog(),
                    cx,
                ))
            })
            .when(self.state.dialog().is_add_project(), |this| {
                this.child(add_project_dialog::render_add_project_dialog(
                    self.state.dialog(),
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
