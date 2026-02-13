//! Business logic handlers for kild-ui.
//!
//! This module contains functions that interact with kild-core
//! to perform operations like creating, destroying, relaunching, and listing kilds.

use std::path::PathBuf;

use kild_core::{Command, CoreStore, Event, KildConfig, OpenMode, Store, session_ops};

use kild_core::SessionInfo;

/// Load config and create a CoreStore instance.
///
/// Helper function to avoid duplicating config loading logic across action handlers.
fn create_store() -> Result<CoreStore, String> {
    let config = KildConfig::load_hierarchy().map_err(|e| {
        tracing::error!(event = "ui.config_load_failed", error = %e);
        format!("Failed to load config: {e}")
    })?;
    Ok(CoreStore::new(config))
}

/// Helper to execute a store command and map the result.
///
/// Handles the common pattern of logging start/completion/failure and converting errors to strings.
fn dispatch_command(command: Command, event_prefix: &str) -> Result<Vec<Event>, String> {
    let mut store = create_store()?;

    match store.dispatch(command) {
        Ok(events) => {
            tracing::info!(event = event_prefix, state = "completed");
            Ok(events)
        }
        Err(e) => {
            tracing::error!(event = event_prefix, state = "failed", error = %e);
            Err(e.to_string())
        }
    }
}

/// Create a new kild with the given branch name, agent, optional note, and optional project path.
///
/// When `project_path` is provided (UI context), detects project from that path.
/// When `None` (shouldn't happen in UI), falls back to current working directory detection.
///
/// Takes owned parameters so this function can be called from background threads.
/// Dispatches through `CoreStore` and returns the resulting events on success.
pub fn create_kild(
    branch: String,
    agent: String,
    note: Option<String>,
    project_path: Option<PathBuf>,
) -> Result<Vec<Event>, String> {
    tracing::info!(
        event = "ui.create_kild.started",
        branch = %branch,
        agent = %agent,
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

    // UI always creates with an agent — bare shell (AgentMode::BareShell) is CLI-only.
    dispatch_command(
        Command::CreateKild {
            branch,
            agent_mode: kild_core::AgentMode::Agent(agent),
            note,
            project_path,
        },
        "ui.create_kild",
    )
}

/// Refresh the list of sessions from disk.
///
/// Returns `(displays, error)` where `error` is `Some` if session loading failed.
pub fn refresh_sessions() -> (Vec<SessionInfo>, Option<String>) {
    tracing::info!(event = "ui.refresh_sessions.started");

    match session_ops::list_sessions() {
        Ok(sessions) => {
            let displays = sessions
                .into_iter()
                .map(SessionInfo::from_session)
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
/// Dispatches through `CoreStore` to route to kild-core's `destroy_session`, which handles
/// terminal cleanup, process termination, worktree removal, and session file deletion.
///
/// Takes owned parameters so this function can be called from background threads.
///
/// # Arguments
/// * `branch` - Branch name of the kild to destroy
/// * `force` - If true, bypasses git safety checks (e.g., uncommitted changes)
pub fn destroy_kild(branch: String, force: bool) -> Result<Vec<Event>, String> {
    tracing::info!(
        event = "ui.destroy_kild.started",
        branch = %branch,
        force = force
    );

    dispatch_command(Command::DestroyKild { branch, force }, "ui.destroy_kild")
}

/// Open a new agent terminal in an existing kild (additive - doesn't close existing terminals).
///
/// Unlike relaunch, this does NOT close existing terminals - multiple agents can run in the same kild.
/// Takes owned parameters so this function can be called from background threads.
/// Dispatches through `CoreStore` and returns the resulting events on success.
pub fn open_kild(branch: String, agent: Option<String>) -> Result<Vec<Event>, String> {
    tracing::info!(event = "ui.open_kild.started", branch = %branch, agent = ?agent);

    let mode = match agent {
        Some(a) => OpenMode::Agent(a),
        None => OpenMode::DefaultAgent,
    };

    dispatch_command(
        Command::OpenKild {
            branch,
            mode,
            runtime_mode: None,
            resume: false,
        },
        "ui.open_kild",
    )
}

/// Stop the agent process in a kild without destroying the kild.
///
/// Takes owned parameters so this function can be called from background threads.
/// Dispatches through `CoreStore` to route to kild-core's `stop_session`.
/// The worktree and session file are preserved. The kild can be reopened with open_kild().
pub fn stop_kild(branch: String) -> Result<Vec<Event>, String> {
    tracing::info!(event = "ui.stop_kild.started", branch = %branch);

    dispatch_command(Command::StopKild { branch }, "ui.stop_kild")
}

// --- Project Management Actions (dispatch-based) ---

/// Add a project via Store dispatch.
///
/// Validates the path, creates a Project, persists to disk, and returns events.
/// Path normalization (tilde expansion) should be done by the caller before invoking.
pub fn dispatch_add_project(path: PathBuf, name: Option<String>) -> Result<Vec<Event>, String> {
    tracing::info!(
        event = "ui.dispatch_add_project.started",
        path = %path.display()
    );

    dispatch_command(
        Command::AddProject { path, name },
        "ui.dispatch_add_project",
    )
}

/// Remove a project via Store dispatch.
pub fn dispatch_remove_project(path: PathBuf) -> Result<Vec<Event>, String> {
    tracing::info!(
        event = "ui.dispatch_remove_project.started",
        path = %path.display()
    );

    dispatch_command(
        Command::RemoveProject { path },
        "ui.dispatch_remove_project",
    )
}

/// Set the active project via Store dispatch.
///
/// Pass `None` to select "all projects" view.
pub fn dispatch_set_active_project(path: Option<PathBuf>) -> Result<Vec<Event>, String> {
    tracing::info!(
        event = "ui.dispatch_set_active_project.started",
        path = ?path
    );

    dispatch_command(
        Command::SelectProject { path },
        "ui.dispatch_set_active_project",
    )
}

#[cfg(test)]
mod tests {
    use kild_core::sessions::types::SessionStatus;
    use kild_core::{GitStatus, ProcessStatus, Session, SessionInfo};
    use std::path::PathBuf;

    /// Get branches of all stopped kilds (for testing filtering logic).
    fn get_stopped_branches(displays: &[SessionInfo]) -> Vec<String> {
        displays
            .iter()
            .filter(|d| d.process_status == ProcessStatus::Stopped)
            .map(|d| d.session.branch.clone())
            .collect()
    }

    /// Get branches of all running kilds (for testing filtering logic).
    fn get_running_branches(displays: &[SessionInfo]) -> Vec<String> {
        displays
            .iter()
            .filter(|d| d.process_status == ProcessStatus::Running)
            .map(|d| d.session.branch.clone())
            .collect()
    }

    fn make_session(id: &str, branch: &str) -> Session {
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
            None,
        )
    }

    fn make_display(id: &str, branch: &str, process_status: ProcessStatus) -> SessionInfo {
        SessionInfo {
            session: make_session(id, branch),
            process_status,
            git_status: GitStatus::Unknown,
            uncommitted_diff: None,
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
        let displays: Vec<SessionInfo> = vec![];
        let stopped = get_stopped_branches(&displays);
        assert!(stopped.is_empty());
    }

    #[test]
    fn test_get_running_branches_empty_input() {
        let displays: Vec<SessionInfo> = vec![];
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
    static EDITOR_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Helper to restore environment variable after test
    fn restore_env_var(key: &str, original: Option<String>) {
        // SAFETY: Caller holds appropriate lock to prevent concurrent access
        unsafe {
            match original {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    #[test]
    fn test_select_editor_uses_env_when_set() {
        let _lock = EDITOR_ENV_LOCK.lock().unwrap();

        let original = std::env::var("EDITOR").ok();

        // SAFETY: We hold EDITOR_ENV_LOCK to prevent concurrent access
        unsafe {
            std::env::set_var("EDITOR", "nvim");
        }
        let editor = super::select_editor();
        assert_eq!(editor, "nvim");

        restore_env_var("EDITOR", original);
    }

    #[test]
    fn test_select_editor_defaults_to_zed() {
        let _lock = EDITOR_ENV_LOCK.lock().unwrap();

        let original = std::env::var("EDITOR").ok();

        // SAFETY: We hold EDITOR_ENV_LOCK to prevent concurrent access
        unsafe {
            std::env::remove_var("EDITOR");
        }
        let editor = super::select_editor();
        assert_eq!(editor, "code");

        restore_env_var("EDITOR", original);
    }

    // --- Validation function tests ---

    #[test]
    fn test_derive_display_name_works_correctly() {
        use kild_core::projects::types::derive_display_name;

        let path = PathBuf::from("/Users/test/Projects/my-awesome-project");
        let name = derive_display_name(&path);
        assert_eq!(name, "my-awesome-project");
    }
}

/// Open a worktree path in the user's preferred editor.
///
/// Editor selection priority (GUI context - no CLI flag available):
/// 1. $EDITOR environment variable
/// 2. Default: "code" (VS Code)
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
                "Failed to open editor '{}': {}. Check that $EDITOR is set correctly or 'code' (VS Code) is installed.",
                editor, e
            ))
        }
    }
}

/// Determine which editor to use based on environment.
///
/// Priority:
/// 1. $EDITOR environment variable
/// 2. Default: "code" (VS Code)
fn select_editor() -> String {
    std::env::var("EDITOR").unwrap_or_else(|_| "code".to_string())
}
