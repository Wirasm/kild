use kild_core::SessionInfo;
use kild_core::projects::{Project, ProjectManager};

use super::dialog::DialogState;
use super::errors::{OperationError, OperationErrors};
use super::loading::LoadingState;
use super::selection::SelectionState;
use super::sessions::SessionStore;

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

    /// In-progress operation tracking (prevents double-dispatch).
    loading: LoadingState,
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
    fn clear_selection_if_matches(&mut self, branch: &str) {
        if self
            .selected_kild()
            .is_some_and(|s| s.session.branch == branch)
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
    #[cfg(test)]
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

    /// Mark a bulk operation as in-flight.
    pub fn set_bulk_loading(&mut self) {
        self.loading.set_bulk();
    }

    /// Clear the bulk operation flag.
    pub fn clear_bulk_loading(&mut self) {
        self.loading.clear_bulk();
    }

    /// Check if a bulk operation is in-flight.
    pub fn is_bulk_loading(&self) -> bool {
        self.loading.is_bulk()
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
            loading: LoadingState::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use kild_core::sessions::types::SessionStatus;
    use kild_core::{Event, GitStatus, ProcessStatus, Session};
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
    fn test_close_dialog_clears_add_project_state() {
        let mut state = AppState::test_new();
        state.dialog = DialogState::AddProject {
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
        let make_session = |id: &str, project_id: &str| {
            Session::new(
                id.to_string(),
                project_id.to_string(),
                format!("branch-{}", id),
                PathBuf::from("/tmp/test"),
                "claude".to_string(),
                SessionStatus::Active,
                "2024-01-01T00:00:00Z".to_string(),
                0,
                0,
                0,
                None,
                None,
                vec![],
                None,
                None,
            )
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
        // Use the actual hash for the project path
        let project_path = PathBuf::from("/Users/test/Projects/project-a");
        let project_id_a = kild_core::projects::generate_project_id(&project_path);
        let project_id_b =
            kild_core::projects::generate_project_id(&PathBuf::from("/other/project"));

        let make_session = |id: &str, project_id: &str| {
            Session::new(
                id.to_string(),
                project_id.to_string(),
                format!("branch-{}", id),
                PathBuf::from("/tmp/test"),
                "claude".to_string(),
                SessionStatus::Active,
                "2024-01-01T00:00:00Z".to_string(),
                0,
                0,
                0,
                None,
                None,
                vec![],
                None,
                None,
            )
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
        let make_session = |id: &str, project_id: &str| {
            Session::new(
                id.to_string(),
                project_id.to_string(),
                format!("branch-{}", id),
                PathBuf::from("/tmp/test"),
                "claude".to_string(),
                SessionStatus::Active,
                "2024-01-01T00:00:00Z".to_string(),
                0,
                0,
                0,
                None,
                None,
                vec![],
                None,
                None,
            )
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
        let make_session = |id: &str| {
            Session::new(
                id.to_string(),
                "test-project".to_string(),
                format!("branch-{}", id),
                PathBuf::from("/tmp/test"),
                "claude".to_string(),
                SessionStatus::Active,
                "2024-01-01T00:00:00Z".to_string(),
                0,
                0,
                0,
                None,
                None,
                vec![],
                None,
                None,
            )
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
        let make_session = |id: &str| {
            Session::new(
                id.to_string(),
                "test-project".to_string(),
                format!("branch-{}", id),
                PathBuf::from("/tmp/test"),
                "claude".to_string(),
                SessionStatus::Active,
                "2024-01-01T00:00:00Z".to_string(),
                0,
                0,
                0,
                None,
                None,
                vec![],
                None,
                None,
            )
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
        let make_session = |id: &str, branch: &str| {
            Session::new(
                id.to_string(),
                "test-project".to_string(),
                branch.to_string(),
                PathBuf::from("/tmp/test"),
                "claude".to_string(),
                SessionStatus::Active,
                "2024-01-01T00:00:00Z".to_string(),
                0,
                0,
                0,
                None,
                None,
                vec![],
                None,
                None,
            )
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
        let make_session = |id: &str, branch: &str| {
            Session::new(
                id.to_string(),
                "test-project".to_string(),
                branch.to_string(),
                PathBuf::from("/tmp/test"),
                "claude".to_string(),
                SessionStatus::Active,
                "2024-01-01T00:00:00Z".to_string(),
                0,
                0,
                0,
                None,
                None,
                vec![],
                None,
                None,
            )
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

    // --- apply_events tests ---

    fn make_session_for_event_test(id: &str, branch: &str) -> Session {
        Session::new(
            id.to_string(),
            "test-project".to_string(),
            branch.to_string(),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            None,
            None,
            vec![],
            None,
            None,
        )
    }

    #[test]
    fn test_apply_events_handles_empty_vec() {
        let mut state = AppState::test_new();
        state.set_dialog(DialogState::open_create());

        state.apply_events(&[]);

        // Dialog should still be open — no events means no mutations
        assert!(state.dialog().is_create());
    }

    #[test]
    fn test_apply_kild_created_closes_dialog_and_refreshes() {
        let mut state = AppState::test_new();
        state.set_dialog(DialogState::open_create());

        state.apply_events(&[Event::KildCreated {
            branch: "test-branch".to_string(),
            session_id: "test-id".to_string(),
        }]);

        assert!(matches!(state.dialog(), DialogState::None));
    }

    #[test]
    fn test_apply_kild_destroyed_clears_selection_when_selected() {
        let mut state = AppState::test_new();
        state.sessions.set_displays(vec![SessionInfo {
            session: make_session_for_event_test("id-1", "branch-1"),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            diff_stats: None,
        }]);
        state.selection.select("id-1".to_string());
        state.set_dialog(DialogState::open_confirm("branch-1".to_string(), None));

        state.apply_events(&[Event::KildDestroyed {
            branch: "branch-1".to_string(),
        }]);

        assert!(!state.has_selection());
        assert!(matches!(state.dialog(), DialogState::None));
    }

    #[test]
    fn test_apply_kild_destroyed_preserves_selection_when_other() {
        let mut state = AppState::test_new();
        state.sessions.set_displays(vec![
            SessionInfo {
                session: make_session_for_event_test("id-1", "branch-1"),
                process_status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            SessionInfo {
                session: make_session_for_event_test("id-2", "branch-2"),
                process_status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
        ]);
        state.selection.select("id-1".to_string());

        state.apply_events(&[Event::KildDestroyed {
            branch: "branch-2".to_string(),
        }]);

        // Selection of branch-1 should be preserved
        assert!(state.has_selection());
        assert_eq!(state.selected_id(), Some("id-1"));
    }

    #[test]
    fn test_apply_kild_opened_preserves_selection_and_dialog() {
        let mut state = AppState::test_new();
        state.sessions.set_displays(vec![SessionInfo {
            session: make_session_for_event_test("id-1", "branch-1"),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            diff_stats: None,
        }]);
        state.selection.select("id-1".to_string());
        state.set_dialog(DialogState::open_create());

        state.apply_events(&[Event::KildOpened {
            branch: "branch-1".to_string(),
            agent: "claude".to_string(),
        }]);

        assert!(state.dialog().is_create());
        assert!(state.has_selection());
        assert_eq!(state.selected_id(), Some("id-1"));
    }

    #[test]
    fn test_apply_kild_stopped_preserves_selection_and_dialog() {
        let mut state = AppState::test_new();
        state.sessions.set_displays(vec![SessionInfo {
            session: make_session_for_event_test("id-1", "branch-1"),
            process_status: ProcessStatus::Running,
            git_status: GitStatus::Unknown,
            diff_stats: None,
        }]);
        state.selection.select("id-1".to_string());
        state.set_dialog(DialogState::open_create());

        state.apply_events(&[Event::KildStopped {
            branch: "branch-1".to_string(),
        }]);

        assert!(state.dialog().is_create());
        assert!(state.has_selection());
        assert_eq!(state.selected_id(), Some("id-1"));
    }

    #[test]
    fn test_apply_kild_completed_clears_selection_when_selected() {
        let mut state = AppState::test_new();
        state.sessions.set_displays(vec![SessionInfo {
            session: make_session_for_event_test("id-1", "branch-1"),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            diff_stats: None,
        }]);
        state.selection.select("id-1".to_string());

        state.apply_events(&[Event::KildCompleted {
            branch: "branch-1".to_string(),
        }]);

        assert!(!state.has_selection());
    }

    // --- apply_events project tests ---

    #[test]
    fn test_apply_project_added_closes_dialog() {
        let mut state = AppState::test_new();
        state.set_dialog(DialogState::open_add_project());

        state.apply_events(&[Event::ProjectAdded {
            path: PathBuf::from("/tmp/project"),
            name: "Project".to_string(),
        }]);

        assert!(matches!(state.dialog(), DialogState::None));
    }

    #[test]
    fn test_apply_project_removed_does_not_close_dialog() {
        let mut state = AppState::test_new();
        state.set_dialog(DialogState::open_create());

        state.apply_events(&[Event::ProjectRemoved {
            path: PathBuf::from("/tmp/project"),
        }]);

        // Remove project should not close dialogs
        assert!(state.dialog().is_create());
    }

    #[test]
    fn test_apply_active_project_changed() {
        let mut state = AppState::test_new();

        // Should not panic on empty project list
        state.apply_events(&[Event::ActiveProjectChanged {
            path: Some(PathBuf::from("/tmp/project")),
        }]);
    }

    #[test]
    fn test_apply_active_project_changed_to_none() {
        let mut state = AppState::test_new();

        // Should not panic
        state.apply_events(&[Event::ActiveProjectChanged { path: None }]);
    }

    // --- Project error boundary tests ---

    #[test]
    fn test_add_project_error_preserves_dialog() {
        let mut state = AppState::test_new();
        state.set_dialog(DialogState::open_add_project());

        // Simulate dispatch failure (invalid path)
        let error = "Path does not exist".to_string();
        state.set_dialog_error(error.clone());

        // Dialog should remain open with error
        assert!(state.dialog().is_add_project());
        if let DialogState::AddProject { error: e, .. } = state.dialog() {
            assert_eq!(e.as_deref(), Some("Path does not exist"));
        } else {
            panic!("Expected AddProject dialog");
        }
    }

    #[test]
    fn test_add_project_success_closes_dialog() {
        let mut state = AppState::test_new();
        state.set_dialog(DialogState::open_add_project());

        // Simulate successful dispatch
        state.apply_events(&[Event::ProjectAdded {
            path: PathBuf::from("/tmp/project"),
            name: "Project".to_string(),
        }]);

        // Dialog should be closed
        assert!(matches!(state.dialog(), DialogState::None));
    }

    #[test]
    fn test_add_project_error_then_success_clears_error_and_closes() {
        let mut state = AppState::test_new();
        state.set_dialog(DialogState::open_add_project());

        // First attempt fails
        state.set_dialog_error("Not a git repo".to_string());
        assert!(state.dialog().is_add_project());

        // Second attempt succeeds
        state.apply_events(&[Event::ProjectAdded {
            path: PathBuf::from("/tmp/project"),
            name: "Project".to_string(),
        }]);
        assert!(matches!(state.dialog(), DialogState::None));
    }

    #[test]
    fn test_select_project_error_surfaces_in_banner() {
        let mut state = AppState::test_new();
        assert!(!state.has_banner_errors());

        // Simulate select project failure
        state.push_error("Failed to select project: not found".to_string());

        assert!(state.has_banner_errors());
        assert_eq!(state.banner_errors().len(), 1);
        assert_eq!(
            state.banner_errors()[0],
            "Failed to select project: not found"
        );
    }

    #[test]
    fn test_remove_project_error_surfaces_in_banner() {
        let mut state = AppState::test_new();

        state.push_error("Failed to remove project: permission denied".to_string());

        assert!(state.has_banner_errors());
        assert_eq!(
            state.banner_errors()[0],
            "Failed to remove project: permission denied"
        );
    }

    // --- Loading facade tests ---

    #[test]
    fn test_loading_facade_branch() {
        let mut state = AppState::test_new();

        assert!(!state.is_loading("branch-1"));
        state.set_loading("branch-1");
        assert!(state.is_loading("branch-1"));
        assert!(!state.is_loading("branch-2"));
        state.clear_loading("branch-1");
        assert!(!state.is_loading("branch-1"));
    }

    #[test]
    fn test_loading_facade_bulk() {
        let mut state = AppState::test_new();

        assert!(!state.is_bulk_loading());
        state.set_bulk_loading();
        assert!(state.is_bulk_loading());
        state.clear_bulk_loading();
        assert!(!state.is_bulk_loading());
    }

    #[test]
    fn test_loading_facade_dialog() {
        let mut state = AppState::test_new();

        assert!(!state.is_dialog_loading());
        state.set_dialog_loading();
        assert!(state.is_dialog_loading());
        state.clear_dialog_loading();
        assert!(!state.is_dialog_loading());
    }

    #[test]
    fn test_multiple_branches_load_independently() {
        let mut state = AppState::test_new();

        state.set_loading("branch-1");
        state.set_loading("branch-2");

        assert!(state.is_loading("branch-1"));
        assert!(state.is_loading("branch-2"));

        state.clear_loading("branch-1");
        assert!(!state.is_loading("branch-1"));
        assert!(state.is_loading("branch-2"));
    }

    #[test]
    fn test_loading_dimensions_independent() {
        let mut state = AppState::test_new();

        state.set_bulk_loading();
        assert!(!state.is_loading("branch-1"));
        assert!(!state.is_dialog_loading());

        state.set_loading("branch-1");
        assert!(!state.is_dialog_loading());

        state.set_dialog_loading();
        state.clear_bulk_loading();
        assert!(state.is_loading("branch-1"));
        assert!(state.is_dialog_loading());
    }

    #[test]
    fn test_set_loading_does_not_clear_existing_error() {
        let mut state = AppState::test_new();
        state.set_error(
            "branch-1",
            OperationError {
                branch: "branch-1".to_string(),
                message: "Previous error".to_string(),
            },
        );

        state.set_loading("branch-1");

        assert!(state.get_error("branch-1").is_some());
        assert_eq!(
            state.get_error("branch-1").unwrap().message,
            "Previous error"
        );
    }

    #[test]
    fn test_error_persists_through_loading_lifecycle() {
        let mut state = AppState::test_new();
        state.set_error(
            "branch-1",
            OperationError {
                branch: "branch-1".to_string(),
                message: "Error message".to_string(),
            },
        );

        // Error persists through loading set/clear cycle
        state.set_loading("branch-1");
        assert!(state.get_error("branch-1").is_some());

        state.clear_loading("branch-1");
        assert!(state.get_error("branch-1").is_some());
    }
}
