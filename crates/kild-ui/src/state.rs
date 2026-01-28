//! Application state for kild-ui.
//!
//! Centralized state management for the GUI, including kild list,
//! create dialog, and form state.

use kild_core::Session;
use kild_core::git::{operations::get_diff_stats, types::DiffStats};
use std::path::PathBuf;

use crate::projects::Project;

/// Process status for a kild, distinguishing between running, stopped, and unknown states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessStatus {
    /// Process is confirmed running
    Running,
    /// Process is confirmed stopped (or no PID exists)
    Stopped,
    /// Could not determine status (process check failed)
    Unknown,
}

/// Error from a kild operation, with the branch name for context.
#[derive(Clone, Debug)]
pub struct OperationError {
    pub branch: String,
    pub message: String,
}

/// Git status for a worktree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitStatus {
    /// Worktree has no uncommitted changes
    Clean,
    /// Worktree has uncommitted changes
    Dirty,
    /// Could not determine git status (error occurred)
    Unknown,
}

/// Display data for a kild, combining Session with computed process status.
#[derive(Clone)]
pub struct KildDisplay {
    pub session: Session,
    pub status: ProcessStatus,
    pub git_status: GitStatus,
    pub diff_stats: Option<DiffStats>,
}

/// Check if a worktree has uncommitted changes.
///
/// Returns `GitStatus::Dirty` if there are uncommitted changes,
/// `GitStatus::Clean` if the worktree is clean, or `GitStatus::Unknown`
/// if the git status check failed.
fn check_git_status(worktree_path: &std::path::Path) -> GitStatus {
    match std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output()
    {
        Ok(output) if output.status.success() => {
            if output.stdout.is_empty() {
                GitStatus::Clean
            } else {
                GitStatus::Dirty
            }
        }
        Ok(output) => {
            tracing::warn!(
                event = "ui.kild_list.git_status_failed",
                path = %worktree_path.display(),
                exit_code = ?output.status.code(),
                stderr = %String::from_utf8_lossy(&output.stderr),
                "Git status command failed"
            );
            GitStatus::Unknown
        }
        Err(e) => {
            tracing::warn!(
                event = "ui.kild_list.git_status_error",
                path = %worktree_path.display(),
                error = %e,
                "Failed to execute git status"
            );
            GitStatus::Unknown
        }
    }
}

/// Determine process status from session data.
///
/// Uses PID-based detection as primary method, falling back to window-based
/// detection for terminals like Ghostty where PID is unavailable.
fn determine_process_status(session: &Session) -> ProcessStatus {
    if let Some(pid) = session.process_id {
        // Primary: PID-based detection
        match kild_core::process::is_process_running(pid) {
            Ok(true) => ProcessStatus::Running,
            Ok(false) => ProcessStatus::Stopped,
            Err(e) => {
                tracing::warn!(
                    event = "ui.kild_list.process_check_failed",
                    pid = pid,
                    branch = session.branch,
                    error = %e
                );
                ProcessStatus::Unknown
            }
        }
    } else if let (Some(terminal_type), Some(window_id)) =
        (&session.terminal_type, &session.terminal_window_id)
    {
        // Fallback: Window-based detection for Ghostty (open -na doesn't return PID)
        match kild_core::terminal::is_terminal_window_open(terminal_type, window_id) {
            Ok(Some(true)) => ProcessStatus::Running,
            Ok(Some(false)) => ProcessStatus::Stopped,
            Ok(None) => ProcessStatus::Stopped, // Backend doesn't support window detection
            Err(e) => {
                tracing::warn!(
                    event = "ui.kild_list.window_check_failed",
                    terminal_type = ?terminal_type,
                    window_id = %window_id,
                    branch = session.branch,
                    error = %e
                );
                ProcessStatus::Stopped
            }
        }
    } else {
        ProcessStatus::Stopped
    }
}

impl KildDisplay {
    pub fn from_session(session: Session) -> Self {
        let status = determine_process_status(&session);

        let git_status = if session.worktree_path.exists() {
            check_git_status(&session.worktree_path)
        } else {
            GitStatus::Unknown
        };

        // Compute diff stats if worktree exists and is dirty
        let diff_stats = if git_status == GitStatus::Dirty {
            match get_diff_stats(&session.worktree_path) {
                Ok(stats) => Some(stats),
                Err(e) => {
                    tracing::warn!(
                        event = "ui.kild_list.diff_stats_failed",
                        path = %session.worktree_path.display(),
                        error = %e,
                        "Failed to compute diff stats - showing fallback indicator"
                    );
                    None
                }
            }
        } else {
            None
        };

        Self {
            session,
            status,
            git_status,
            diff_stats,
        }
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
pub struct AppState {
    pub displays: Vec<KildDisplay>,
    pub load_error: Option<String>,
    pub show_create_dialog: bool,
    pub create_form: CreateFormState,
    pub create_error: Option<String>,

    // Confirm dialog state
    pub show_confirm_dialog: bool,
    pub confirm_target_branch: Option<String>,
    pub confirm_error: Option<String>,

    // Open error state (shown inline per-row)
    pub open_error: Option<OperationError>,

    // Stop error state (shown inline per-row)
    pub stop_error: Option<OperationError>,

    // Bulk operation errors (shown as banner)
    pub bulk_errors: Vec<OperationError>,

    // Editor error state (shown inline per-row)
    pub editor_error: Option<OperationError>,

    // Focus terminal error state (shown inline per-row)
    pub focus_error: Option<OperationError>,

    /// ID of the currently selected kild (for detail panel)
    pub selected_kild_id: Option<String>,

    /// Timestamp of last successful status refresh
    pub last_refresh: std::time::Instant,

    // Project management state
    pub projects: Vec<Project>,
    pub active_project: Option<PathBuf>,
    pub show_add_project_dialog: bool,
    pub add_project_form: AddProjectFormState,
    pub add_project_error: Option<String>,
}

impl AppState {
    /// Create new application state, loading sessions from disk.
    pub fn new() -> Self {
        let (displays, load_error) = crate::actions::refresh_sessions();

        // Migrate projects to canonical paths (fixes case mismatch on macOS)
        if let Err(e) = crate::projects::migrate_projects_to_canonical() {
            tracing::warn!(
                event = "ui.projects.migration_failed",
                error = %e,
                "Project migration failed - some projects may not filter correctly"
            );
        }

        // Load projects from disk (after migration)
        let projects_data = crate::projects::load_projects();

        Self {
            displays,
            load_error,
            show_create_dialog: false,
            create_form: CreateFormState::default(),
            create_error: None,
            show_confirm_dialog: false,
            confirm_target_branch: None,
            confirm_error: None,
            open_error: None,
            stop_error: None,
            bulk_errors: Vec::new(),
            editor_error: None,
            focus_error: None,
            selected_kild_id: None,
            last_refresh: std::time::Instant::now(),
            projects: projects_data.projects,
            active_project: projects_data.active,
            show_add_project_dialog: false,
            add_project_form: AddProjectFormState::default(),
            add_project_error: None,
        }
    }

    /// Refresh sessions from disk.
    pub fn refresh_sessions(&mut self) {
        let (displays, load_error) = crate::actions::refresh_sessions();
        self.displays = displays;
        self.load_error = load_error;
        self.last_refresh = std::time::Instant::now();
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
        // Check if session count changed (external create/destroy)
        let disk_count_result = count_session_files();

        if let Some(disk_count) = disk_count_result {
            if disk_count != self.displays.len() {
                tracing::info!(
                    event = "ui.auto_refresh.session_count_mismatch",
                    disk_count = disk_count,
                    memory_count = self.displays.len(),
                    action = "triggering full refresh"
                );
                self.refresh_sessions();
                return;
            }
        } else {
            // Cannot determine count - skip mismatch check and just update statuses.
            // This is a non-critical degradation (directory read failure).
            tracing::debug!(
                event = "ui.auto_refresh.count_check_skipped",
                reason = "cannot read sessions directory"
            );
        }

        // No count change (or count unavailable) - just update process statuses
        for kild_display in &mut self.displays {
            kild_display.status = determine_process_status(&kild_display.session);
        }
        self.last_refresh = std::time::Instant::now();
    }

    /// Reset the create form to default state.
    pub fn reset_create_form(&mut self) {
        self.create_form = CreateFormState::default();
        self.create_error = None;
    }

    /// Reset the confirm dialog to default state.
    pub fn reset_confirm_dialog(&mut self) {
        self.show_confirm_dialog = false;
        self.confirm_target_branch = None;
        self.confirm_error = None;
    }

    /// Clear any open error.
    pub fn clear_open_error(&mut self) {
        self.open_error = None;
    }

    /// Clear any stop error.
    pub fn clear_stop_error(&mut self) {
        self.stop_error = None;
    }

    /// Clear any bulk operation errors.
    pub fn clear_bulk_errors(&mut self) {
        self.bulk_errors.clear();
    }

    /// Clear any editor error.
    pub fn clear_editor_error(&mut self) {
        self.editor_error = None;
    }

    /// Clear any focus terminal error.
    pub fn clear_focus_error(&mut self) {
        self.focus_error = None;
    }

    /// Reset the add project form to default state.
    pub fn reset_add_project_form(&mut self) {
        self.add_project_form = AddProjectFormState::default();
        self.add_project_error = None;
    }

    /// Get the project ID for the active project.
    pub fn active_project_id(&self) -> Option<String> {
        self.active_project
            .as_ref()
            .map(|p| crate::projects::derive_project_id(p))
    }

    /// Get displays filtered by active project.
    ///
    /// Filters kilds where `session.project_id` matches the derived ID of the active project path.
    /// Uses path-based hashing that matches kild-core's `generate_project_id`.
    /// If no active project is set, returns all displays (unfiltered).
    pub fn filtered_displays(&self) -> Vec<&KildDisplay> {
        if let Some(active_id) = self.active_project_id() {
            self.displays
                .iter()
                .filter(|d| d.session.project_id == active_id)
                .collect()
        } else {
            // No active project - show all kilds
            self.displays.iter().collect()
        }
    }

    /// Count kilds with Stopped status.
    pub fn stopped_count(&self) -> usize {
        self.displays
            .iter()
            .filter(|d| d.status == ProcessStatus::Stopped)
            .count()
    }

    /// Count kilds with Running status.
    pub fn running_count(&self) -> usize {
        self.displays
            .iter()
            .filter(|d| d.status == ProcessStatus::Running)
            .count()
    }

    /// Count kilds for a specific project (by project path).
    pub fn kild_count_for_project(&self, project_path: &std::path::Path) -> usize {
        let project_id = crate::projects::derive_project_id(project_path);
        self.displays
            .iter()
            .filter(|d| d.session.project_id == project_id)
            .count()
    }

    /// Count total kilds across all projects.
    pub fn total_kild_count(&self) -> usize {
        self.displays.len()
    }

    /// Get the selected kild display, if any.
    ///
    /// Returns `None` if no kild is selected or if the selected kild no longer
    /// exists in the current display list (e.g., after being destroyed externally).
    pub fn selected_kild(&self) -> Option<&KildDisplay> {
        let id = self.selected_kild_id.as_ref()?;

        match self.displays.iter().find(|d| d.session.id == *id) {
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
        self.selected_kild_id = None;
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Count session files on disk without fully loading them.
///
/// This is a lightweight check used by `update_statuses_only()` to detect
/// when sessions have been added or removed externally (e.g., via CLI).
///
/// Returns `None` if the directory cannot be read (permission error, I/O error, etc.),
/// allowing the caller to distinguish between "0 sessions exist" and "cannot determine count".
fn count_session_files() -> Option<usize> {
    let config = kild_core::config::Config::new();
    let sessions_dir = config.sessions_dir();

    if !sessions_dir.exists() {
        return Some(0);
    }

    match std::fs::read_dir(&sessions_dir) {
        Ok(entries) => {
            let count = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
                .count();
            Some(count)
        }
        Err(e) => {
            tracing::warn!(
                event = "ui.count_session_files.read_dir_failed",
                path = %sessions_dir.display(),
                error = %e
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a minimal AppState for testing.
    fn make_test_state() -> AppState {
        AppState {
            displays: Vec::new(),
            load_error: None,
            show_create_dialog: false,
            create_form: CreateFormState::default(),
            create_error: None,
            show_confirm_dialog: false,
            confirm_target_branch: None,
            confirm_error: None,
            open_error: None,
            stop_error: None,
            bulk_errors: Vec::new(),
            editor_error: None,
            focus_error: None,
            selected_kild_id: None,
            last_refresh: std::time::Instant::now(),
            projects: Vec::new(),
            active_project: None,
            show_add_project_dialog: false,
            add_project_form: AddProjectFormState::default(),
            add_project_error: None,
        }
    }

    #[test]
    fn test_reset_confirm_dialog_clears_all_fields() {
        // Create state with confirm dialog open and an error
        let mut state = make_test_state();
        state.show_confirm_dialog = true;
        state.confirm_target_branch = Some("feature-branch".to_string());
        state.confirm_error = Some("Some error".to_string());

        state.reset_confirm_dialog();

        assert!(!state.show_confirm_dialog);
        assert!(state.confirm_target_branch.is_none());
        assert!(state.confirm_error.is_none());
    }

    #[test]
    fn test_clear_open_error() {
        let mut state = make_test_state();
        state.open_error = Some(OperationError {
            branch: "branch".to_string(),
            message: "error".to_string(),
        });

        state.clear_open_error();

        assert!(state.open_error.is_none());
    }

    #[test]
    fn test_clear_stop_error() {
        let mut state = make_test_state();
        state.stop_error = Some(OperationError {
            branch: "branch".to_string(),
            message: "error".to_string(),
        });

        state.clear_stop_error();

        assert!(state.stop_error.is_none());
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

        let display = KildDisplay::from_session(session);
        assert_eq!(display.status, ProcessStatus::Stopped);
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

        let display = KildDisplay::from_session(session);
        // With window detection fallback, should attempt to check window
        // In test environment without Ghostty running, will fall back to Stopped
        assert!(
            display.status == ProcessStatus::Stopped || display.status == ProcessStatus::Running,
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

        let display = KildDisplay::from_session(session);

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

    #[test]
    fn test_git_status_clean_repo() {
        use std::process::Command;
        use tempfile::TempDir;

        // Create a temp directory and initialize a git repo
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Initialize git repo
        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init failed");

        // Configure git user (required for commit)
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .expect("git config email failed");
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(path)
            .output()
            .expect("git config name failed");

        // Create a file and commit it
        std::fs::write(path.join("test.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .expect("git add failed");
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(path)
            .output()
            .expect("git commit failed");

        // Now repo should be clean
        let status = check_git_status(path);
        assert_eq!(status, GitStatus::Clean, "Expected clean repo after commit");
    }

    #[test]
    fn test_git_status_dirty_repo() {
        use std::process::Command;
        use tempfile::TempDir;

        // Create a temp directory and initialize a git repo
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Initialize git repo
        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init failed");

        // Create an uncommitted file
        std::fs::write(path.join("test.txt"), "hello").unwrap();

        // Repo should be dirty
        let status = check_git_status(path);
        assert_eq!(
            status,
            GitStatus::Dirty,
            "Expected dirty repo with uncommitted files"
        );
    }

    #[test]
    fn test_git_status_non_git_directory() {
        use tempfile::TempDir;

        // Create a temp directory that is NOT a git repo
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Should return Unknown (git command fails in non-git directory)
        let status = check_git_status(path);
        assert_eq!(
            status,
            GitStatus::Unknown,
            "Expected Unknown for non-git directory"
        );
    }

    #[test]
    fn test_git_status_nonexistent_directory() {
        use std::path::Path;

        let path = Path::new("/nonexistent/path/that/does/not/exist");

        // Should return Unknown (git command fails)
        let status = check_git_status(path);
        assert_eq!(
            status,
            GitStatus::Unknown,
            "Expected Unknown for nonexistent directory"
        );
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
        let mut form = CreateFormState::default();
        // Set an invalid index
        form.selected_agent_index = 999;

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
        let mut state = make_test_state();
        state.last_refresh = initial_time;

        // Small delay to ensure time difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        state.update_statuses_only();

        // last_refresh should be updated to a later time
        assert!(state.last_refresh > initial_time);
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

        let mut state = make_test_state();
        state.displays = vec![
            KildDisplay {
                session: session_with_dead_pid,
                status: ProcessStatus::Running, // Start as Running (incorrect)
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            KildDisplay {
                session: session_with_live_pid,
                status: ProcessStatus::Stopped, // Start as Stopped (incorrect)
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            KildDisplay {
                session: session_no_pid,
                status: ProcessStatus::Stopped, // Start as Stopped (correct)
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
        ];

        let original_len = state.displays.len();
        state.update_statuses_only();

        // Note: update_statuses_only() may trigger a full refresh if the session count
        // on disk differs from the in-memory count (see issue #103 fix). In that case,
        // the displays will be replaced with whatever is on disk.
        //
        // If the display count changed, a refresh was triggered and we can't test
        // the status update logic directly. Skip the assertions in that case.
        if state.displays.len() != original_len {
            // Refresh was triggered due to count mismatch - this is expected behavior
            // when running tests in an environment with actual session files.
            return;
        }

        // Non-existent PID should be marked Stopped
        assert_eq!(
            state.displays[0].status,
            ProcessStatus::Stopped,
            "Non-existent PID should be marked Stopped"
        );

        // Current process PID should be marked Running
        assert_eq!(
            state.displays[1].status,
            ProcessStatus::Running,
            "Current process PID should be marked Running"
        );

        // No PID should remain Stopped (not checked, so unchanged)
        assert_eq!(
            state.displays[2].status,
            ProcessStatus::Stopped,
            "Session with no PID should remain Stopped"
        );
    }

    #[test]
    fn test_stopped_count_empty() {
        let state = make_test_state();

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

        let mut state = make_test_state();
        state.displays = vec![
            KildDisplay {
                session: make_session("1", "branch-1"),
                status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            KildDisplay {
                session: make_session("2", "branch-2"),
                status: ProcessStatus::Running,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            KildDisplay {
                session: make_session("3", "branch-3"),
                status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            KildDisplay {
                session: make_session("4", "branch-4"),
                status: ProcessStatus::Running,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            KildDisplay {
                session: make_session("5", "branch-5"),
                status: ProcessStatus::Unknown,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
        ];

        assert_eq!(state.stopped_count(), 2, "Should count 2 stopped kilds");
        assert_eq!(state.running_count(), 2, "Should count 2 running kilds");
    }

    // --- Project-related tests ---

    #[test]
    fn test_reset_add_project_form() {
        let mut state = make_test_state();
        state.add_project_form.path = "/some/path".to_string();
        state.add_project_form.name = "test".to_string();
        state.add_project_error = Some("Error".to_string());

        state.reset_add_project_form();

        assert!(state.add_project_form.path.is_empty());
        assert!(state.add_project_form.name.is_empty());
        assert!(state.add_project_error.is_none());
    }

    #[test]
    fn test_active_project_id() {
        let mut state = make_test_state();

        // No active project
        assert!(state.active_project_id().is_none());

        // With active project - should return a hash, not directory name
        state.active_project = Some(PathBuf::from("/Users/test/Projects/my-project"));
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

        let mut state = make_test_state();
        state.displays = vec![
            KildDisplay {
                session: make_session("1", "project-a"),
                status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            KildDisplay {
                session: make_session("2", "project-b"),
                status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
        ];

        // No active project - should return all
        let filtered = state.filtered_displays();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filtered_displays_with_active_project() {
        use kild_core::sessions::types::SessionStatus;

        // Use the actual hash for the project path
        let project_path = PathBuf::from("/Users/test/Projects/project-a");
        let project_id_a = crate::projects::derive_project_id(&project_path);
        let project_id_b = crate::projects::derive_project_id(&PathBuf::from("/other/project"));

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

        let mut state = make_test_state();
        state.displays = vec![
            KildDisplay {
                session: make_session("1", &project_id_a),
                status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            KildDisplay {
                session: make_session("2", &project_id_b),
                status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            KildDisplay {
                session: make_session("3", &project_id_a),
                status: ProcessStatus::Running,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
        ];

        // Active project set - should filter
        state.active_project = Some(project_path);
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

        let mut state = make_test_state();
        state.displays = vec![KildDisplay {
            session: make_session("1", "other-project-hash"),
            status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            diff_stats: None,
        }];

        // Active project set to a different path - should return empty
        state.active_project = Some(PathBuf::from("/different/project/path"));
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

        let mut state = make_test_state();
        state.displays = vec![KildDisplay {
            session: make_session("test-id"),
            status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            diff_stats: None,
        }];
        state.selected_kild_id = Some("test-id".to_string());

        // Verify selection works initially
        assert!(state.selected_kild().is_some());

        // Simulate refresh that removes the kild (e.g., destroyed via CLI)
        state.displays.clear();

        // Selection ID still set, but selected_kild() should return None gracefully
        assert!(state.selected_kild_id.is_some());
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

        let mut state = make_test_state();
        state.displays = vec![KildDisplay {
            session: make_session("test-id"),
            status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            diff_stats: None,
        }];
        state.selected_kild_id = Some("test-id".to_string());

        // Verify initial selection
        assert!(state.selected_kild().is_some());

        // Simulate refresh that keeps the same kild (new display list with same ID)
        state.displays = vec![KildDisplay {
            session: make_session("test-id"),
            status: ProcessStatus::Running, // Status may change
            git_status: GitStatus::Dirty,   // Git status may change
            diff_stats: None,
        }];

        // Selection should persist
        let selected = state.selected_kild();
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().session.id, "test-id");
    }

    #[test]
    fn test_clear_selection_clears_selected_kild_id() {
        let mut state = make_test_state();
        state.selected_kild_id = Some("test-id".to_string());

        assert!(state.selected_kild_id.is_some());

        state.clear_selection();

        assert!(
            state.selected_kild_id.is_none(),
            "clear_selection should set selected_kild_id to None"
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

        let mut state = make_test_state();
        state.displays = vec![
            KildDisplay {
                session: make_session("id-1", "branch-1"),
                status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            KildDisplay {
                session: make_session("id-2", "branch-2"),
                status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
        ];
        state.selected_kild_id = Some("id-1".to_string());

        // Simulate destroy of selected kild - the destroy handler logic:
        // if selected_kild().session.branch == destroyed_branch { clear_selection() }
        let destroyed_branch = "branch-1";
        if let Some(selected) = state.selected_kild() {
            if selected.session.branch == destroyed_branch {
                state.clear_selection();
            }
        }
        state
            .displays
            .retain(|d| d.session.branch != destroyed_branch);

        // Selection should be cleared
        assert!(
            state.selected_kild_id.is_none(),
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

        let mut state = make_test_state();
        state.displays = vec![
            KildDisplay {
                session: make_session("id-1", "branch-1"),
                status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
            KildDisplay {
                session: make_session("id-2", "branch-2"),
                status: ProcessStatus::Stopped,
                git_status: GitStatus::Unknown,
                diff_stats: None,
            },
        ];
        state.selected_kild_id = Some("id-1".to_string());

        // Destroy branch-2 (not selected)
        let destroyed_branch = "branch-2";
        if let Some(selected) = state.selected_kild() {
            if selected.session.branch == destroyed_branch {
                state.clear_selection();
            }
        }
        state
            .displays
            .retain(|d| d.session.branch != destroyed_branch);

        // Selection of branch-1 should persist
        assert_eq!(
            state.selected_kild_id,
            Some("id-1".to_string()),
            "Selection should persist when a different kild is destroyed"
        );
        assert!(state.selected_kild().is_some());
    }
}
