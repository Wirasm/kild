//! Application state for kild-ui.
//!
//! Centralized state management for the GUI, including kild list,
//! create dialog, and form state.
//!
//! ## State Modules
//!
//! AppState is composed of several specialized modules that encapsulate related state:
//! - `DialogState`: Mutually exclusive dialog states (create, confirm, add project)
//! - More modules to be added in future refactoring phases

use kild_core::projects::{Project, ProjectError, ProjectManager};
use kild_core::{DestroySafetyInfo, ProcessStatus, SessionInfo};

// =============================================================================
// Dialog State
// =============================================================================

/// Dialog state for the application.
///
/// Only one dialog can be open at a time. This enum enforces mutual exclusion
/// at compile-time, preventing impossible states like having both the create
/// and confirm dialogs open simultaneously.
#[derive(Clone, Debug, Default)]
pub enum DialogState {
    /// No dialog is open.
    #[default]
    None,
    /// Create kild dialog is open.
    Create {
        form: CreateFormState,
        error: Option<String>,
    },
    /// Confirm destroy dialog is open.
    Confirm {
        /// Branch being destroyed.
        branch: String,
        /// Safety information for the destroy operation.
        /// None if the safety check failed (proceed without warnings).
        safety_info: Option<DestroySafetyInfo>,
        error: Option<String>,
    },
    /// Add project dialog is open.
    AddProject {
        form: AddProjectFormState,
        error: Option<String>,
    },
}

impl DialogState {
    /// Returns true if the create dialog is open.
    pub fn is_create(&self) -> bool {
        matches!(self, DialogState::Create { .. })
    }

    /// Returns true if the confirm dialog is open.
    pub fn is_confirm(&self) -> bool {
        matches!(self, DialogState::Confirm { .. })
    }

    /// Returns true if the add project dialog is open.
    pub fn is_add_project(&self) -> bool {
        matches!(self, DialogState::AddProject { .. })
    }

    /// Open the create dialog with default form state.
    pub fn open_create() -> Self {
        DialogState::Create {
            form: CreateFormState::default(),
            error: None,
        }
    }

    /// Open the confirm dialog for destroying a branch.
    pub fn open_confirm(branch: String, safety_info: Option<DestroySafetyInfo>) -> Self {
        DialogState::Confirm {
            branch,
            safety_info,
            error: None,
        }
    }

    /// Open the add project dialog with default form state.
    pub fn open_add_project() -> Self {
        DialogState::AddProject {
            form: AddProjectFormState::default(),
            error: None,
        }
    }
}

/// Error from a kild operation, with the branch name for context.
#[derive(Clone, Debug)]
pub struct OperationError {
    pub branch: String,
    pub message: String,
}

// =============================================================================
// Operation Errors
// =============================================================================

/// Unified error tracking for kild operations.
///
/// Consolidates per-branch errors (open, stop, editor, focus) and bulk operation
/// errors into a single struct with a consistent API.
#[derive(Clone, Debug, Default)]
pub struct OperationErrors {
    /// Per-branch errors (keyed by branch name).
    by_branch: std::collections::HashMap<String, OperationError>,
    /// Bulk operation errors (e.g., "open all" failures).
    bulk: Vec<OperationError>,
}

impl OperationErrors {
    /// Create a new empty error collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an error for a specific branch (replaces any existing error).
    pub fn set(&mut self, branch: &str, error: OperationError) {
        self.by_branch.insert(branch.to_string(), error);
    }

    /// Get the error for a specific branch, if any.
    pub fn get(&self, branch: &str) -> Option<&OperationError> {
        self.by_branch.get(branch)
    }

    /// Clear the error for a specific branch.
    pub fn clear(&mut self, branch: &str) {
        self.by_branch.remove(branch);
    }

    /// Set bulk errors (replaces existing).
    pub fn set_bulk(&mut self, errors: Vec<OperationError>) {
        self.bulk = errors;
    }

    /// Get bulk operation errors.
    pub fn bulk_errors(&self) -> &[OperationError] {
        &self.bulk
    }

    /// Check if there are any bulk errors.
    pub fn has_bulk_errors(&self) -> bool {
        !self.bulk.is_empty()
    }

    /// Clear all bulk operation errors.
    pub fn clear_bulk(&mut self) {
        self.bulk.clear();
    }
}

// =============================================================================
// Selection State
// =============================================================================

/// Encapsulates kild selection state.
///
/// Provides a clean API for selecting/deselecting kilds and checking
/// if a selection is still valid after list updates.
#[derive(Clone, Debug, Default)]
pub struct SelectionState {
    /// ID of the currently selected kild, or None if nothing selected.
    selected_id: Option<String>,
}

impl SelectionState {
    /// Create a new empty selection state.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Select a kild by ID.
    pub fn select(&mut self, id: String) {
        self.selected_id = Some(id);
    }

    /// Clear the selection.
    pub fn clear(&mut self) {
        self.selected_id = None;
    }

    /// Get the selected kild ID, if any.
    pub fn id(&self) -> Option<&str> {
        self.selected_id.as_deref()
    }

    /// Check if a kild is selected.
    pub fn has_selection(&self) -> bool {
        self.selected_id.is_some()
    }
}

// =============================================================================
// Session Store
// =============================================================================

/// Encapsulates session display data with refresh tracking.
///
/// Provides a clean API for managing kild displays, filtering by project,
/// and tracking refresh timestamps. Encapsulates:
/// - `displays`: The list of SessionInfo items
/// - `load_error`: Error from last refresh attempt
/// - `last_refresh`: Timestamp of last successful refresh
pub struct SessionStore {
    /// List of kild displays (private to enforce invariants).
    displays: Vec<SessionInfo>,
    /// Error from last refresh attempt, if any.
    load_error: Option<String>,
    /// Timestamp of last successful status refresh.
    last_refresh: std::time::Instant,
}

impl SessionStore {
    /// Create a new session store by loading sessions from disk.
    pub fn new() -> Self {
        let (displays, load_error) = crate::actions::refresh_sessions();
        Self {
            displays,
            load_error,
            last_refresh: std::time::Instant::now(),
        }
    }

    /// Create a session store with provided data (for testing).
    #[cfg(test)]
    pub fn from_data(displays: Vec<SessionInfo>, load_error: Option<String>) -> Self {
        Self {
            displays,
            load_error,
            last_refresh: std::time::Instant::now(),
        }
    }

    /// Set displays directly (for testing).
    #[cfg(test)]
    pub fn set_displays(&mut self, displays: Vec<SessionInfo>) {
        self.displays = displays;
    }

    /// Get mutable access to displays (for testing status updates).
    #[cfg(test)]
    pub fn displays_mut(&mut self) -> &mut Vec<SessionInfo> {
        &mut self.displays
    }

    /// Refresh sessions from disk.
    pub fn refresh(&mut self) {
        let (displays, load_error) = crate::actions::refresh_sessions();
        self.displays = displays;
        self.load_error = load_error;
        self.last_refresh = std::time::Instant::now();
    }

    /// Update only the process status of existing kilds without reloading from disk.
    ///
    /// This is faster than `refresh()` for status polling because it:
    /// - Doesn't reload session files from disk (unless count mismatch detected)
    /// - Only checks if tracked processes are still running
    /// - Preserves the existing kild list structure
    ///
    /// If the session count on disk differs from the in-memory count (indicating
    /// external create/destroy operations), triggers a full refresh instead.
    ///
    /// Note: This does NOT update git status or diff stats. Use `refresh()`
    /// for a full refresh that includes git information.
    pub fn update_statuses_only(&mut self) {
        // Check if session count changed (external create/destroy).
        let disk_count = kild_core::sessions::store::count_session_files();

        if let Some(count) = disk_count {
            if count != self.displays.len() {
                tracing::info!(
                    event = "ui.auto_refresh.session_count_mismatch",
                    disk_count = count,
                    memory_count = self.displays.len(),
                    action = "triggering full refresh"
                );
                self.refresh();
                return;
            }
        } else {
            tracing::debug!(
                event = "ui.auto_refresh.count_check_skipped",
                reason = "cannot read sessions directory"
            );
        }

        // No count change (or count unavailable) - just update process statuses
        for kild_display in &mut self.displays {
            kild_display.process_status =
                kild_core::sessions::info::determine_process_status(&kild_display.session);
        }
        self.last_refresh = std::time::Instant::now();
    }

    /// Get all displays.
    pub fn displays(&self) -> &[SessionInfo] {
        &self.displays
    }

    /// Get displays filtered by project ID.
    ///
    /// Returns all displays where `session.project_id` matches the given ID.
    /// If `project_id` is `None`, returns all displays (unfiltered).
    pub fn filtered_by_project(&self, project_id: Option<&str>) -> Vec<&SessionInfo> {
        match project_id {
            Some(id) => self
                .displays
                .iter()
                .filter(|d| d.session.project_id == id)
                .collect(),
            None => self.displays.iter().collect(),
        }
    }

    /// Get the load error from the last refresh attempt, if any.
    pub fn load_error(&self) -> Option<&str> {
        self.load_error.as_deref()
    }

    /// Get the timestamp of the last successful refresh.
    #[allow(dead_code)]
    pub fn last_refresh(&self) -> std::time::Instant {
        self.last_refresh
    }

    /// Count kilds with Stopped status.
    pub fn stopped_count(&self) -> usize {
        self.displays
            .iter()
            .filter(|d| d.process_status == ProcessStatus::Stopped)
            .count()
    }

    /// Count kilds with Running status.
    pub fn running_count(&self) -> usize {
        self.displays
            .iter()
            .filter(|d| d.process_status == ProcessStatus::Running)
            .count()
    }

    /// Count kilds for a specific project (by project ID).
    pub fn kild_count_for_project(&self, project_id: &str) -> usize {
        self.displays
            .iter()
            .filter(|d| d.session.project_id == project_id)
            .count()
    }

    /// Count total kilds across all projects.
    pub fn total_count(&self) -> usize {
        self.displays.len()
    }

    /// Check if there are no displays.
    pub fn is_empty(&self) -> bool {
        self.displays.is_empty()
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Which field is focused in the create dialog.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CreateDialogField {
    #[default]
    BranchName,
    Agent,
    Note,
}

/// Form state for creating a new kild.
#[derive(Clone, Debug)]
pub struct CreateFormState {
    pub branch_name: String,
    pub selected_agent_index: usize,
    pub note: String,
    pub focused_field: CreateDialogField,
}

impl CreateFormState {
    /// Get the currently selected agent name.
    ///
    /// Derives the agent name from the index, falling back to the default
    /// agent if the index is out of bounds (with warning logged).
    pub fn selected_agent(&self) -> String {
        let agents = kild_core::agents::valid_agent_names();
        agents
            .get(self.selected_agent_index)
            .copied()
            .unwrap_or_else(|| {
                tracing::warn!(
                    event = "ui.create_form.agent_index_out_of_bounds",
                    index = self.selected_agent_index,
                    agent_count = agents.len(),
                    "Selected agent index out of bounds, using default"
                );
                kild_core::agents::default_agent_name()
            })
            .to_string()
    }
}

impl Default for CreateFormState {
    fn default() -> Self {
        let agents = kild_core::agents::valid_agent_names();
        let default_agent = kild_core::agents::default_agent_name();

        if agents.is_empty() {
            tracing::error!(
                event = "ui.create_form.no_agents_available",
                "Agent list is empty - using hardcoded fallback"
            );
            return Self {
                branch_name: String::new(),
                selected_agent_index: 0,
                note: String::new(),
                focused_field: CreateDialogField::default(),
            };
        }

        let index = agents
            .iter()
            .position(|&a| a == default_agent)
            .unwrap_or_else(|| {
                tracing::warn!(
                    event = "ui.create_form.default_agent_not_found",
                    default = default_agent,
                    selected = agents[0],
                    "Default agent not in list, using first available"
                );
                0
            });

        Self {
            branch_name: String::new(),
            selected_agent_index: index,
            note: String::new(),
            focused_field: CreateDialogField::default(),
        }
    }
}

/// Which field is focused in the add project dialog.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum AddProjectDialogField {
    #[default]
    Path,
    Name,
}

/// Form state for adding a new project.
#[derive(Clone, Debug, Default)]
pub struct AddProjectFormState {
    pub path: String,
    pub name: String,
    pub focused_field: AddProjectDialogField,
}

/// Main application state.
///
/// All fields are private - access state through the facade methods below.
/// This ensures all state mutations go through controlled methods that
/// maintain invariants and provide a consistent API.
pub struct AppState {
    /// Session display data with refresh tracking.
    sessions: SessionStore,

    /// Current dialog state (mutually exclusive - only one dialog can be open).
    dialog: DialogState,

    /// Operation errors (per-branch and bulk).
    errors: OperationErrors,

    /// Kild selection state (for detail panel).
    selection: SelectionState,

    /// Project management with enforced invariants.
    projects: ProjectManager,

    /// Startup errors that should be shown to the user (migration failures, load errors).
    startup_errors: Vec<String>,
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

    /// Clear all bulk operation errors.
    pub fn clear_bulk_errors(&mut self) {
        self.errors.clear_bulk();
    }

    /// Get the project ID for the active project.
    pub fn active_project_id(&self) -> Option<String> {
        self.projects
            .active_path()
            .map(kild_core::projects::generate_project_id)
    }

    /// Get displays filtered by active project.
    ///
    /// Filters kilds where `session.project_id` matches the derived ID of the active project path.
    /// Uses path-based hashing that matches kild-core's `generate_project_id`.
    /// If no active project is set, returns all displays (unfiltered).
    pub fn filtered_displays(&self) -> Vec<&SessionInfo> {
        self.sessions
            .filtered_by_project(self.active_project_id().as_deref())
    }

    /// Count kilds with Stopped status.
    pub fn stopped_count(&self) -> usize {
        self.sessions.stopped_count()
    }

    /// Count kilds with Running status.
    pub fn running_count(&self) -> usize {
        self.sessions.running_count()
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
    pub fn selected_kild(&self) -> Option<&SessionInfo> {
        let id = self.selection.id()?;

        match self.sessions.displays().iter().find(|d| d.session.id == id) {
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
    #[allow(dead_code)]
    pub fn get_error(&self, branch: &str) -> Option<&OperationError> {
        self.errors.get(branch)
    }

    /// Set bulk errors (replaces existing).
    pub fn set_bulk_errors(&mut self, errors: Vec<OperationError>) {
        self.errors.set_bulk(errors);
    }

    /// Get bulk operation errors.
    pub fn bulk_errors(&self) -> &[OperationError] {
        self.errors.bulk_errors()
    }

    /// Check if there are any bulk errors.
    pub fn has_bulk_errors(&self) -> bool {
        self.errors.has_bulk_errors()
    }

    /// Clone the operation errors (for capturing in closures).
    pub fn errors_clone(&self) -> OperationErrors {
        self.errors.clone()
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

    /// Select a project by path.
    pub fn select_project(&mut self, path: &std::path::Path) -> Result<(), ProjectError> {
        self.projects.select(path)
    }

    /// Select "all projects" view (clears active project selection).
    pub fn select_all_projects(&mut self) {
        self.projects.select_all();
    }

    /// Add a project to the list.
    pub fn add_project(&mut self, project: Project) -> Result<(), ProjectError> {
        self.projects.add(project)
    }

    /// Remove a project by path.
    pub fn remove_project(&mut self, path: &std::path::Path) -> Result<Project, ProjectError> {
        self.projects.remove(path)
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
    pub fn projects_is_empty(&self) -> bool {
        self.projects.is_empty()
    }

    // =========================================================================
    // Session facade methods
    // =========================================================================

    /// Get all session displays.
    pub fn displays(&self) -> &[SessionInfo] {
        self.sessions.displays()
    }

    /// Get the load error from the last refresh attempt, if any.
    pub fn load_error(&self) -> Option<&str> {
        self.sessions.load_error()
    }

    /// Check if there are no session displays.
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
        }
    }

    /// Create an AppState for testing with provided displays.
    #[cfg(test)]
    pub fn test_with_displays(displays: Vec<SessionInfo>) -> Self {
        Self {
            sessions: SessionStore::from_data(displays, None),
            dialog: DialogState::None,
            errors: OperationErrors::new(),
            selection: SelectionState::default(),
            projects: ProjectManager::new(),
            startup_errors: Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use kild_core::{GitStatus, Session};
    use std::path::PathBuf;

    #[test]
    fn test_close_dialog_clears_confirm_state() {
        let mut state = AppState::test_new();
        state.set_dialog(DialogState::Confirm {
            branch: "feature-branch".to_string(),
            safety_info: None,
            error: Some("Some error".to_string()),
        });

        state.close_dialog();

        assert!(matches!(state.dialog(), DialogState::None));
    }

    #[test]
    fn test_dialog_state_mutual_exclusion() {
        // DialogState enum enforces mutual exclusion at compile-time.
        // This test documents the invariant.
        let create = DialogState::open_create();
        assert!(create.is_create());
        assert!(!create.is_confirm());
        assert!(!create.is_add_project());

        let confirm = DialogState::open_confirm("test-branch".to_string(), None);
        assert!(!confirm.is_create());
        assert!(confirm.is_confirm());
        assert!(!confirm.is_add_project());

        let add_project = DialogState::open_add_project();
        assert!(!add_project.is_create());
        assert!(!add_project.is_confirm());
        assert!(add_project.is_add_project());

        let none = DialogState::None;
        assert!(!none.is_create());
        assert!(!none.is_confirm());
        assert!(!none.is_add_project());
    }

    #[test]
    fn test_set_dialog_error_sets_error_on_create() {
        let mut state = AppState::test_new();
        state.set_dialog(DialogState::open_create());

        state.set_dialog_error("Test error".to_string());

        if let DialogState::Create { error, .. } = state.dialog() {
            assert_eq!(error.as_deref(), Some("Test error"));
        } else {
            panic!("Expected Create dialog");
        }
    }

    #[test]
    fn test_set_dialog_error_sets_error_on_confirm() {
        let mut state = AppState::test_new();
        state.open_confirm_dialog("test-branch".to_string());

        state.set_dialog_error("Destroy failed".to_string());

        if let DialogState::Confirm { error, .. } = state.dialog() {
            assert_eq!(error.as_deref(), Some("Destroy failed"));
        } else {
            panic!("Expected Confirm dialog");
        }
    }

    #[test]
    fn test_operation_errors_set_and_get() {
        let mut errors = OperationErrors::new();

        errors.set(
            "branch-1",
            OperationError {
                branch: "branch-1".to_string(),
                message: "error 1".to_string(),
            },
        );

        assert!(errors.get("branch-1").is_some());
        assert_eq!(errors.get("branch-1").unwrap().message, "error 1");
        assert!(errors.get("branch-2").is_none());
    }

    #[test]
    fn test_operation_errors_clear() {
        let mut errors = OperationErrors::new();

        errors.set(
            "branch-1",
            OperationError {
                branch: "branch-1".to_string(),
                message: "error 1".to_string(),
            },
        );
        errors.clear("branch-1");

        assert!(errors.get("branch-1").is_none());
    }

    #[test]
    fn test_operation_errors_bulk() {
        let mut errors = OperationErrors::new();

        errors.set_bulk(vec![
            OperationError {
                branch: "branch-1".to_string(),
                message: "error 1".to_string(),
            },
            OperationError {
                branch: "branch-2".to_string(),
                message: "error 2".to_string(),
            },
        ]);

        assert!(errors.has_bulk_errors());
        assert_eq!(errors.bulk_errors().len(), 2);

        errors.clear_bulk();
        assert!(!errors.has_bulk_errors());
    }

    #[test]
    fn test_clear_error() {
        let mut state = AppState::test_new();
        state.set_error(
            "branch",
            OperationError {
                branch: "branch".to_string(),
                message: "error".to_string(),
            },
        );

        state.clear_error("branch");

        assert!(state.get_error("branch").is_none());
    }

    #[test]
    fn test_process_status_from_session_no_pid() {
        use kild_core::sessions::types::SessionStatus;
        use std::path::PathBuf;

        let session = Session {
            id: "test-id".to_string(),
            branch: "test-branch".to_string(),
            worktree_path: PathBuf::from("/tmp/nonexistent-test-path"),
            agent: "claude".to_string(),
            project_id: "test-project".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: String::new(),
            last_activity: None,
            note: None,
        };

        let display = SessionInfo::from_session(session);
        assert_eq!(display.process_status, ProcessStatus::Stopped);
        // Non-existent path should result in Unknown git status
        assert_eq!(display.git_status, GitStatus::Unknown);
    }

    #[test]
    fn test_process_status_from_session_with_window_id_no_pid() {
        use kild_core::sessions::types::SessionStatus;
        use kild_core::terminal::types::TerminalType;
        use std::path::PathBuf;

        // Session with terminal_window_id but no process_id (Ghostty case)
        let session = Session {
            id: "test-id".to_string(),
            branch: "test-branch".to_string(),
            worktree_path: PathBuf::from("/tmp/nonexistent-test-path"),
            agent: "claude".to_string(),
            project_id: "test-project".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: Some(TerminalType::Ghostty),
            terminal_window_id: Some("kild-test-window".to_string()),
            command: String::new(),
            last_activity: None,
            note: None,
        };

        let display = SessionInfo::from_session(session);
        // With window detection fallback, should attempt to check window
        // In test environment without Ghostty running, will fall back to Stopped
        assert!(
            display.process_status == ProcessStatus::Stopped
                || display.process_status == ProcessStatus::Running,
            "Should have valid status from window detection fallback"
        );
    }

    #[test]
    fn test_kild_display_from_session_populates_diff_stats_when_dirty() {
        use kild_core::sessions::types::SessionStatus;
        use std::process::Command;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Initialize git repo with a commit
        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .output()
            .unwrap();
        std::fs::write(path.join("test.txt"), "line1\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(path)
            .output()
            .unwrap();

        // Make it dirty (unstaged changes)
        std::fs::write(path.join("test.txt"), "line1\nline2\nline3\n").unwrap();

        let session = Session {
            id: "test".to_string(),
            branch: "test".to_string(),
            worktree_path: path.to_path_buf(),
            agent: "claude".to_string(),
            project_id: "test".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: String::new(),
            last_activity: None,
            note: None,
        };

        let display = SessionInfo::from_session(session);

        assert_eq!(display.git_status, GitStatus::Dirty);
        assert!(
            display.diff_stats.is_some(),
            "diff_stats should be populated for dirty repo"
        );
        let stats = display.diff_stats.unwrap();
        assert_eq!(stats.insertions, 2, "Should have 2 insertions");
        assert_eq!(stats.files_changed, 1);
        assert!(stats.has_changes());
    }

    // --- CreateDialogField tests ---

    #[test]
    fn test_create_dialog_field_default_is_branch_name() {
        let field = CreateDialogField::default();
        assert_eq!(field, CreateDialogField::BranchName);
    }

    #[test]
    fn test_tab_navigation_cycles_correctly() {
        // Simulates the tab navigation state machine:
        // BranchName -> Agent -> Note -> BranchName
        let mut field = CreateDialogField::BranchName;

        // Tab 1: BranchName -> Agent
        field = match field {
            CreateDialogField::BranchName => CreateDialogField::Agent,
            CreateDialogField::Agent => CreateDialogField::Note,
            CreateDialogField::Note => CreateDialogField::BranchName,
        };
        assert_eq!(field, CreateDialogField::Agent);

        // Tab 2: Agent -> Note
        field = match field {
            CreateDialogField::BranchName => CreateDialogField::Agent,
            CreateDialogField::Agent => CreateDialogField::Note,
            CreateDialogField::Note => CreateDialogField::BranchName,
        };
        assert_eq!(field, CreateDialogField::Note);

        // Tab 3: Note -> BranchName (cycle complete)
        field = match field {
            CreateDialogField::BranchName => CreateDialogField::Agent,
            CreateDialogField::Agent => CreateDialogField::Note,
            CreateDialogField::Note => CreateDialogField::BranchName,
        };
        assert_eq!(field, CreateDialogField::BranchName);
    }

    // --- CreateFormState tests ---

    #[test]
    fn test_create_form_state_default_focused_field() {
        let form = CreateFormState::default();
        assert_eq!(form.focused_field, CreateDialogField::BranchName);
    }

    #[test]
    fn test_create_form_state_selected_agent_derives_from_index() {
        let mut form = CreateFormState::default();
        let agents = kild_core::agents::valid_agent_names();

        if agents.len() > 1 {
            // Change index and verify selected_agent() returns the correct agent
            form.selected_agent_index = 1;
            assert_eq!(form.selected_agent(), agents[1]);
        }
    }

    #[test]
    fn test_create_form_state_selected_agent_fallback_on_invalid_index() {
        let form = CreateFormState {
            selected_agent_index: 999,
            ..Default::default()
        };

        // Should fall back to default agent
        let expected = kild_core::agents::default_agent_name();
        assert_eq!(form.selected_agent(), expected);
    }

    // --- Note field input tests ---

    #[test]
    fn test_note_allows_spaces() {
        let mut note = String::new();
        let c = ' ';

        // Note field accepts spaces directly (unlike branch name which converts to hyphen)
        if !c.is_control() {
            note.push(c);
        }

        assert_eq!(note, " ");
    }

    #[test]
    fn test_note_rejects_control_characters() {
        let mut note = String::new();

        // Control characters should be rejected
        for c in ['\n', '\r', '\t', '\x00', '\x1b'] {
            if !c.is_control() {
                note.push(c);
            }
        }

        assert!(
            note.is_empty(),
            "Control characters should not be added to note"
        );
    }

    #[test]
    fn test_note_accepts_unicode() {
        let mut note = String::new();

        // Unicode characters should be accepted
        for c in ['日', '本', '語', '🚀', 'é', 'ñ'] {
            if !c.is_control() {
                note.push(c);
            }
        }

        assert_eq!(note, "日本語🚀éñ");
    }

    #[test]
    fn test_branch_name_validation() {
        let mut branch = String::new();

        // Valid characters for branch names
        for c in ['a', 'Z', '0', '-', '_', '/'] {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '/' {
                branch.push(c);
            }
        }
        assert_eq!(branch, "aZ0-_/");

        // Invalid characters should be rejected
        let mut branch2 = String::new();
        for c in [' ', '@', '#', '$', '%', '!'] {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '/' {
                branch2.push(c);
            }
        }
        assert!(branch2.is_empty(), "Invalid characters should be rejected");
    }

    // --- Note truncation tests ---

    #[test]
    fn test_note_truncation_at_boundary() {
        let note_25_chars = "1234567890123456789012345";
        let note_26_chars = "12345678901234567890123456";

        // 25 chars should not be truncated
        let truncated_25 = if note_25_chars.chars().count() > 25 {
            format!("{}...", note_25_chars.chars().take(25).collect::<String>())
        } else {
            note_25_chars.to_string()
        };
        assert_eq!(truncated_25, note_25_chars);

        // 26 chars should be truncated to "25chars..."
        let truncated_26 = if note_26_chars.chars().count() > 25 {
            format!("{}...", note_26_chars.chars().take(25).collect::<String>())
        } else {
            note_26_chars.to_string()
        };
        assert_eq!(truncated_26, "1234567890123456789012345...");
    }

    #[test]
    fn test_note_truncation_unicode() {
        // Unicode characters should be counted as single characters, not bytes
        let unicode_note = "日本語テスト文字列はここにあります長い"; // 18 chars

        let truncated = if unicode_note.chars().count() > 25 {
            format!("{}...", unicode_note.chars().take(25).collect::<String>())
        } else {
            unicode_note.to_string()
        };

        // Should not be truncated (only 18 chars)
        assert_eq!(truncated, unicode_note);
    }

    #[test]
    fn test_note_trimming_whitespace_only() {
        let note_whitespace = "   \t  \n  ";

        // Whitespace-only note should become None
        let trimmed = if note_whitespace.trim().is_empty() {
            None
        } else {
            Some(note_whitespace.trim().to_string())
        };

        assert!(trimmed.is_none(), "Whitespace-only note should become None");
    }

    #[test]
    fn test_note_trimming_preserves_content() {
        let note_with_spaces = "  hello world  ";

        let trimmed = if note_with_spaces.trim().is_empty() {
            None
        } else {
            Some(note_with_spaces.trim().to_string())
        };

        assert_eq!(trimmed, Some("hello world".to_string()));
    }

    #[test]
    fn test_update_statuses_only_updates_last_refresh() {
        let initial_time = std::time::Instant::now();
        let mut state = AppState::test_new();

        // Small delay to ensure time difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        state.update_statuses_only();

        // last_refresh should be updated to a later time
        assert!(state.sessions.last_refresh() > initial_time);
    }

    #[test]
    fn test_update_statuses_only_updates_process_status() {
        use kild_core::sessions::types::SessionStatus;
        use std::path::PathBuf;

        // Create a session with a PID that doesn't exist (should become Stopped)
        let session_with_dead_pid = Session {
            id: "test-dead".to_string(),
            branch: "dead-branch".to_string(),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            project_id: "test-project".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: Some(999999), // Non-existent PID
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: String::new(),
            last_activity: None,
            note: None,
        };

        // Create a session with our own PID (should be Running)
        let session_with_live_pid = Session {
            id: "test-live".to_string(),
            branch: "live-branch".to_string(),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            project_id: "test-project".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: Some(std::process::id()), // Current process PID
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: String::new(),
            last_activity: None,
            note: None,
        };

        // Create a session with no PID (should remain Stopped)
        let session_no_pid = Session {
            id: "test-no-pid".to_string(),
            branch: "no-pid-branch".to_string(),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            project_id: "test-project".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: String::new(),
            last_activity: None,
            note: None,
        };

        let mut state = AppState::test_new();
        state.sessions.set_displays(vec![
            SessionInfo {
                session: session_with_dead_pid,
                process_status: ProcessStatus::Running, // Start as Running (incorrect)
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            SessionInfo {
                session: session_with_live_pid,
                process_status: ProcessStatus::Stopped, // Start as Stopped (incorrect)
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            SessionInfo {
                session: session_no_pid,
                process_status: ProcessStatus::Stopped, // Start as Stopped (correct)
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
        ]);

        let original_len = state.sessions.displays().len();
        state.update_statuses_only();

        // Note: update_statuses_only() may trigger a full refresh if the session count
        // on disk differs from the in-memory count (see issue #103 fix). In that case,
        // the displays will be replaced with whatever is on disk.
        //
        // If the display count changed, a refresh was triggered and we can't test
        // the status update logic directly. Skip the assertions in that case.
        if state.sessions.displays().len() != original_len {
            // Refresh was triggered due to count mismatch - this is expected behavior
            // when running tests in an environment with actual session files.
            return;
        }

        // Non-existent PID should be marked Stopped
        assert_eq!(
            state.sessions.displays()[0].process_status,
            ProcessStatus::Stopped,
            "Non-existent PID should be marked Stopped"
        );

        // Current process PID should be marked Running
        assert_eq!(
            state.sessions.displays()[1].process_status,
            ProcessStatus::Running,
            "Current process PID should be marked Running"
        );

        // No PID should remain Stopped (not checked, so unchanged)
        assert_eq!(
            state.sessions.displays()[2].process_status,
            ProcessStatus::Stopped,
            "Session with no PID should remain Stopped"
        );
    }

    #[test]
    fn test_stopped_count_empty() {
        let state = AppState::test_new();

        assert_eq!(state.stopped_count(), 0);
        assert_eq!(state.running_count(), 0);
    }

    #[test]
    fn test_stopped_and_running_counts() {
        use kild_core::sessions::types::SessionStatus;
        use std::path::PathBuf;

        let make_session = |id: &str, branch: &str| Session {
            id: id.to_string(),
            branch: branch.to_string(),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            project_id: "test-project".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: String::new(),
            last_activity: None,
            note: None,
        };

        let mut state = AppState::test_new();
        state.sessions.set_displays(vec![
            SessionInfo {
                session: make_session("1", "branch-1"),
                process_status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            SessionInfo {
                session: make_session("2", "branch-2"),
                process_status: ProcessStatus::Running,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            SessionInfo {
                session: make_session("3", "branch-3"),
                process_status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            SessionInfo {
                session: make_session("4", "branch-4"),
                process_status: ProcessStatus::Running,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            SessionInfo {
                session: make_session("5", "branch-5"),
                process_status: ProcessStatus::Unknown,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
        ]);

        assert_eq!(state.stopped_count(), 2, "Should count 2 stopped kilds");
        assert_eq!(state.running_count(), 2, "Should count 2 running kilds");
    }

    // --- Project-related tests ---

    #[test]
    fn test_close_dialog_clears_add_project_state() {
        let mut state = AppState::test_new();
        state.dialog = DialogState::AddProject {
            form: AddProjectFormState {
                path: "/some/path".to_string(),
                name: "test".to_string(),
                focused_field: AddProjectDialogField::Path,
            },
            error: Some("Error".to_string()),
        };

        state.close_dialog();

        assert!(matches!(state.dialog, DialogState::None));
    }

    #[test]
    fn test_active_project_id() {
        let mut state = AppState::test_new();

        // No active project
        assert!(state.active_project_id().is_none());

        // With active project - should return a hash, not directory name
        let project = kild_core::projects::types::test_helpers::make_test_project(
            PathBuf::from("/Users/test/Projects/my-project"),
            "My Project".to_string(),
        );
        state.projects.add(project).unwrap();
        // First project is automatically selected
        let project_id = state.active_project_id();
        assert!(project_id.is_some());
        // Should be a hex hash, not the directory name
        let id = project_id.unwrap();
        assert!(!id.is_empty());
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_filtered_displays_no_active_project() {
        use kild_core::sessions::types::SessionStatus;

        let make_session = |id: &str, project_id: &str| Session {
            id: id.to_string(),
            branch: format!("branch-{}", id),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            project_id: project_id.to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: String::new(),
            last_activity: None,
            note: None,
        };

        let mut state = AppState::test_new();
        state.sessions.set_displays(vec![
            SessionInfo {
                session: make_session("1", "project-a"),
                process_status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            SessionInfo {
                session: make_session("2", "project-b"),
                process_status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
        ]);

        // No active project - should return all
        let filtered = state.filtered_displays();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filtered_displays_with_active_project() {
        use kild_core::sessions::types::SessionStatus;

        // Use the actual hash for the project path
        let project_path = PathBuf::from("/Users/test/Projects/project-a");
        let project_id_a = kild_core::projects::generate_project_id(&project_path);
        let project_id_b =
            kild_core::projects::generate_project_id(&PathBuf::from("/other/project"));

        let make_session = |id: &str, project_id: &str| Session {
            id: id.to_string(),
            branch: format!("branch-{}", id),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            project_id: project_id.to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: String::new(),
            last_activity: None,
            note: None,
        };

        let mut state = AppState::test_new();
        state.sessions.set_displays(vec![
            SessionInfo {
                session: make_session("1", &project_id_a),
                process_status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            SessionInfo {
                session: make_session("2", &project_id_b),
                process_status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            SessionInfo {
                session: make_session("3", &project_id_a),
                process_status: ProcessStatus::Running,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
        ]);

        // Active project set - should filter
        // Add project and select it
        let project = kild_core::projects::types::test_helpers::make_test_project(
            project_path.clone(),
            "Project A".to_string(),
        );
        state.projects.add(project).unwrap();
        // First project is auto-selected, so this should filter
        let filtered = state.filtered_displays();
        assert_eq!(filtered.len(), 2);
        assert!(
            filtered
                .iter()
                .all(|d| d.session.project_id == project_id_a)
        );
    }

    #[test]
    fn test_filtered_displays_returns_empty_when_no_matching_project() {
        use kild_core::sessions::types::SessionStatus;

        let make_session = |id: &str, project_id: &str| Session {
            id: id.to_string(),
            branch: format!("branch-{}", id),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            project_id: project_id.to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: String::new(),
            last_activity: None,
            note: None,
        };

        let mut state = AppState::test_new();
        state.sessions.set_displays(vec![SessionInfo {
            session: make_session("1", "other-project-hash"),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            diff_stats: None,
        }]);

        // Active project set to a different path - should return empty
        let project = kild_core::projects::types::test_helpers::make_test_project(
            PathBuf::from("/different/project/path"),
            "Different Project".to_string(),
        );
        state.projects.add(project).unwrap();
        let filtered = state.filtered_displays();
        assert!(
            filtered.is_empty(),
            "Should return empty when no kilds match active project"
        );
    }

    #[test]
    fn test_selected_kild_returns_none_when_kild_removed_after_refresh() {
        use kild_core::sessions::types::SessionStatus;

        let make_session = |id: &str| Session {
            id: id.to_string(),
            branch: format!("branch-{}", id),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            project_id: "test-project".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: String::new(),
            last_activity: None,
            note: None,
        };

        let mut state = AppState::test_new();
        state.sessions.set_displays(vec![SessionInfo {
            session: make_session("test-id"),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            diff_stats: None,
        }]);
        state.selection.select("test-id".to_string());

        // Verify selection works initially
        assert!(state.selected_kild().is_some());

        // Simulate refresh that removes the kild (e.g., destroyed via CLI)
        state.sessions.set_displays(vec![]);

        // Selection ID still set, but selected_kild() should return None gracefully
        assert!(state.selection.has_selection());
        assert!(
            state.selected_kild().is_none(),
            "Should return None when selected kild no longer exists"
        );
    }

    #[test]
    fn test_selected_kild_persists_after_refresh_when_kild_still_exists() {
        use kild_core::sessions::types::SessionStatus;

        let make_session = |id: &str| Session {
            id: id.to_string(),
            branch: format!("branch-{}", id),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            project_id: "test-project".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: String::new(),
            last_activity: None,
            note: None,
        };

        let mut state = AppState::test_new();
        state.sessions.set_displays(vec![SessionInfo {
            session: make_session("test-id"),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            diff_stats: None,
        }]);
        state.selection.select("test-id".to_string());

        // Verify initial selection
        assert!(state.selected_kild().is_some());

        // Simulate refresh that keeps the same kild (new display list with same ID)
        state.sessions.set_displays(vec![SessionInfo {
            session: make_session("test-id"),
            process_status: ProcessStatus::Running, // Status may change
            git_status: GitStatus::Dirty,           // Git status may change
            diff_stats: None,
        }]);

        // Selection should persist
        let selected = state.selected_kild();
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().session.id, "test-id");
    }

    #[test]
    fn test_clear_selection_clears_selection() {
        let mut state = AppState::test_new();
        state.selection.select("test-id".to_string());

        assert!(state.selection.has_selection());

        state.clear_selection();

        assert!(
            !state.selection.has_selection(),
            "clear_selection should clear the selection"
        );
    }

    #[test]
    fn test_destroy_should_clear_selection_when_selected_kild_destroyed() {
        use kild_core::sessions::types::SessionStatus;

        let make_session = |id: &str, branch: &str| Session {
            id: id.to_string(),
            branch: branch.to_string(),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            project_id: "test-project".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: String::new(),
            last_activity: None,
            note: None,
        };

        let mut state = AppState::test_new();
        state.sessions.set_displays(vec![
            SessionInfo {
                session: make_session("id-1", "branch-1"),
                process_status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            SessionInfo {
                session: make_session("id-2", "branch-2"),
                process_status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
        ]);
        state.selection.select("id-1".to_string());

        // Simulate destroy of selected kild - the destroy handler logic:
        // if selected_kild().session.branch == destroyed_branch { clear_selection() }
        let destroyed_branch = "branch-1";
        if state
            .selected_kild()
            .is_some_and(|s| s.session.branch == destroyed_branch)
        {
            state.clear_selection();
        }
        state
            .sessions
            .displays_mut()
            .retain(|d| d.session.branch != destroyed_branch);

        // Selection should be cleared
        assert!(
            !state.selection.has_selection(),
            "Selection should be cleared when selected kild is destroyed"
        );
    }

    #[test]
    fn test_destroy_preserves_selection_when_different_kild_destroyed() {
        use kild_core::sessions::types::SessionStatus;

        let make_session = |id: &str, branch: &str| Session {
            id: id.to_string(),
            branch: branch.to_string(),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            project_id: "test-project".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: String::new(),
            last_activity: None,
            note: None,
        };

        let mut state = AppState::test_new();
        state.sessions.set_displays(vec![
            SessionInfo {
                session: make_session("id-1", "branch-1"),
                process_status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            SessionInfo {
                session: make_session("id-2", "branch-2"),
                process_status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
        ]);
        state.selection.select("id-1".to_string());

        // Destroy branch-2 (not selected)
        let destroyed_branch = "branch-2";
        if state
            .selected_kild()
            .is_some_and(|s| s.session.branch == destroyed_branch)
        {
            state.clear_selection();
        }
        state
            .sessions
            .displays_mut()
            .retain(|d| d.session.branch != destroyed_branch);

        // Selection of branch-1 should persist
        assert_eq!(
            state.selection.id(),
            Some("id-1"),
            "Selection should persist when a different kild is destroyed"
        );
        assert!(state.selected_kild().is_some());
    }
}
