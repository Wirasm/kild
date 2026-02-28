use kild_core::SessionSnapshot;
use kild_core::projects::{Project, ProjectManager};

use crate::state::dialog::DialogState;
use crate::state::errors::{OperationError, OperationErrors};
use crate::state::loading::LoadingState;
use crate::state::selection::SelectionState;
use crate::state::sessions::SessionStore;

/// Main application state.
///
/// All fields are private - access state through the facade methods below.
/// This ensures all state mutations go through controlled methods that
/// maintain invariants and provide a consistent API.
pub struct AppState {
    /// Session display data with refresh tracking.
    pub(super) sessions: SessionStore,

    /// Current dialog state (mutually exclusive - only one dialog can be open).
    pub(super) dialog: DialogState,

    /// Operation errors (per-branch and bulk).
    pub(super) errors: OperationErrors,

    /// Kild selection state (for detail panel).
    pub(super) selection: SelectionState,

    /// Project management with enforced invariants.
    pub(super) projects: ProjectManager,

    /// Startup errors that should be shown to the user (migration failures, load errors).
    pub(super) startup_errors: Vec<String>,

    /// In-progress operation tracking (prevents double-dispatch).
    pub(super) loading: LoadingState,
}

impl AppState {
    /// Create new application state, loading sessions from disk.
    pub fn new() -> Self {
        let mut startup_errors = Vec::new();

        // Migrate projects to canonical paths (fixes case mismatch on macOS)
        if let Err(e) = kild_core::projects::migrate_projects_to_canonical() {
            tracing::error!(
                event = "ui.projects.migration_failed",
                error = %e,
                "Project migration failed - some projects may not filter correctly"
            );
            startup_errors.push(format!("Project migration failed: {}", e));
        }

        // Load projects from disk (after migration)
        let projects_data = kild_core::projects::load_projects();
        if let Some(load_error) = projects_data.load_error {
            startup_errors.push(load_error);
        }
        let projects = ProjectManager::from_data(projects_data.projects, projects_data.active);

        Self {
            sessions: SessionStore::new(),
            dialog: DialogState::None,
            errors: OperationErrors::new(),
            selection: SelectionState::default(),
            projects,
            startup_errors,
            loading: LoadingState::new(),
        }
    }

    /// Refresh sessions from disk.
    pub fn refresh_sessions(&mut self) {
        self.sessions.refresh();
    }

    /// Update only the process status of existing kilds without reloading from disk.
    ///
    /// This is faster than refresh_sessions() for status polling because it:
    /// - Doesn't reload session files from disk (unless count mismatch detected)
    /// - Only checks if tracked processes are still running
    /// - Preserves the existing kild list structure
    ///
    /// If the session count on disk differs from the in-memory count (indicating
    /// external create/destroy operations), triggers a full refresh instead.
    ///
    /// Note: This does NOT update git status or diff stats. Use `refresh_sessions()`
    /// for a full refresh that includes git information.
    pub fn update_statuses_only(&mut self) {
        self.sessions.update_statuses_only();
    }

    /// Apply core events to update application state.
    ///
    /// Maps each `Event` variant to the appropriate state mutations.
    /// Called after successful `CoreStore::dispatch()` to drive UI updates
    /// from the event stream rather than manual side-effect code.
    pub fn apply_events(&mut self, events: &[kild_core::Event]) {
        tracing::debug!(
            event = "ui.state.apply_events_started",
            count = events.len()
        );

        for ev in events {
            tracing::debug!(event = "ui.state.event_applied", event_type = ?ev);

            match ev {
                kild_core::Event::KildCreated { .. } => {
                    self.close_dialog();
                    self.refresh_sessions();
                }
                kild_core::Event::KildDestroyed { branch } => {
                    self.clear_selection_if_matches(branch);
                    self.close_dialog();
                    self.refresh_sessions();
                }
                kild_core::Event::KildOpened { .. } => {
                    self.refresh_sessions();
                }
                kild_core::Event::KildStopped { .. } => {
                    self.refresh_sessions();
                }
                kild_core::Event::KildCompleted { branch } => {
                    self.clear_selection_if_matches(branch);
                    self.refresh_sessions();
                }
                kild_core::Event::PrStatusRefreshed { .. } => {
                    // PR sidecar updated — sessions list will pick it up on next refresh
                }
                kild_core::Event::SessionsRefreshed => {
                    // Already handled by the refresh call that produced this event
                }
                kild_core::Event::ProjectAdded { .. } => {
                    self.reload_projects();
                    self.close_dialog();
                    self.refresh_sessions();
                }
                kild_core::Event::ProjectRemoved { .. } => {
                    self.reload_projects();
                    self.refresh_sessions();
                    // Don't close dialog — removal isn't initiated from a modal,
                    // so there's no dialog to dismiss (unlike ProjectAdded).
                }
                kild_core::Event::ActiveProjectChanged { .. } => {
                    self.reload_projects();
                }
                kild_core::Event::AgentStatusUpdated { .. } => {
                    self.refresh_sessions();
                }
            }
        }
    }

    /// Clear selection if the currently selected kild matches the given branch.
    fn clear_selection_if_matches(&mut self, branch: &kild_core::BranchName) {
        if self
            .selected_kild()
            .is_some_and(|s| s.session.branch == *branch)
        {
            self.clear_selection();
        }
    }

    /// Close any open dialog.
    pub fn close_dialog(&mut self) {
        self.dialog = DialogState::None;
    }

    /// Open the create dialog.
    pub fn open_create_dialog(&mut self) {
        self.dialog = DialogState::open_create();
    }

    /// Open the confirm dialog for a specific branch.
    ///
    /// Fetches safety information (uncommitted changes, unpushed commits, etc.)
    /// to display warnings in the dialog.
    pub fn open_confirm_dialog(&mut self, branch: String) {
        // Fetch safety info (best-effort, don't block on failure)
        let safety_info = match kild_core::session_ops::get_destroy_safety_info(&branch) {
            Ok(info) => {
                tracing::debug!(
                    event = "ui.confirm_dialog.safety_info_fetched",
                    branch = %branch,
                    should_block = info.should_block(),
                    has_warnings = info.has_warnings()
                );
                Some(info)
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.confirm_dialog.safety_info_failed",
                    branch = %branch,
                    error = %e,
                    "Failed to fetch safety info - proceeding without warnings"
                );
                None
            }
        };

        self.dialog = DialogState::open_confirm(branch, safety_info);
    }

    /// Open the add project dialog.
    pub fn open_add_project_dialog(&mut self) {
        self.dialog = DialogState::open_add_project();
    }

    /// Set error message in the current dialog.
    /// No-op if no dialog is open.
    pub fn set_dialog_error(&mut self, error: String) {
        match &mut self.dialog {
            DialogState::None => {
                tracing::warn!(
                    event = "ui.state.set_dialog_error_no_dialog",
                    "Attempted to set dialog error but no dialog is open"
                );
            }
            DialogState::Create { error: e, .. } => *e = Some(error),
            DialogState::Confirm { error: e, .. } => *e = Some(error),
            DialogState::AddProject { error: e, .. } => *e = Some(error),
        }
    }

    /// Clear the error for a specific branch.
    pub fn clear_error(&mut self, branch: &str) {
        self.errors.clear(branch);
    }

    /// Get the project ID for the active project.
    pub fn active_project_id(&self) -> Option<String> {
        self.projects
            .active_path()
            .map(|p| kild_core::projects::generate_project_id(p).to_string())
    }

    /// Get displays filtered by active project.
    ///
    /// Filters kilds where `session.project_id` matches the derived ID of the active project path.
    /// Uses path-based hashing that matches kild-core's `generate_project_id`.
    /// If no active project is set, returns all displays (unfiltered).
    pub fn filtered_displays(&self) -> Vec<&SessionSnapshot> {
        self.sessions
            .filtered_by_project(self.active_project_id().as_deref())
    }

    /// Count kilds for a specific project (by project path).
    pub fn kild_count_for_project(&self, project_path: &std::path::Path) -> usize {
        let project_id = kild_core::projects::generate_project_id(project_path);
        self.sessions.kild_count_for_project(&project_id)
    }

    /// Count total kilds across all projects.
    pub fn total_kild_count(&self) -> usize {
        self.sessions.total_count()
    }

    /// Get the selected kild display, if any.
    ///
    /// Returns `None` if no kild is selected or if the selected kild no longer
    /// exists in the current display list (e.g., after being destroyed externally).
    pub fn selected_kild(&self) -> Option<&SessionSnapshot> {
        let id = self.selection.id()?;

        match self
            .sessions
            .displays()
            .iter()
            .find(|d| &*d.session.id == id)
        {
            Some(kild) => Some(kild),
            None => {
                tracing::debug!(
                    event = "ui.state.stale_selection",
                    selected_id = id,
                    "Selected kild not found in current display list"
                );
                None
            }
        }
    }

    /// Clear selection (e.g., when kild is destroyed).
    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    // =========================================================================
    // Dialog facade methods
    // =========================================================================

    /// Get read-only reference to dialog state.
    ///
    /// Use this for pattern matching and reading dialog data.
    pub fn dialog(&self) -> &DialogState {
        &self.dialog
    }

    /// Get mutable reference to dialog state.
    ///
    /// Use this for direct form field mutation in keyboard handlers.
    pub fn dialog_mut(&mut self) -> &mut DialogState {
        &mut self.dialog
    }

    // =========================================================================
    // Error facade methods
    // =========================================================================

    /// Set an error for a specific branch.
    pub fn set_error(&mut self, branch: &str, error: OperationError) {
        self.errors.set(branch, error);
    }

    /// Get the error for a specific branch, if any.
    pub fn get_error(&self, branch: &str) -> Option<&OperationError> {
        self.errors.get(branch)
    }

    // =========================================================================
    // Error banner facade methods
    // =========================================================================

    /// Get errors that should be shown to the user in the error banner.
    pub fn banner_errors(&self) -> &[String] {
        &self.startup_errors
    }

    /// Check if there are any banner errors.
    pub fn has_banner_errors(&self) -> bool {
        !self.startup_errors.is_empty()
    }

    /// Add an error to the banner (for runtime failures the user should see).
    pub fn push_error(&mut self, message: String) {
        self.startup_errors.push(message);
    }

    /// Dismiss all banner errors (user acknowledged them).
    pub fn dismiss_errors(&mut self) {
        self.startup_errors.clear();
    }

    // =========================================================================
    // Loading facade methods
    // =========================================================================

    /// Mark a branch as having an in-flight operation.
    pub fn set_loading(&mut self, branch: &str) {
        self.loading.set_branch(branch);
    }

    /// Clear the in-flight operation for a branch.
    pub fn clear_loading(&mut self, branch: &str) {
        self.loading.clear_branch(branch);
    }

    /// Check if a branch has an in-flight operation.
    pub fn is_loading(&self, branch: &str) -> bool {
        self.loading.is_branch_loading(branch)
    }

    /// Mark a dialog operation as in-flight.
    pub fn set_dialog_loading(&mut self) {
        self.loading.set_dialog();
    }

    /// Clear the dialog operation flag.
    pub fn clear_dialog_loading(&mut self) {
        self.loading.clear_dialog();
    }

    /// Check if a dialog operation is in-flight.
    pub fn is_dialog_loading(&self) -> bool {
        self.loading.is_dialog()
    }

    // =========================================================================
    // Selection facade methods
    // =========================================================================

    /// Select a kild by ID.
    pub fn select_kild(&mut self, id: String) {
        self.selection.select(id);
    }

    /// Get the selected kild ID, if any.
    pub fn selected_id(&self) -> Option<&str> {
        self.selection.id()
    }

    /// Check if a kild is selected.
    #[allow(dead_code)]
    pub fn has_selection(&self) -> bool {
        self.selection.has_selection()
    }

    // =========================================================================
    // Project facade methods
    // =========================================================================

    /// Reload projects from disk, replacing in-memory state.
    ///
    /// Used to recover from state desync (e.g., disk write succeeded but
    /// in-memory update failed).
    pub fn reload_projects(&mut self) {
        let data = kild_core::projects::load_projects();
        if let Some(load_error) = data.load_error {
            self.startup_errors.push(load_error);
        }
        self.projects = ProjectManager::from_data(data.projects, data.active);
    }

    /// Get the active project, if any.
    pub fn active_project(&self) -> Option<&Project> {
        self.projects.active()
    }

    /// Get the active project's path, if any.
    pub fn active_project_path(&self) -> Option<&std::path::Path> {
        self.projects.active_path()
    }

    /// Iterate over all projects.
    pub fn projects_iter(&self) -> impl Iterator<Item = &Project> {
        self.projects.iter()
    }

    /// Check if the project list is empty.
    #[allow(dead_code)]
    pub fn projects_is_empty(&self) -> bool {
        self.projects.is_empty()
    }

    // =========================================================================
    // Session facade methods
    // =========================================================================

    /// Get all session displays.
    pub fn displays(&self) -> &[SessionSnapshot] {
        self.sessions.displays()
    }

    /// Get the load error from the last refresh attempt, if any.
    #[allow(dead_code)]
    pub fn load_error(&self) -> Option<&str> {
        self.sessions.load_error()
    }

    /// Check if there are no session displays.
    #[allow(dead_code)]
    pub fn sessions_is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    // =========================================================================
    // Test-only methods
    // =========================================================================

    /// Create an AppState for testing with empty state.
    #[cfg(test)]
    pub fn test_new() -> Self {
        Self {
            sessions: SessionStore::from_data(Vec::new(), None),
            dialog: DialogState::None,
            errors: OperationErrors::new(),
            selection: SelectionState::default(),
            projects: ProjectManager::new(),
            startup_errors: Vec::new(),
            loading: LoadingState::new(),
        }
    }

    /// Set the dialog state directly (for testing).
    #[cfg(test)]
    pub fn set_dialog(&mut self, dialog: DialogState) {
        self.dialog = dialog;
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
