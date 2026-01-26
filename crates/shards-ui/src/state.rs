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

/// Display data for a shard, combining Session with computed process status.
#[derive(Clone)]
pub struct ShardDisplay {
    pub session: Session,
    pub status: ProcessStatus,
}

impl ShardDisplay {
    pub fn from_session(session: Session) -> Self {
        let status = Self::check_process_status(session.process_id, &session.branch);
        Self { session, status }
    }

    fn check_process_status(process_id: Option<u32>, branch: &str) -> ProcessStatus {
        let Some(pid) = process_id else {
            return ProcessStatus::Stopped;
        };

        match shards_core::process::is_process_running(pid) {
            Ok(true) => ProcessStatus::Running,
            Ok(false) => ProcessStatus::Stopped,
            Err(e) => {
                tracing::warn!(
                    event = "ui.shard_list.process_check_failed",
                    pid = pid,
                    branch = branch,
                    error = %e
                );
                ProcessStatus::Unknown
            }
        }
    }
}

/// Form state for creating a new shard.
#[derive(Clone, Debug)]
pub struct CreateFormState {
    pub branch_name: String,
    pub selected_agent: String,
    pub selected_agent_index: usize,
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
                selected_agent: default_agent.to_string(),
                selected_agent_index: 0,
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
            selected_agent: agents[index].to_string(),
            selected_agent_index: index,
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
            shard_display.status = ShardDisplay::check_process_status(
                shard_display.session.process_id,
                &shard_display.session.branch,
            );
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

        let display = ShardDisplay::from_session(session);
        assert_eq!(display.status, ProcessStatus::Stopped);
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
                },
                ShardDisplay {
                    session: session_with_live_pid,
                    status: ProcessStatus::Stopped, // Start as Stopped (incorrect)
                },
                ShardDisplay {
                    session: session_no_pid,
                    status: ProcessStatus::Stopped, // Start as Stopped (correct)
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
}
