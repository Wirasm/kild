//! Business logic handlers for shards-ui.
//!
//! This module contains functions that interact with shards-core
//! to perform operations like creating, destroying, relaunching, and listing shards.

use shards_core::{CreateSessionRequest, Session, ShardsConfig, session_ops};

use crate::state::{OperationError, ProcessStatus, ShardDisplay};

/// Create a new shard with the given branch name, agent, and optional note.
///
/// Returns the created session on success, or an error message on failure.
pub fn create_shard(branch: &str, agent: &str, note: Option<String>) -> Result<Session, String> {
    tracing::info!(
        event = "ui.create_shard.started",
        branch = branch,
        agent = agent,
        note = ?note
    );

    if branch.trim().is_empty() {
        tracing::warn!(
            event = "ui.create_dialog.validation_failed",
            reason = "empty branch name"
        );
        return Err("Branch name cannot be empty".to_string());
    }

    let config = match ShardsConfig::load_hierarchy() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(
                event = "ui.create_shard.config_load_failed",
                error = %e
            );
            return Err(format!("Failed to load config: {e}"));
        }
    };

    let request = CreateSessionRequest::new(branch.to_string(), Some(agent.to_string()), note);

    match session_ops::create_session(request, &config) {
        Ok(session) => {
            tracing::info!(
                event = "ui.create_shard.completed",
                session_id = session.id,
                branch = session.branch
            );
            Ok(session)
        }
        Err(e) => {
            tracing::error!(
                event = "ui.create_shard.failed",
                branch = branch,
                agent = agent,
                error = %e
            );
            Err(e.to_string())
        }
    }
}

/// Refresh the list of sessions from disk.
///
/// Returns `(displays, error)` where `error` is `Some` if session loading failed.
pub fn refresh_sessions() -> (Vec<ShardDisplay>, Option<String>) {
    tracing::info!(event = "ui.refresh_sessions.started");

    match session_ops::list_sessions() {
        Ok(sessions) => {
            let displays = sessions
                .into_iter()
                .map(ShardDisplay::from_session)
                .collect();
            tracing::info!(event = "ui.refresh_sessions.completed");
            (displays, None)
        }
        Err(e) => {
            tracing::error!(event = "ui.refresh_sessions.failed", error = %e);
            (Vec::new(), Some(e.to_string()))
        }
    }
}

/// Destroy a shard by branch name.
///
/// Thin wrapper around shards-core's `destroy_session`, which handles
/// terminal cleanup, process termination, worktree removal, and session file deletion.
pub fn destroy_shard(branch: &str) -> Result<(), String> {
    tracing::info!(event = "ui.destroy_shard.started", branch = branch);

    match session_ops::destroy_session(branch, false) {
        Ok(()) => {
            tracing::info!(event = "ui.destroy_shard.completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            tracing::error!(event = "ui.destroy_shard.failed", branch = branch, error = %e);
            Err(e.to_string())
        }
    }
}

/// Open a new agent terminal in an existing shard (additive - doesn't close existing terminals).
///
/// Unlike relaunch, this does NOT close existing terminals - multiple agents can run in the same shard.
pub fn open_shard(branch: &str, agent: Option<String>) -> Result<Session, String> {
    tracing::info!(event = "ui.open_shard.started", branch = branch, agent = ?agent);

    match session_ops::open_session(branch, agent) {
        Ok(session) => {
            tracing::info!(
                event = "ui.open_shard.completed",
                branch = branch,
                process_id = session.process_id
            );
            Ok(session)
        }
        Err(e) => {
            tracing::error!(event = "ui.open_shard.failed", branch = branch, error = %e);
            Err(e.to_string())
        }
    }
}

/// Stop the agent process in a shard without destroying the shard.
///
/// The worktree and session file are preserved. The shard can be reopened with open_shard().
pub fn stop_shard(branch: &str) -> Result<(), String> {
    tracing::info!(event = "ui.stop_shard.started", branch = branch);

    match session_ops::stop_session(branch) {
        Ok(()) => {
            tracing::info!(event = "ui.stop_shard.completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            tracing::error!(event = "ui.stop_shard.failed", branch = branch, error = %e);
            Err(e.to_string())
        }
    }
}

/// Open agents in all stopped shards.
///
/// Returns (opened_count, errors) where errors contains operation errors with branch names.
pub fn open_all_stopped(displays: &[ShardDisplay]) -> (usize, Vec<OperationError>) {
    execute_bulk_operation(
        displays,
        ProcessStatus::Stopped,
        |branch| {
            session_ops::open_session(branch, None)
                .map(|_| ())
                .map_err(|e| e.to_string())
        },
        "ui.open_all_stopped",
    )
}

/// Stop all running shards.
///
/// Returns (stopped_count, errors) where errors contains operation errors with branch names.
pub fn stop_all_running(displays: &[ShardDisplay]) -> (usize, Vec<OperationError>) {
    execute_bulk_operation(
        displays,
        ProcessStatus::Running,
        |branch| session_ops::stop_session(branch).map_err(|e| e.to_string()),
        "ui.stop_all_running",
    )
}

/// Execute a bulk operation on shards with a specific status.
fn execute_bulk_operation(
    displays: &[ShardDisplay],
    target_status: ProcessStatus,
    operation: impl Fn(&str) -> Result<(), String>,
    event_prefix: &str,
) -> (usize, Vec<OperationError>) {
    tracing::info!(event = format!("{}.started", event_prefix));

    let targets: Vec<_> = displays
        .iter()
        .filter(|d| d.status == target_status)
        .collect();

    let mut success_count = 0;
    let mut errors = Vec::new();

    for shard_display in targets {
        let branch = &shard_display.session.branch;
        match operation(branch) {
            Ok(()) => {
                tracing::info!(
                    event = format!("{}.shard_completed", event_prefix),
                    branch = branch
                );
                success_count += 1;
            }
            Err(e) => {
                tracing::error!(
                    event = format!("{}.shard_failed", event_prefix),
                    branch = branch,
                    error = %e
                );
                errors.push(OperationError {
                    branch: branch.clone(),
                    message: e,
                });
            }
        }
    }

    tracing::info!(
        event = format!("{}.completed", event_prefix),
        succeeded = success_count,
        failed = errors.len()
    );

    (success_count, errors)
}

#[cfg(test)]
mod tests {
    use crate::state::{GitStatus, ProcessStatus, ShardDisplay};
    use shards_core::Session;
    use shards_core::sessions::types::SessionStatus;
    use std::path::PathBuf;

    /// Get branches of all stopped shards (for testing filtering logic).
    fn get_stopped_branches(displays: &[ShardDisplay]) -> Vec<String> {
        displays
            .iter()
            .filter(|d| d.status == ProcessStatus::Stopped)
            .map(|d| d.session.branch.clone())
            .collect()
    }

    /// Get branches of all running shards (for testing filtering logic).
    fn get_running_branches(displays: &[ShardDisplay]) -> Vec<String> {
        displays
            .iter()
            .filter(|d| d.status == ProcessStatus::Running)
            .map(|d| d.session.branch.clone())
            .collect()
    }

    fn make_session(id: &str, branch: &str) -> Session {
        Session {
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
        }
    }

    fn make_display(id: &str, branch: &str, status: ProcessStatus) -> ShardDisplay {
        ShardDisplay {
            session: make_session(id, branch),
            status,
            git_status: GitStatus::Unknown,
        }
    }

    // --- Filtering Logic Tests ---

    #[test]
    fn test_get_stopped_branches_only_returns_stopped() {
        let displays = vec![
            make_display("1", "stopped-1", ProcessStatus::Stopped),
            make_display("2", "running-1", ProcessStatus::Running),
            make_display("3", "stopped-2", ProcessStatus::Stopped),
            make_display("4", "running-2", ProcessStatus::Running),
        ];

        let stopped = get_stopped_branches(&displays);

        assert_eq!(stopped.len(), 2);
        assert!(stopped.contains(&"stopped-1".to_string()));
        assert!(stopped.contains(&"stopped-2".to_string()));
        assert!(!stopped.contains(&"running-1".to_string()));
        assert!(!stopped.contains(&"running-2".to_string()));
    }

    #[test]
    fn test_get_running_branches_only_returns_running() {
        let displays = vec![
            make_display("1", "stopped-1", ProcessStatus::Stopped),
            make_display("2", "running-1", ProcessStatus::Running),
            make_display("3", "stopped-2", ProcessStatus::Stopped),
            make_display("4", "running-2", ProcessStatus::Running),
        ];

        let running = get_running_branches(&displays);

        assert_eq!(running.len(), 2);
        assert!(running.contains(&"running-1".to_string()));
        assert!(running.contains(&"running-2".to_string()));
        assert!(!running.contains(&"stopped-1".to_string()));
        assert!(!running.contains(&"stopped-2".to_string()));
    }

    // --- Unknown Status Handling Tests ---

    #[test]
    fn test_get_stopped_branches_ignores_unknown_status() {
        let displays = vec![
            make_display("1", "stopped-1", ProcessStatus::Stopped),
            make_display("2", "unknown-1", ProcessStatus::Unknown),
            make_display("3", "running-1", ProcessStatus::Running),
            make_display("4", "unknown-2", ProcessStatus::Unknown),
        ];

        let stopped = get_stopped_branches(&displays);

        assert_eq!(stopped.len(), 1);
        assert_eq!(stopped[0], "stopped-1");
        // Unknown status shards should NOT be included
        assert!(!stopped.contains(&"unknown-1".to_string()));
        assert!(!stopped.contains(&"unknown-2".to_string()));
    }

    #[test]
    fn test_get_running_branches_ignores_unknown_status() {
        let displays = vec![
            make_display("1", "stopped-1", ProcessStatus::Stopped),
            make_display("2", "unknown-1", ProcessStatus::Unknown),
            make_display("3", "running-1", ProcessStatus::Running),
            make_display("4", "unknown-2", ProcessStatus::Unknown),
        ];

        let running = get_running_branches(&displays);

        assert_eq!(running.len(), 1);
        assert_eq!(running[0], "running-1");
        // Unknown status shards should NOT be included
        assert!(!running.contains(&"unknown-1".to_string()));
        assert!(!running.contains(&"unknown-2".to_string()));
    }

    // --- Empty Input Tests ---

    #[test]
    fn test_get_stopped_branches_empty_input() {
        let displays: Vec<ShardDisplay> = vec![];
        let stopped = get_stopped_branches(&displays);
        assert!(stopped.is_empty());
    }

    #[test]
    fn test_get_running_branches_empty_input() {
        let displays: Vec<ShardDisplay> = vec![];
        let running = get_running_branches(&displays);
        assert!(running.is_empty());
    }

    #[test]
    fn test_get_stopped_branches_no_stopped_shards() {
        let displays = vec![
            make_display("1", "running-1", ProcessStatus::Running),
            make_display("2", "running-2", ProcessStatus::Running),
        ];

        let stopped = get_stopped_branches(&displays);
        assert!(stopped.is_empty());
    }

    #[test]
    fn test_get_running_branches_no_running_shards() {
        let displays = vec![
            make_display("1", "stopped-1", ProcessStatus::Stopped),
            make_display("2", "stopped-2", ProcessStatus::Stopped),
        ];

        let running = get_running_branches(&displays);
        assert!(running.is_empty());
    }

    // --- All Same Status Tests ---

    #[test]
    fn test_get_stopped_branches_all_stopped() {
        let displays = vec![
            make_display("1", "branch-1", ProcessStatus::Stopped),
            make_display("2", "branch-2", ProcessStatus::Stopped),
            make_display("3", "branch-3", ProcessStatus::Stopped),
        ];

        let stopped = get_stopped_branches(&displays);

        assert_eq!(stopped.len(), 3);
        assert!(stopped.contains(&"branch-1".to_string()));
        assert!(stopped.contains(&"branch-2".to_string()));
        assert!(stopped.contains(&"branch-3".to_string()));
    }

    #[test]
    fn test_get_running_branches_all_running() {
        let displays = vec![
            make_display("1", "branch-1", ProcessStatus::Running),
            make_display("2", "branch-2", ProcessStatus::Running),
            make_display("3", "branch-3", ProcessStatus::Running),
        ];

        let running = get_running_branches(&displays);

        assert_eq!(running.len(), 3);
        assert!(running.contains(&"branch-1".to_string()));
        assert!(running.contains(&"branch-2".to_string()));
        assert!(running.contains(&"branch-3".to_string()));
    }

    // --- All Unknown Status Test ---

    #[test]
    fn test_get_stopped_branches_all_unknown() {
        let displays = vec![
            make_display("1", "unknown-1", ProcessStatus::Unknown),
            make_display("2", "unknown-2", ProcessStatus::Unknown),
        ];

        let stopped = get_stopped_branches(&displays);
        assert!(
            stopped.is_empty(),
            "Unknown status should not be treated as Stopped"
        );
    }

    #[test]
    fn test_get_running_branches_all_unknown() {
        let displays = vec![
            make_display("1", "unknown-1", ProcessStatus::Unknown),
            make_display("2", "unknown-2", ProcessStatus::Unknown),
        ];

        let running = get_running_branches(&displays);
        assert!(
            running.is_empty(),
            "Unknown status should not be treated as Running"
        );
    }
}
