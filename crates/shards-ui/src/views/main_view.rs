//! Main view for shards-ui.
//!
//! Root view that composes header, shard list, create dialog, and confirm dialog.
//! Handles keyboard input and dialog state management.

use gpui::{
    Context, FocusHandle, Focusable, FontWeight, IntoElement, KeyDownEvent, Render, Task, Window,
    div, prelude::*, rgb,
};
use tracing::{debug, warn};

use std::path::PathBuf;

use crate::actions;
use crate::state::{AddProjectDialogField, AppState, CreateDialogField};
use crate::views::{
    add_project_dialog, confirm_dialog, create_dialog, project_selector, shard_list,
};

/// Normalize user-entered path for project addition.
///
/// Handles:
/// - Whitespace trimming (leading/trailing spaces removed)
/// - Tilde expansion (~/ -> home directory, or ~ alone)
/// - Missing leading slash (users/... -> /users/... if valid directory)
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
            return Ok(home.join(rest));
        }
        if path_str == "~" {
            return Ok(home);
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
                return Ok(with_slash);
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

    Ok(PathBuf::from(path_str))
}

/// Main application view that composes the shard list, header, and create dialog.
///
/// Owns application state and handles keyboard input for the create dialog.
pub struct MainView {
    state: AppState,
    focus_handle: FocusHandle,
    /// Handle to the background refresh task. Must be stored to prevent cancellation.
    _refresh_task: Task<()>,
}

impl MainView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let refresh_task = cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            tracing::debug!(event = "ui.auto_refresh.started");

            loop {
                cx.background_executor()
                    .timer(crate::refresh::REFRESH_INTERVAL)
                    .await;

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

        Self {
            state: AppState::new(),
            focus_handle: cx.focus_handle(),
            _refresh_task: refresh_task,
        }
    }

    /// Handle click on the Create button in header.
    fn on_create_button_click(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.create_dialog.opened");
        self.state.show_create_dialog = true;
        cx.notify();
    }

    /// Handle dialog cancel button click.
    pub fn on_dialog_cancel(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.create_dialog.cancelled");
        self.state.show_create_dialog = false;
        self.state.reset_create_form();
        cx.notify();
    }

    /// Handle dialog submit button click.
    pub fn on_dialog_submit(&mut self, cx: &mut Context<Self>) {
        let branch = self.state.create_form.branch_name.trim().to_string();
        let agent = self.state.create_form.selected_agent();
        let note = if self.state.create_form.note.trim().is_empty() {
            None
        } else {
            Some(self.state.create_form.note.trim().to_string())
        };

        match actions::create_shard(&branch, &agent, note) {
            Ok(_session) => {
                // Success - close dialog and refresh list
                self.state.show_create_dialog = false;
                self.state.reset_create_form();
                self.state.refresh_sessions();
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.dialog_submit.error_displayed",
                    branch = %branch,
                    agent = %agent,
                    error = %e
                );
                self.state.create_error = Some(e);
            }
        }
        cx.notify();
    }

    /// Cycle to the next agent in the list.
    pub fn on_agent_cycle(&mut self, cx: &mut Context<Self>) {
        let agents = create_dialog::agent_options();
        if agents.is_empty() {
            tracing::error!(event = "ui.create_dialog.no_agents_available");
            self.state.create_error =
                Some("No agents available. Check shards-core configuration.".to_string());
            cx.notify();
            return;
        }
        let next_index = (self.state.create_form.selected_agent_index + 1) % agents.len();
        self.state.create_form.selected_agent_index = next_index;
        tracing::info!(
            event = "ui.create_dialog.agent_changed",
            agent = %self.state.create_form.selected_agent()
        );
        cx.notify();
    }

    /// Handle click on the Refresh button in header.
    fn on_refresh_click(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.refresh_clicked");
        self.state.refresh_sessions();
        cx.notify();
    }

    /// Handle click on the destroy button [×] in a shard row.
    pub fn on_destroy_click(&mut self, branch: &str, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.destroy_dialog.opened", branch = branch);
        self.state.confirm_target_branch = Some(branch.to_string());
        self.state.show_confirm_dialog = true;
        cx.notify();
    }

    /// Handle confirm button click in destroy dialog.
    pub fn on_confirm_destroy(&mut self, cx: &mut Context<Self>) {
        let Some(branch) = self.state.confirm_target_branch.clone() else {
            tracing::warn!(event = "ui.confirm_destroy.no_target");
            return;
        };

        match actions::destroy_shard(&branch) {
            Ok(()) => {
                self.state.reset_confirm_dialog();
                self.state.refresh_sessions();
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.confirm_destroy.error_displayed",
                    branch = %branch,
                    error = %e
                );
                self.state.confirm_error = Some(e);
            }
        }
        cx.notify();
    }

    /// Handle cancel button click in destroy dialog.
    pub fn on_confirm_cancel(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.confirm_dialog.cancelled");
        self.state.reset_confirm_dialog();
        cx.notify();
    }

    /// Handle click on the Open button [▶] in a shard row.
    pub fn on_open_click(&mut self, branch: &str, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.open_clicked", branch = branch);
        self.state.clear_open_error();

        match actions::open_shard(branch, None) {
            Ok(_session) => {
                self.state.refresh_sessions();
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.open_click.error_displayed",
                    branch = branch,
                    error = %e
                );
                self.state.open_error = Some(crate::state::OperationError {
                    branch: branch.to_string(),
                    message: e,
                });
            }
        }
        cx.notify();
    }

    /// Handle click on the Stop button [⏹] in a shard row.
    pub fn on_stop_click(&mut self, branch: &str, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.stop_clicked", branch = branch);
        self.state.clear_stop_error();

        match actions::stop_shard(branch) {
            Ok(()) => {
                self.state.refresh_sessions();
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.stop_click.error_displayed",
                    branch = branch,
                    error = %e
                );
                self.state.stop_error = Some(crate::state::OperationError {
                    branch: branch.to_string(),
                    message: e,
                });
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
        operation: impl Fn(&[crate::state::ShardDisplay]) -> (usize, Vec<crate::state::OperationError>),
        error_event: &str,
        cx: &mut Context<Self>,
    ) {
        self.state.clear_bulk_errors();

        let (count, errors) = operation(&self.state.displays);

        for error in &errors {
            tracing::warn!(
                event = error_event,
                branch = error.branch,
                error = error.message
            );
        }
        self.state.bulk_errors = errors;

        if count > 0 || !self.state.bulk_errors.is_empty() {
            self.state.refresh_sessions();
        }
        cx.notify();
    }

    /// Handle click on the Copy Path button in a shard row.
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

    /// Handle click on the Open Editor button in a shard row.
    ///
    /// Opens the worktree in the user's preferred editor ($EDITOR or zed).
    /// Surfaces any errors inline in the shard row.
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
        self.state.clear_editor_error();

        if let Err(e) = actions::open_in_editor(worktree_path) {
            tracing::warn!(
                event = "ui.open_editor_click.error_displayed",
                branch = branch,
                error = %e
            );
            self.state.editor_error = Some(crate::state::OperationError {
                branch: branch.to_string(),
                message: e,
            });
        }
        cx.notify();
    }

    /// Handle click on the Focus Terminal button in a shard row.
    ///
    /// Requires both `terminal_type` and `window_id` to be present. If either is
    /// missing (e.g., session started before window tracking was implemented),
    /// surfaces an error to the user explaining the limitation.
    ///
    /// Also surfaces any errors from the underlying `focus_terminal` operation.
    pub fn on_focus_terminal_click(
        &mut self,
        terminal_type: Option<&shards_core::terminal::types::TerminalType>,
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
        self.state.clear_focus_error();

        let result = if let (Some(tt), Some(wid)) = (terminal_type, window_id) {
            shards_core::terminal_ops::focus_terminal(tt, wid)
                .map_err(|e| format!("Failed to focus terminal: {}", e))
        } else if terminal_type.is_none() && window_id.is_none() {
            tracing::debug!(
                event = "ui.focus_terminal_no_window_info",
                branch = branch,
                message = "Legacy session - no terminal info recorded"
            );
            Err("Terminal window info not available. This session was created before window tracking was added.".to_string())
        } else {
            tracing::warn!(
                event = "ui.focus_terminal_inconsistent_state",
                branch = branch,
                has_terminal_type = terminal_type.is_some(),
                has_window_id = window_id.is_some(),
                message = "Inconsistent terminal state - one field present, one missing"
            );
            Err(
                "Terminal window info is incomplete. Try stopping and reopening the shard."
                    .to_string(),
            )
        };

        if let Err(e) = result {
            tracing::warn!(
                event = "ui.focus_terminal_click.error_displayed",
                branch = branch,
                error = %e
            );
            self.state.focus_error = Some(crate::state::OperationError {
                branch: branch.to_string(),
                message: e,
            });
        }
        cx.notify();
    }

    /// Clear bulk operation errors (called when user dismisses the banner).
    fn on_dismiss_bulk_errors(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.bulk_errors.dismissed");
        self.state.clear_bulk_errors();
        cx.notify();
    }

    // --- Project management handlers ---

    /// Handle click on Add Project button.
    pub fn on_add_project_click(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.add_project_dialog.opened");
        self.state.show_add_project_dialog = true;
        self.state.show_project_dropdown = false;
        cx.notify();
    }

    /// Handle add project dialog cancel.
    pub fn on_add_project_cancel(&mut self, cx: &mut Context<Self>) {
        tracing::info!(event = "ui.add_project_dialog.cancelled");
        self.state.show_add_project_dialog = false;
        self.state.reset_add_project_form();
        cx.notify();
    }

    /// Handle add project dialog submit.
    pub fn on_add_project_submit(&mut self, cx: &mut Context<Self>) {
        let path_str = self.state.add_project_form.path.trim().to_string();
        let name = if self.state.add_project_form.name.trim().is_empty() {
            None
        } else {
            Some(self.state.add_project_form.name.trim().to_string())
        };

        if path_str.is_empty() {
            self.state.add_project_error = Some("Path cannot be empty".to_string());
            cx.notify();
            return;
        }

        // Normalize path: expand ~ and ensure absolute path
        let path = match normalize_project_path(&path_str) {
            Ok(p) => p,
            Err(e) => {
                self.state.add_project_error = Some(e);
                cx.notify();
                return;
            }
        };

        match actions::add_project(path.clone(), name) {
            Ok(project) => {
                tracing::info!(
                    event = "ui.add_project.succeeded",
                    path = %path.display(),
                    name = %project.name
                );
                // Update local state with new project
                self.state.projects.push(project);
                if self.state.projects.len() == 1 {
                    self.state.active_project = Some(path);
                }
                self.state.show_add_project_dialog = false;
                self.state.reset_add_project_form();
                // Refresh sessions to filter by new active project
                self.state.refresh_sessions();
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.add_project.error_displayed",
                    path = %path.display(),
                    error = %e
                );
                self.state.add_project_error = Some(e);
            }
        }
        cx.notify();
    }

    /// Handle project selection from dropdown.
    pub fn on_project_select(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        tracing::info!(
            event = "ui.project_selected",
            path = %path.display()
        );

        if let Err(e) = actions::set_active_project(Some(path.clone())) {
            tracing::error!(event = "ui.project_select.failed", error = %e);
            self.state.add_project_error = Some(format!("Failed to save project selection: {}", e));
            cx.notify();
            return;
        }

        self.state.active_project = Some(path);
        self.state.show_project_dropdown = false;
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
            self.state.add_project_error = Some(format!("Failed to remove project: {}", e));
            cx.notify();
            return;
        }

        // Update local state
        self.state.projects.retain(|p| p.path != path);
        if self.state.active_project.as_ref() == Some(&path) {
            self.state.active_project = self.state.projects.first().map(|p| p.path.clone());
        }
        self.state.show_project_dropdown = false;
        cx.notify();
    }

    /// Toggle project dropdown open/closed.
    pub fn on_toggle_project_dropdown(&mut self, cx: &mut Context<Self>) {
        self.state.show_project_dropdown = !self.state.show_project_dropdown;
        cx.notify();
    }

    /// Render a bulk operation button with consistent styling.
    fn render_bulk_button(
        &self,
        id: &'static str,
        label: &str,
        count: usize,
        enabled_bg: u32,
        enabled_hover: u32,
        on_click: impl Fn(&gpui::MouseUpEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> impl IntoElement {
        let is_disabled = count == 0;
        let bg_color = if is_disabled {
            rgb(0x333333)
        } else {
            rgb(enabled_bg)
        };
        let hover_color = if is_disabled {
            rgb(0x333333)
        } else {
            rgb(enabled_hover)
        };
        let text_color = if is_disabled {
            rgb(0x666666)
        } else {
            rgb(0xffffff)
        };

        div()
            .id(id)
            .px_3()
            .py_1()
            .bg(bg_color)
            .when(!is_disabled, |d| d.hover(|style| style.bg(hover_color)))
            .rounded_md()
            .when(!is_disabled, |d| d.cursor_pointer())
            .when(!is_disabled, |d| {
                d.on_mouse_up(gpui::MouseButton::Left, on_click)
            })
            .child(
                div()
                    .text_color(text_color)
                    .child(format!("{} ({})", label, count)),
            )
    }

    /// Handle keyboard input for dialogs.
    ///
    /// When create dialog is open: handles branch name input (alphanumeric, -, _, /, space converts to hyphen),
    /// form submission (Enter), dialog dismissal (Escape), and agent cycling (Tab).
    /// When confirm dialog is open: handles dialog dismissal (Escape).
    /// When add project dialog is open: handles path/name input, submission, and dismissal.
    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let key_str = event.keystroke.key.to_string();

        // Handle confirm dialog escape
        if self.state.show_confirm_dialog && key_str == "escape" {
            self.on_confirm_cancel(cx);
            return;
        }

        // Handle add project dialog keyboard input
        if self.state.show_add_project_dialog {
            match key_str.as_str() {
                "backspace" => {
                    match self.state.add_project_form.focused_field {
                        AddProjectDialogField::Path => {
                            self.state.add_project_form.path.pop();
                        }
                        AddProjectDialogField::Name => {
                            self.state.add_project_form.name.pop();
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
                    self.state.add_project_form.focused_field =
                        match self.state.add_project_form.focused_field {
                            AddProjectDialogField::Path => AddProjectDialogField::Name,
                            AddProjectDialogField::Name => AddProjectDialogField::Path,
                        };
                    cx.notify();
                }
                key if key.len() == 1 => {
                    if let Some(c) = key.chars().next() {
                        // Path and name fields accept most characters (file paths can have spaces, etc.)
                        if !c.is_control() {
                            match self.state.add_project_form.focused_field {
                                AddProjectDialogField::Path => {
                                    self.state.add_project_form.path.push(c);
                                }
                                AddProjectDialogField::Name => {
                                    self.state.add_project_form.name.push(c);
                                }
                            }
                            cx.notify();
                        }
                    }
                }
                "space" => {
                    // Allow spaces in both path and name
                    match self.state.add_project_form.focused_field {
                        AddProjectDialogField::Path => {
                            self.state.add_project_form.path.push(' ');
                        }
                        AddProjectDialogField::Name => {
                            self.state.add_project_form.name.push(' ');
                        }
                    }
                    cx.notify();
                }
                _ => {
                    // Ignore other keys
                }
            }
            return;
        }

        // Create dialog keyboard handling
        if !self.state.show_create_dialog {
            return;
        }

        match key_str.as_str() {
            "backspace" => {
                match self.state.create_form.focused_field {
                    CreateDialogField::BranchName => {
                        self.state.create_form.branch_name.pop();
                    }
                    CreateDialogField::Note => {
                        self.state.create_form.note.pop();
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
                match self.state.create_form.focused_field {
                    CreateDialogField::BranchName => {
                        // Convert spaces to hyphens for branch names
                        self.state.create_form.branch_name.push('-');
                    }
                    CreateDialogField::Note => {
                        // Allow actual spaces in notes
                        self.state.create_form.note.push(' ');
                    }
                    CreateDialogField::Agent => {}
                }
                cx.notify();
            }
            "tab" => {
                // Cycle focus between fields
                self.state.create_form.focused_field = match self.state.create_form.focused_field {
                    CreateDialogField::BranchName => CreateDialogField::Agent,
                    CreateDialogField::Agent => CreateDialogField::Note,
                    CreateDialogField::Note => CreateDialogField::BranchName,
                };
                cx.notify();
            }
            key if key.len() == 1 => {
                if let Some(c) = key.chars().next() {
                    match self.state.create_form.focused_field {
                        CreateDialogField::BranchName => {
                            // Branch names: alphanumeric, -, _, /
                            if c.is_alphanumeric() || c == '-' || c == '_' || c == '/' {
                                self.state.create_form.branch_name.push(c);
                                cx.notify();
                            }
                        }
                        CreateDialogField::Note => {
                            // Notes: any non-control character
                            if !c.is_control() {
                                self.state.create_form.note.push(c);
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
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e1e))
            // Header with title, Refresh button, and Create button
            .child(
                div()
                    .px_4()
                    .py_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .text_xl()
                                    .text_color(rgb(0xffffff))
                                    .font_weight(FontWeight::BOLD)
                                    .child("Shards"),
                            )
                            // Project selector dropdown
                            .child(project_selector::render_project_selector(&self.state, cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            // Open All button - green when enabled
                            .child(self.render_bulk_button(
                                "open-all-btn",
                                "Open All",
                                self.state.stopped_count(),
                                0x446644,
                                0x557755,
                                cx.listener(|view, _, _, cx| view.on_open_all_click(cx)),
                            ))
                            // Stop All button - red when enabled
                            .child(self.render_bulk_button(
                                "stop-all-btn",
                                "Stop All",
                                self.state.running_count(),
                                0x664444,
                                0x775555,
                                cx.listener(|view, _, _, cx| view.on_stop_all_click(cx)),
                            ))
                            // Refresh button - TEXT label, gray background (secondary action)
                            .child(
                                div()
                                    .id("refresh-btn")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(0x444444))
                                    .hover(|style| style.bg(rgb(0x555555)))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(|view, _, _, cx| {
                                            view.on_refresh_click(cx);
                                        }),
                                    )
                                    .child(div().text_color(rgb(0xffffff)).child("Refresh")),
                            )
                            // Create button - blue/accent background (primary action)
                            .child(
                                div()
                                    .id("create-header-btn")
                                    .px_3()
                                    .py_1()
                                    .bg(rgb(0x4a9eff))
                                    .hover(|style| style.bg(rgb(0x5aafff)))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .on_mouse_up(
                                        gpui::MouseButton::Left,
                                        cx.listener(|view, _, _, cx| {
                                            view.on_create_button_click(cx);
                                        }),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(div().text_color(rgb(0xffffff)).child("+"))
                                            .child(div().text_color(rgb(0xffffff)).child("Create")),
                                    ),
                            ),
                    ),
            )
            // Bulk operation errors banner (dismissible)
            .when(!self.state.bulk_errors.is_empty(), |this| {
                let error_count = self.state.bulk_errors.len();
                this.child(
                    div()
                        .mx_4()
                        .mt_2()
                        .px_4()
                        .py_2()
                        .bg(rgb(0x662222))
                        .rounded_md()
                        .flex()
                        .flex_col()
                        .gap_1()
                        // Header with dismiss button
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_color(rgb(0xff6b6b))
                                        .font_weight(FontWeight::BOLD)
                                        .child(format!(
                                            "{} operation{} failed:",
                                            error_count,
                                            if error_count == 1 { "" } else { "s" }
                                        )),
                                )
                                .child(
                                    div()
                                        .id("dismiss-bulk-errors")
                                        .px_2()
                                        .cursor_pointer()
                                        .text_color(rgb(0xaaaaaa))
                                        .hover(|style| style.text_color(rgb(0xffffff)))
                                        .on_mouse_up(
                                            gpui::MouseButton::Left,
                                            cx.listener(|view, _, _, cx| {
                                                view.on_dismiss_bulk_errors(cx);
                                            }),
                                        )
                                        .child("×"),
                                ),
                        )
                        // Error list
                        .children(self.state.bulk_errors.iter().map(|e| {
                            div()
                                .text_sm()
                                .text_color(rgb(0xffaaaa))
                                .child(format!("• {}: {}", e.branch, e.message))
                        })),
                )
            })
            // Shard list
            .child(shard_list::render_shard_list(&self.state, cx))
            // Create dialog (conditional)
            .when(self.state.show_create_dialog, |this| {
                this.child(create_dialog::render_create_dialog(&self.state, cx))
            })
            // Confirm dialog (conditional)
            .when(self.state.show_confirm_dialog, |this| {
                this.child(confirm_dialog::render_confirm_dialog(&self.state, cx))
            })
            // Add project dialog (conditional)
            .when(self.state.show_add_project_dialog, |this| {
                this.child(add_project_dialog::render_add_project_dialog(
                    &self.state,
                    cx,
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_with_leading_slash() {
        let result = normalize_project_path("/Users/test/project").unwrap();
        assert_eq!(result, PathBuf::from("/Users/test/project"));
    }

    #[test]
    fn test_normalize_path_tilde_expansion() {
        let result = normalize_project_path("~/projects/test").unwrap();
        let expected_home = dirs::home_dir().expect("test requires home dir");
        assert_eq!(result, expected_home.join("projects/test"));
    }

    #[test]
    fn test_normalize_path_bare_tilde() {
        let result = normalize_project_path("~").unwrap();
        let expected_home = dirs::home_dir().expect("test requires home dir");
        assert_eq!(result, expected_home);
    }

    #[test]
    fn test_normalize_path_trims_whitespace() {
        let result = normalize_project_path("  /Users/test/project  ").unwrap();
        assert_eq!(result, PathBuf::from("/Users/test/project"));
    }

    #[test]
    fn test_normalize_path_without_leading_slash_fallback() {
        // Non-existent path should remain unchanged (passthrough)
        let result = normalize_project_path("nonexistent/path/here").unwrap();
        assert_eq!(result, PathBuf::from("nonexistent/path/here"));
    }

    #[test]
    fn test_normalize_path_empty_string() {
        // Empty input passes through as empty path
        // Caller validates for empty before calling this function
        let result = normalize_project_path("").unwrap();
        assert_eq!(result, PathBuf::from(""));
    }

    #[test]
    fn test_normalize_path_whitespace_only() {
        // Whitespace-only trims to empty, passes through
        // Caller validates for empty before calling this function
        let result = normalize_project_path("   ").unwrap();
        assert_eq!(result, PathBuf::from(""));
    }

    #[test]
    fn test_normalize_path_tilde_in_middle_not_expanded() {
        let result = normalize_project_path("/Users/test/~project").unwrap();
        assert_eq!(result, PathBuf::from("/Users/test/~project"));
    }
}
