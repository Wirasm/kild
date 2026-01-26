//! Application state for shards-ui.
//!
//! Centralized state management for the GUI, including shard list,
//! create dialog, and form state.

use shards_core::Session;

/// Process status for a shard, distinguishing between running, stopped, and unknown states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessStatus {
    /// Process is confirmed running
    Running,
    /// Process is confirmed stopped (or no PID exists)
    Stopped,
    /// Could not determine status (process check failed)
    Unknown,
}

/// Error from a shard operation, with the branch name for context.
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

/// Display data for a shard, combining Session with computed process status.
#[derive(Clone)]
pub struct ShardDisplay {
    pub session: Session,
    pub status: ProcessStatus,
    pub git_status: GitStatus,
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
                event = "ui.shard_list.git_status_failed",
                path = %worktree_path.display(),
                exit_code = ?output.status.code(),
                stderr = %String::from_utf8_lossy(&output.stderr),
                "Git status command failed"
            );
            GitStatus::Unknown
        }
        Err(e) => {
            tracing::warn!(
                event = "ui.shard_list.git_status_error",
                path = %worktree_path.display(),
                error = %e,
                "Failed to execute git status"
            );
            GitStatus::Unknown
        }
    }
}

impl ShardDisplay {
    pub fn from_session(session: Session) -> Self {
        let status = match session.process_id {
            None => ProcessStatus::Stopped,
            Some(pid) => match shards_core::process::is_process_running(pid) {
                Ok(true) => ProcessStatus::Running,
                Ok(false) => ProcessStatus::Stopped,
                Err(e) => {
                    tracing::warn!(
                        event = "ui.shard_list.process_check_failed",
                        pid = pid,
                        branch = session.branch,
                        error = %e
                    );
                    ProcessStatus::Unknown
                }
            },
        };

        let git_status = if session.worktree_path.exists() {
            check_git_status(&session.worktree_path)
        } else {
            GitStatus::Unknown
        };

        Self {
            session,
            status,
            git_status,
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

/// Form state for creating a new shard.
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
    /// agent if the index is out of bounds.
    pub fn selected_agent(&self) -> String {
        let agents = shards_core::agents::valid_agent_names();
        agents
            .get(self.selected_agent_index)
            .copied()
            .unwrap_or_else(|| shards_core::agents::default_agent_name())
            .to_string()
    }
}

impl Default for CreateFormState {
    fn default() -> Self {
        let agents = shards_core::agents::valid_agent_names();
        let default_agent = shards_core::agents::default_agent_name();

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
                tracing::info!(
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

/// Main application state.
pub struct AppState {
    pub displays: Vec<ShardDisplay>,
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

    /// Timestamp of last successful status refresh
    pub last_refresh: std::time::Instant,
}

impl AppState {
    /// Create new application state, loading sessions from disk.
    pub fn new() -> Self {
        let (displays, load_error) = crate::actions::refresh_sessions();

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
            last_refresh: std::time::Instant::now(),
        }
    }

    /// Refresh sessions from disk.
    pub fn refresh_sessions(&mut self) {
        let (displays, load_error) = crate::actions::refresh_sessions();
        self.displays = displays;
        self.load_error = load_error;
        self.last_refresh = std::time::Instant::now();
    }

    /// Update only the process status of existing shards without reloading from disk.
    ///
    /// This is faster than refresh_sessions() for status polling because it:
    /// - Doesn't reload session files from disk
    /// - Only checks if tracked processes are still running
    /// - Preserves the existing shard list structure
    pub fn update_statuses_only(&mut self) {
        for shard_display in &mut self.displays {
            shard_display.status = match shard_display.session.process_id {
                None => ProcessStatus::Stopped,
                Some(pid) => match shards_core::process::is_process_running(pid) {
                    Ok(true) => ProcessStatus::Running,
                    Ok(false) => ProcessStatus::Stopped,
                    Err(e) => {
                        tracing::warn!(
                            event = "ui.shard_list.process_check_failed",
                            pid = pid,
                            branch = shard_display.session.branch,
                            error = %e
                        );
                        ProcessStatus::Unknown
                    }
                },
            };
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

    /// Count shards with Stopped status.
    pub fn stopped_count(&self) -> usize {
        self.displays
            .iter()
            .filter(|d| d.status == ProcessStatus::Stopped)
            .count()
    }

    /// Count shards with Running status.
    pub fn running_count(&self) -> usize {
        self.displays
            .iter()
            .filter(|d| d.status == ProcessStatus::Running)
            .count()
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

    #[test]
    fn test_reset_confirm_dialog_clears_all_fields() {
        // Create state with confirm dialog open and an error
        let mut state = AppState {
            displays: Vec::new(),
            load_error: None,
            show_create_dialog: false,
            create_form: CreateFormState::default(),
            create_error: None,
            show_confirm_dialog: true,
            confirm_target_branch: Some("feature-branch".to_string()),
            confirm_error: Some("Some error".to_string()),
            open_error: None,
            stop_error: None,
            bulk_errors: Vec::new(),
            last_refresh: std::time::Instant::now(),
        };

        state.reset_confirm_dialog();

        assert!(!state.show_confirm_dialog);
        assert!(state.confirm_target_branch.is_none());
        assert!(state.confirm_error.is_none());
    }

    #[test]
    fn test_clear_open_error() {
        let mut state = AppState {
            displays: Vec::new(),
            load_error: None,
            show_create_dialog: false,
            create_form: CreateFormState::default(),
            create_error: None,
            show_confirm_dialog: false,
            confirm_target_branch: None,
            confirm_error: None,
            open_error: Some(OperationError {
                branch: "branch".to_string(),
                message: "error".to_string(),
            }),
            stop_error: None,
            bulk_errors: Vec::new(),
            last_refresh: std::time::Instant::now(),
        };

        state.clear_open_error();

        assert!(state.open_error.is_none());
    }

    #[test]
    fn test_clear_stop_error() {
        let mut state = AppState {
            displays: Vec::new(),
            load_error: None,
            show_create_dialog: false,
            create_form: CreateFormState::default(),
            create_error: None,
            show_confirm_dialog: false,
            confirm_target_branch: None,
            confirm_error: None,
            open_error: None,
            stop_error: Some(OperationError {
                branch: "branch".to_string(),
                message: "error".to_string(),
            }),
            bulk_errors: Vec::new(),
            last_refresh: std::time::Instant::now(),
        };

        state.clear_stop_error();

        assert!(state.stop_error.is_none());
    }

    #[test]
    fn test_process_status_from_session_no_pid() {
        use shards_core::sessions::types::SessionStatus;
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

        let display = ShardDisplay::from_session(session);
        assert_eq!(display.status, ProcessStatus::Stopped);
        // Non-existent path should result in Unknown git status
        assert_eq!(display.git_status, GitStatus::Unknown);
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
        let agents = shards_core::agents::valid_agent_names();

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
        let expected = shards_core::agents::default_agent_name();
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
        let mut state = AppState {
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
            last_refresh: initial_time,
        };

        // Small delay to ensure time difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        state.update_statuses_only();

        // last_refresh should be updated to a later time
        assert!(state.last_refresh > initial_time);
    }

    #[test]
    fn test_update_statuses_only_updates_process_status() {
        use shards_core::sessions::types::SessionStatus;
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

        let mut state = AppState {
            displays: vec![
                ShardDisplay {
                    session: session_with_dead_pid,
                    status: ProcessStatus::Running, // Start as Running (incorrect)
                    git_status: GitStatus::Unknown,
                },
                ShardDisplay {
                    session: session_with_live_pid,
                    status: ProcessStatus::Stopped, // Start as Stopped (incorrect)
                    git_status: GitStatus::Unknown,
                },
                ShardDisplay {
                    session: session_no_pid,
                    status: ProcessStatus::Stopped, // Start as Stopped (correct)
                    git_status: GitStatus::Unknown,
                },
            ],
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
            last_refresh: std::time::Instant::now(),
        };

        state.update_statuses_only();

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
        let state = AppState {
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
            last_refresh: std::time::Instant::now(),
        };

        assert_eq!(state.stopped_count(), 0);
        assert_eq!(state.running_count(), 0);
    }

    #[test]
    fn test_stopped_and_running_counts() {
        use shards_core::sessions::types::SessionStatus;
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

        let state = AppState {
            displays: vec![
                ShardDisplay {
                    session: make_session("1", "branch-1"),
                    status: ProcessStatus::Stopped,
                    git_status: GitStatus::Unknown,
                },
                ShardDisplay {
                    session: make_session("2", "branch-2"),
                    status: ProcessStatus::Running,
                    git_status: GitStatus::Unknown,
                },
                ShardDisplay {
                    session: make_session("3", "branch-3"),
                    status: ProcessStatus::Stopped,
                    git_status: GitStatus::Unknown,
                },
                ShardDisplay {
                    session: make_session("4", "branch-4"),
                    status: ProcessStatus::Running,
                    git_status: GitStatus::Unknown,
                },
                ShardDisplay {
                    session: make_session("5", "branch-5"),
                    status: ProcessStatus::Unknown,
                    git_status: GitStatus::Unknown,
                },
            ],
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
            last_refresh: std::time::Instant::now(),
        };

        assert_eq!(state.stopped_count(), 2, "Should count 2 stopped shards");
        assert_eq!(state.running_count(), 2, "Should count 2 running shards");
    }
}
