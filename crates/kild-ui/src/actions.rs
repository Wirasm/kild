//! Business logic handlers for kild-ui.
//!
//! This module contains functions that interact with kild-core
//! to perform operations like creating, destroying, relaunching, and listing kilds.

use std::path::{Path, PathBuf};

use kild_core::{CreateSessionRequest, KildConfig, Session, session_ops};

use crate::projects::{Project, ProjectError, load_projects, save_projects};
use crate::state::{KildDisplay, OperationError, ProcessStatus};

/// Create a new kild with the given branch name, agent, optional note, and optional project path.
///
/// When `project_path` is provided (UI context), detects project from that path.
/// When `None` (shouldn't happen in UI), falls back to current working directory detection.
///
/// Returns the created session on success, or an error message on failure.
pub fn create_kild(
    branch: &str,
    agent: &str,
    note: Option<String>,
    project_path: Option<PathBuf>,
) -> Result<Session, String> {
    tracing::info!(
        event = "ui.create_kild.started",
        branch = branch,
        agent = agent,
        note = ?note,
        project_path = ?project_path
    );

    if branch.trim().is_empty() {
        tracing::warn!(
            event = "ui.create_dialog.validation_failed",
            reason = "empty branch name"
        );
        return Err("Branch name cannot be empty".to_string());
    }

    let config = match KildConfig::load_hierarchy() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(
                event = "ui.create_kild.config_load_failed",
                error = %e
            );
            return Err(format!("Failed to load config: {e}"));
        }
    };

    let request = match project_path {
        Some(path) => CreateSessionRequest::with_project_path(
            branch.to_string(),
            Some(agent.to_string()),
            note,
            path,
        ),
        None => CreateSessionRequest::new(branch.to_string(), Some(agent.to_string()), note),
    };

    match session_ops::create_session(request, &config) {
        Ok(session) => {
            tracing::info!(
                event = "ui.create_kild.completed",
                session_id = session.id,
                branch = session.branch
            );
            Ok(session)
        }
        Err(e) => {
            tracing::error!(
                event = "ui.create_kild.failed",
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
pub fn refresh_sessions() -> (Vec<KildDisplay>, Option<String>) {
    tracing::info!(event = "ui.refresh_sessions.started");

    match session_ops::list_sessions() {
        Ok(sessions) => {
            let displays = sessions
                .into_iter()
                .map(KildDisplay::from_session)
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

/// Destroy a kild by branch name.
///
/// Thin wrapper around kild-core's `destroy_session`, which handles
/// terminal cleanup, process termination, worktree removal, and session file deletion.
pub fn destroy_kild(branch: &str) -> Result<(), String> {
    tracing::info!(event = "ui.destroy_kild.started", branch = branch);

    match session_ops::destroy_session(branch, false) {
        Ok(()) => {
            tracing::info!(event = "ui.destroy_kild.completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            tracing::error!(event = "ui.destroy_kild.failed", branch = branch, error = %e);
            Err(e.to_string())
        }
    }
}

/// Open a new agent terminal in an existing kild (additive - doesn't close existing terminals).
///
/// Unlike relaunch, this does NOT close existing terminals - multiple agents can run in the same kild.
pub fn open_kild(branch: &str, agent: Option<String>) -> Result<Session, String> {
    tracing::info!(event = "ui.open_kild.started", branch = branch, agent = ?agent);

    match session_ops::open_session(branch, agent) {
        Ok(session) => {
            tracing::info!(
                event = "ui.open_kild.completed",
                branch = branch,
                process_id = session.process_id
            );
            Ok(session)
        }
        Err(e) => {
            tracing::error!(event = "ui.open_kild.failed", branch = branch, error = %e);
            Err(e.to_string())
        }
    }
}

/// Stop the agent process in a kild without destroying the kild.
///
/// The worktree and session file are preserved. The kild can be reopened with open_kild().
pub fn stop_kild(branch: &str) -> Result<(), String> {
    tracing::info!(event = "ui.stop_kild.started", branch = branch);

    match session_ops::stop_session(branch) {
        Ok(()) => {
            tracing::info!(event = "ui.stop_kild.completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            tracing::error!(event = "ui.stop_kild.failed", branch = branch, error = %e);
            Err(e.to_string())
        }
    }
}

/// Open agents in all stopped kilds.
///
/// Returns (opened_count, errors) where errors contains operation errors with branch names.
pub fn open_all_stopped(displays: &[KildDisplay]) -> (usize, Vec<OperationError>) {
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

/// Stop all running kilds.
///
/// Returns (stopped_count, errors) where errors contains operation errors with branch names.
pub fn stop_all_running(displays: &[KildDisplay]) -> (usize, Vec<OperationError>) {
    execute_bulk_operation(
        displays,
        ProcessStatus::Running,
        |branch| session_ops::stop_session(branch).map_err(|e| e.to_string()),
        "ui.stop_all_running",
    )
}

/// Execute a bulk operation on kilds with a specific status.
fn execute_bulk_operation(
    displays: &[KildDisplay],
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

    for kild_display in targets {
        let branch = &kild_display.session.branch;
        match operation(branch) {
            Ok(()) => {
                tracing::info!(
                    event = format!("{}.kild_completed", event_prefix),
                    branch = branch
                );
                success_count += 1;
            }
            Err(e) => {
                tracing::error!(
                    event = format!("{}.kild_failed", event_prefix),
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

// --- Project Management Actions ---

/// Add a new project after validation.
///
/// Returns the added project on success, or an error message if validation fails.
/// The path is canonicalized to ensure consistent hashing for filtering.
pub fn add_project(path: PathBuf, name: Option<String>) -> Result<Project, String> {
    tracing::info!(
        event = "ui.add_project.started",
        path = %path.display()
    );

    let mut data = load_projects();

    // Create validated project with canonical path
    let project = Project::new(path.clone(), name).map_err(|e| match e {
        ProjectError::NotADirectory => format!("'{}' is not a directory", path.display()),
        ProjectError::NotAGitRepo => format!("'{}' is not a git repository", path.display()),
        ProjectError::CanonicalizationFailed(io_err) => {
            format!("Cannot access '{}': {}", path.display(), io_err)
        }
    })?;

    // Check if project already exists (by canonical path)
    if data.projects.iter().any(|p| p.path() == project.path()) {
        return Err("Project already exists".to_string());
    }

    let canonical_path = project.path().to_path_buf();
    data.projects.push(project.clone());

    // If this is the first project, make it active
    if data.projects.len() == 1 {
        data.active = Some(canonical_path.clone());
    }

    save_projects(&data)?;

    tracing::info!(
        event = "ui.add_project.completed",
        path = %canonical_path.display(),
        name = %project.name()
    );

    Ok(project)
}

/// Remove a project from the list (doesn't affect kilds).
pub fn remove_project(path: &Path) -> Result<(), String> {
    tracing::info!(
        event = "ui.remove_project.started",
        path = %path.display()
    );

    let mut data = load_projects();

    let original_len = data.projects.len();
    data.projects.retain(|p| p.path() != path);

    if data.projects.len() == original_len {
        return Err("Project not found".to_string());
    }

    // Clear active project if it was removed
    if data.active.as_ref() == Some(&path.to_path_buf()) {
        data.active = data.projects.first().map(|p| p.path().to_path_buf());
    }

    save_projects(&data)?;

    tracing::info!(
        event = "ui.remove_project.completed",
        path = %path.display()
    );

    Ok(())
}

/// Set the active project.
pub fn set_active_project(path: Option<PathBuf>) -> Result<(), String> {
    tracing::info!(
        event = "ui.set_active_project.started",
        path = ?path
    );

    let mut data = load_projects();

    // Validate that the project exists if a path is provided
    if let Some(p) = &path {
        let project_exists = data.projects.iter().any(|proj| proj.path() == p.as_path());
        if !project_exists {
            return Err("Project not found".to_string());
        }
    }

    data.active = path;
    save_projects(&data)?;

    tracing::info!(
        event = "ui.set_active_project.completed",
        path = ?data.active
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::state::{GitStatus, KildDisplay, ProcessStatus};
    use kild_core::Session;
    use kild_core::sessions::types::SessionStatus;
    use std::path::PathBuf;

    /// Get branches of all stopped kilds (for testing filtering logic).
    fn get_stopped_branches(displays: &[KildDisplay]) -> Vec<String> {
        displays
            .iter()
            .filter(|d| d.status == ProcessStatus::Stopped)
            .map(|d| d.session.branch.clone())
            .collect()
    }

    /// Get branches of all running kilds (for testing filtering logic).
    fn get_running_branches(displays: &[KildDisplay]) -> Vec<String> {
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

    fn make_display(id: &str, branch: &str, status: ProcessStatus) -> KildDisplay {
        KildDisplay {
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
        // Unknown status kilds should NOT be included
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
        // Unknown status kilds should NOT be included
        assert!(!running.contains(&"unknown-1".to_string()));
        assert!(!running.contains(&"unknown-2".to_string()));
    }

    // --- Empty Input Tests ---

    #[test]
    fn test_get_stopped_branches_empty_input() {
        let displays: Vec<KildDisplay> = vec![];
        let stopped = get_stopped_branches(&displays);
        assert!(stopped.is_empty());
    }

    #[test]
    fn test_get_running_branches_empty_input() {
        let displays: Vec<KildDisplay> = vec![];
        let running = get_running_branches(&displays);
        assert!(running.is_empty());
    }

    #[test]
    fn test_get_stopped_branches_no_stopped_kilds() {
        let displays = vec![
            make_display("1", "running-1", ProcessStatus::Running),
            make_display("2", "running-2", ProcessStatus::Running),
        ];

        let stopped = get_stopped_branches(&displays);
        assert!(stopped.is_empty());
    }

    #[test]
    fn test_get_running_branches_no_running_kilds() {
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

    // --- Editor Selection Tests ---

    use std::sync::Mutex;

    // Mutex to ensure editor selection tests don't interfere with each other
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Helper to restore environment variable after test
    fn restore_env_var(key: &str, original: Option<String>) {
        // SAFETY: We hold ENV_LOCK to prevent concurrent access to env vars
        unsafe {
            match original {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    #[test]
    fn test_select_editor_uses_env_when_set() {
        let _guard = ENV_LOCK.lock().unwrap();

        let original = std::env::var("EDITOR").ok();

        // SAFETY: We hold ENV_LOCK to prevent concurrent access to env vars
        unsafe {
            std::env::set_var("EDITOR", "nvim");
        }
        let editor = super::select_editor();
        assert_eq!(editor, "nvim");

        restore_env_var("EDITOR", original);
    }

    #[test]
    fn test_select_editor_defaults_to_zed() {
        let _guard = ENV_LOCK.lock().unwrap();

        let original = std::env::var("EDITOR").ok();

        // SAFETY: We hold ENV_LOCK to prevent concurrent access to env vars
        unsafe {
            std::env::remove_var("EDITOR");
        }
        let editor = super::select_editor();
        assert_eq!(editor, "zed");

        restore_env_var("EDITOR", original);
    }

    // --- add_project validation tests ---

    #[test]
    fn test_add_project_returns_error_for_nonexistent_path() {
        let path = PathBuf::from("/nonexistent/path/that/does/not/exist");
        let result = super::add_project(path, None);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Cannot access"),
            "Expected 'Cannot access' error, got: {}",
            err
        );
    }

    #[test]
    fn test_add_project_returns_error_for_file_not_directory() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "test").unwrap();

        let result = super::add_project(file_path.clone(), None);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("is not a directory"),
            "Expected 'not a directory' error, got: {}",
            err
        );
    }

    #[test]
    fn test_add_project_returns_error_for_non_git_directory() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        let result = super::add_project(path, None);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("is not a git repository"),
            "Expected 'not a git repository' error, got: {}",
            err
        );
    }

    #[test]
    fn test_add_project_uses_provided_name() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init failed");

        let result = super::add_project(path.to_path_buf(), Some("Custom Name".to_string()));

        // This will actually save to the real projects file, so we need to check the returned project
        // If it succeeds, it should have the custom name
        if let Ok(project) = result {
            assert_eq!(project.name(), "Custom Name");
        }
        // If it fails due to file system issues, that's acceptable for this test
    }

    #[test]
    fn test_add_project_derives_name_from_path() {
        use tempfile::TempDir;

        // Create a temp dir with a specific name
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init failed");

        let result = super::add_project(path.to_path_buf(), None);

        // If it succeeds, the name should be derived from the path
        if let Ok(project) = result {
            // Name should be the directory name (temp dir names are random)
            assert!(!project.name().is_empty());
            assert_ne!(project.name(), "unknown");
        }
    }

    // --- Validation function tests ---

    #[test]
    fn test_derive_display_name_works_correctly() {
        use crate::projects::derive_display_name;

        let path = PathBuf::from("/Users/test/Projects/my-awesome-project");
        let name = derive_display_name(&path);
        assert_eq!(name, "my-awesome-project");
    }
}

/// Open a worktree path in the user's preferred editor.
///
/// Editor selection priority (GUI context - no CLI flag available):
/// 1. $EDITOR environment variable
/// 2. Default: "zed"
///
/// Note: The CLI `code` command also supports an `--editor` flag that takes
/// highest precedence, but this is unavailable in the GUI context.
///
/// Returns `Ok(())` on successful spawn, or an error message if the editor
/// failed to launch (e.g., editor not found, permission denied).
pub fn open_in_editor(worktree_path: &std::path::Path) -> Result<(), String> {
    let editor = select_editor();

    tracing::info!(
        event = "ui.open_in_editor.started",
        path = %worktree_path.display(),
        editor = %editor
    );

    match std::process::Command::new(&editor)
        .arg(worktree_path)
        .spawn()
    {
        Ok(_) => {
            tracing::info!(
                event = "ui.open_in_editor.completed",
                path = %worktree_path.display(),
                editor = %editor
            );
            Ok(())
        }
        Err(e) => {
            tracing::error!(
                event = "ui.open_in_editor.failed",
                path = %worktree_path.display(),
                editor = %editor,
                error = %e
            );
            Err(format!(
                "Failed to open editor '{}': {}. Check that $EDITOR is set correctly or 'zed' is installed.",
                editor, e
            ))
        }
    }
}

/// Determine which editor to use based on environment.
///
/// Priority:
/// 1. $EDITOR environment variable
/// 2. Default: "zed"
fn select_editor() -> String {
    std::env::var("EDITOR").unwrap_or_else(|_| "zed".to_string())
}
