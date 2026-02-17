use kild_paths::KildPaths;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::git;
use crate::git::get_worktree_status;
use crate::process::{delete_pid_file, get_pid_file_path};
use crate::sessions::{errors::SessionError, persistence, types::*};
use crate::terminal;

/// Clean up PID files for a session (best-effort).
///
/// Handles both multi-agent sessions (per-agent spawn ID PID files) and
/// legacy sessions (session-level PID file). Failures are logged at debug
/// level since PID file cleanup is best-effort.
pub(crate) fn cleanup_session_pid_files(
    session: &Session,
    kild_dir: &std::path::Path,
    operation: &str,
) {
    if !session.has_agents() {
        // Legacy session (pre-multi-agent) — attempt session-level PID file cleanup
        warn!(
            event = "core.session.pid_cleanup_no_agents",
            session_id = %session.id,
            operation = operation,
            "Session has no tracked agents, attempting session-level PID file cleanup"
        );
        let pid_file = get_pid_file_path(kild_dir, &session.id);
        match delete_pid_file(&pid_file) {
            Ok(()) => {
                debug!(
                    event = "core.session.pid_file_cleaned",
                    session_id = %session.id,
                    operation = operation,
                    pid_file = %pid_file.display()
                );
            }
            Err(e) => {
                debug!(
                    event = "core.session.pid_file_cleanup_failed",
                    session_id = %session.id,
                    operation = operation,
                    pid_file = %pid_file.display(),
                    error = %e
                );
            }
        }
        return;
    }

    for agent_proc in session.agents() {
        // Determine PID file key: use spawn_id if available, otherwise fall back to session ID
        let pid_key = if agent_proc.spawn_id().is_empty() {
            session.id.to_string() // Backward compat: old sessions without spawn_id
        } else {
            agent_proc.spawn_id().to_string()
        };
        let pid_file = get_pid_file_path(kild_dir, &pid_key);
        match delete_pid_file(&pid_file) {
            Ok(()) => {
                debug!(
                    event = "core.session.pid_file_cleaned",
                    session_id = %session.id,
                    operation = operation,
                    pid_file = %pid_file.display()
                );
            }
            Err(e) => {
                debug!(
                    event = "core.session.pid_file_cleanup_failed",
                    session_id = %session.id,
                    operation = operation,
                    pid_file = %pid_file.display(),
                    error = %e
                );
            }
        }
    }
}

/// Clean up Claude Code task list directory for a session.
///
/// Removes `~/.claude/tasks/<task_list_id>/` if it exists. Failures are logged
/// and printed as warnings but do not block session destruction.
pub fn cleanup_task_list(session_id: &str, task_list_id: &str, home_dir: &std::path::Path) {
    let task_dir = home_dir.join(".claude").join("tasks").join(task_list_id);
    if task_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&task_dir) {
            warn!(
                event = "core.session.task_list_cleanup_failed",
                session_id = session_id,
                task_list_id = task_list_id,
                path = %task_dir.display(),
                error = %e,
            );
            eprintln!(
                "Warning: Failed to remove task list at {}: {}",
                task_dir.display(),
                e
            );
        } else {
            info!(
                event = "core.session.task_list_cleanup_completed",
                session_id = session_id,
                task_list_id = task_list_id,
            );
        }
    }
}

/// Destroys a kild by removing its worktree, killing the process, and deleting the session file.
///
/// # Arguments
/// * `name` - Branch name or kild identifier
/// * `force` - If true, bypass git safety checks and force removal
///
/// # Force Mode Behavior
/// When `force` is false:
/// - Process kill failures block destruction
/// - Git refuses to remove worktree with uncommitted changes
///
/// When `force` is true:
/// - Process kill failures are logged but don't block destruction
/// - Worktree is force-deleted even with uncommitted changes (work will be lost)
pub fn destroy_session(name: &str, force: bool) -> Result<(), SessionError> {
    info!(
        event = "core.session.destroy_started",
        name = name,
        force = force
    );

    let config = Config::new();

    // 1. Find session by name (branch name)
    let session =
        persistence::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    info!(
        event = "core.session.destroy_found",
        session_id = %session.id,
        worktree_path = %session.worktree_path.display(),
        port_range_start = session.port_range_start,
        port_range_end = session.port_range_end,
        agent_count = session.agent_count()
    );

    // 2. Close all terminal windows and kill all processes
    {
        if !session.has_agents() {
            warn!(
                event = "core.session.destroy_no_agents",
                session_id = %session.id,
                branch = name,
                "Session has no tracked agents — skipping process/terminal cleanup"
            );
        }

        // Kill/stop all tracked agents — branch on daemon vs terminal
        let mut kill_errors: Vec<(u32, String)> = Vec::with_capacity(session.agent_count());
        for agent_proc in session.agents() {
            if let Some(daemon_sid) = agent_proc.daemon_session_id() {
                // Daemon-managed: destroy via IPC
                info!(
                    event = "core.session.destroy_daemon_session",
                    daemon_session_id = daemon_sid,
                    agent = agent_proc.agent()
                );
                if let Err(e) = crate::daemon::client::destroy_daemon_session(daemon_sid, force) {
                    warn!(
                        event = "core.session.destroy_daemon_failed_continue",
                        daemon_session_id = daemon_sid,
                        error = %e,
                        force = force,
                    );
                    // Don't add to kill_errors — daemon cleanup failure is non-fatal.
                    // The kild session file is being removed regardless.
                }

                // Close the attach terminal window (if tracked)
                if let (Some(terminal_type), Some(window_id)) =
                    (agent_proc.terminal_type(), agent_proc.terminal_window_id())
                {
                    info!(
                        event = "core.session.destroy_close_attach_window",
                        terminal_type = ?terminal_type,
                        agent = agent_proc.agent(),
                    );
                    terminal::handler::close_terminal(terminal_type, Some(window_id));
                }
            } else {
                // Terminal-managed: close window + kill process
                if let (Some(terminal_type), Some(window_id)) =
                    (agent_proc.terminal_type(), agent_proc.terminal_window_id())
                {
                    info!(
                        event = "core.session.destroy_close_terminal",
                        terminal_type = ?terminal_type,
                        agent = agent_proc.agent(),
                    );
                    terminal::handler::close_terminal(terminal_type, Some(window_id));
                }

                let Some(pid) = agent_proc.process_id() else {
                    continue;
                };

                info!(
                    event = "core.session.destroy_kill_started",
                    pid = pid,
                    agent = agent_proc.agent()
                );

                let result = crate::process::kill_process(
                    pid,
                    agent_proc.process_name(),
                    agent_proc.process_start_time(),
                );

                match result {
                    Ok(()) => {
                        info!(event = "core.session.destroy_kill_completed", pid = pid);
                    }
                    Err(crate::process::ProcessError::NotFound { .. }) => {
                        info!(event = "core.session.destroy_kill_already_dead", pid = pid);
                    }
                    Err(e) if force => {
                        warn!(
                            event = "core.session.destroy_kill_failed_force_continue",
                            pid = pid,
                            error = %e
                        );
                    }
                    Err(e) => {
                        kill_errors.push((pid, e.to_string()));
                    }
                }
            }
        }

        if !kill_errors.is_empty() && !force {
            for (pid, err) in &kill_errors {
                error!(
                    event = "core.session.destroy_kill_failed",
                    pid = pid,
                    error = %err
                );
            }

            let error_count = kill_errors.len();
            let (first_pid, first_msg) = {
                let (p, m) = kill_errors.first().unwrap();
                (*p, m.clone())
            };

            let message = if error_count == 1 {
                format!(
                    "Process still running. Kill it manually or use --force flag: {}",
                    first_msg
                )
            } else {
                let pids: String = kill_errors
                    .iter()
                    .map(|(p, _)| p.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{} processes still running (PIDs: {}). Kill them manually or use --force flag.",
                    error_count, pids
                )
            };

            return Err(SessionError::ProcessKillFailed {
                pid: first_pid,
                message,
            });
        }
    }

    // 3a. Sweep for untracked daemon sessions (e.g., UI-created shells)
    //
    // UI-created daemon sessions use the naming pattern `{kild_id}_ui_shell_{counter}`
    // and are not tracked in the session file. Query the daemon for all sessions
    // with a matching prefix and destroy any remaining ones.
    {
        let prefix = format!("{}_ui_shell_", session.id);
        match crate::daemon::client::list_daemon_sessions() {
            Ok(sessions) => {
                let ui_sessions: Vec<_> = sessions
                    .iter()
                    .filter(|s| s.id.starts_with(&prefix))
                    .collect();

                if !ui_sessions.is_empty() {
                    info!(
                        event = "core.session.destroy_ui_sessions_sweep_started",
                        session_id = %session.id,
                        count = ui_sessions.len()
                    );
                }

                for daemon_session in ui_sessions {
                    info!(
                        event = "core.session.destroy_ui_session",
                        daemon_session_id = %daemon_session.id,
                    );
                    if let Err(e) =
                        crate::daemon::client::destroy_daemon_session(&daemon_session.id, true)
                    {
                        warn!(
                            event = "core.session.destroy_ui_session_failed",
                            daemon_session_id = %daemon_session.id,
                            error = %e,
                        );
                        eprintln!(
                            "Warning: Failed to clean up UI terminal session {}: {}",
                            daemon_session.id, e
                        );
                    }
                }
            }
            Err(crate::daemon::client::DaemonClientError::NotRunning { .. }) => {
                // Daemon not running — no UI sessions to clean up
                debug!(
                    event = "core.session.destroy_ui_sessions_sweep_skipped",
                    session_id = %session.id,
                    reason = "daemon_not_running"
                );
            }
            Err(e) => {
                warn!(
                    event = "core.session.destroy_ui_sessions_sweep_failed",
                    session_id = %session.id,
                    error = %e,
                    "Could not query daemon for orphaned UI sessions"
                );
            }
        }
    }

    // 3b. Clean up tmux shim state and destroy child shim panes
    if let Ok(paths) = KildPaths::resolve() {
        let shim_dir = paths.shim_session_dir(&session.id);
        if shim_dir.exists() {
            // Destroy any child shim panes that may still be running
            let panes_path = paths.shim_panes_file(&session.id);
            match std::fs::read_to_string(&panes_path) {
                Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(registry) => {
                        if let Some(panes) = registry.get("panes").and_then(|p| p.as_object()) {
                            for (pane_id, entry) in panes {
                                if pane_id == "%0" {
                                    continue; // Skip the parent pane (already destroyed above)
                                }
                                if let Some(child_sid) =
                                    entry.get("daemon_session_id").and_then(|s| s.as_str())
                                {
                                    info!(
                                        event = "core.session.destroy_shim_child",
                                        pane_id = pane_id,
                                        daemon_session_id = child_sid
                                    );
                                    if let Err(e) = crate::daemon::client::destroy_daemon_session(
                                        child_sid, true,
                                    ) {
                                        error!(
                                            event = "core.session.destroy_shim_child_failed",
                                            pane_id = pane_id,
                                            daemon_session_id = child_sid,
                                            error = %e,
                                        );
                                        eprintln!(
                                            "Warning: Failed to destroy agent team PTY {}: {}",
                                            pane_id, e
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            event = "core.session.shim_registry_parse_failed",
                            session_id = %session.id,
                            path = %panes_path.display(),
                            error = %e,
                        );
                        eprintln!(
                            "Warning: Could not parse agent team state at {} — child PTYs may be orphaned: {}",
                            panes_path.display(),
                            e
                        );
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // No panes.json means no child panes to clean up
                }
                Err(e) => {
                    error!(
                        event = "core.session.shim_registry_read_failed",
                        session_id = %session.id,
                        path = %panes_path.display(),
                        error = %e,
                    );
                    eprintln!(
                        "Warning: Could not read agent team state at {} — child PTYs may be orphaned: {}",
                        panes_path.display(),
                        e
                    );
                }
            }

            if let Err(e) = std::fs::remove_dir_all(&shim_dir) {
                error!(
                    event = "core.session.shim_cleanup_failed",
                    session_id = %session.id,
                    path = %shim_dir.display(),
                    error = %e,
                );
                eprintln!(
                    "Warning: Failed to remove agent team state at {}: {}",
                    shim_dir.display(),
                    e
                );
            } else {
                info!(
                    event = "core.session.shim_cleanup_completed",
                    session_id = %session.id
                );
            }
        }
    } else {
        warn!(
            event = "core.session.shim_cleanup_skipped",
            session_id = %session.id,
            "HOME not set, skipping shim cleanup"
        );
    }

    // 3c. Clean up Claude Code task list directory
    if let Some(task_list_id) = &session.task_list_id
        && let Some(home) = dirs::home_dir()
    {
        cleanup_task_list(&session.id, task_list_id, &home);
    }

    // 4. Resolve main repo path before worktree removal (needed for branch cleanup)
    let main_repo_path = git::removal::find_main_repo_root(&session.worktree_path);

    // 5. Remove git worktree
    if force {
        info!(
            event = "core.session.destroy_worktree_force",
            worktree = %session.worktree_path.display()
        );
        git::removal::remove_worktree_force(&session.worktree_path)
            .map_err(|e| SessionError::GitError { source: e })?;
    } else {
        git::removal::remove_worktree_by_path(&session.worktree_path)
            .map_err(|e| SessionError::GitError { source: e })?;
    }

    info!(
        event = "core.session.destroy_worktree_removed",
        session_id = %session.id,
        worktree_path = %session.worktree_path.display()
    );

    // 6. Delete local kild branch (best-effort, don't block destroy)
    if let Some(repo_path) = &main_repo_path {
        let kild_branch = git::naming::kild_branch_name(&session.branch);
        git::removal::delete_branch_if_exists(repo_path, &kild_branch);
    }

    // 7. Clean up PID files (best-effort, don't fail if missing)
    cleanup_session_pid_files(&session, config.kild_dir(), "destroy");

    // 8. Remove sidecar files (best-effort)
    persistence::remove_agent_status_file(&config.sessions_dir(), &session.id);
    persistence::remove_pr_info_file(&config.sessions_dir(), &session.id);

    // 9. Remove session file (automatically frees port range)
    persistence::remove_session_file(&config.sessions_dir(), &session.id)?;

    info!(
        event = "core.session.port_deallocated",
        session_id = %session.id,
        port_range_start = session.port_range_start,
        port_range_end = session.port_range_end
    );

    info!(
        event = "core.session.destroy_completed",
        session_id = %session.id,
        name = name
    );

    Ok(())
}

/// Check if the git repository at the given path has any remote configured.
///
/// Uses git2 to enumerate remotes. Returns false on any error (graceful degradation).
pub fn has_remote_configured(worktree_path: &std::path::Path) -> bool {
    match git2::Repository::open(worktree_path) {
        Ok(repo) => match repo.remotes() {
            Ok(remotes) => !remotes.is_empty(),
            Err(e) => {
                debug!(
                    event = "core.session.remote_check_failed",
                    path = %worktree_path.display(),
                    error = %e
                );
                false
            }
        },
        Err(e) => {
            debug!(
                event = "core.session.remote_check_repo_open_failed",
                path = %worktree_path.display(),
                error = %e
            );
            false
        }
    }
}

/// Get safety information before destroying a kild.
///
/// Gathers information about:
/// - Uncommitted changes in the worktree
/// - Unpushed commits
/// - Whether a remote branch exists
/// - Whether a PR exists (optional, requires gh CLI)
///
/// # Conservative Fallback
///
/// If git status checks fail, the fallback is conservative (assumes dirty) to
/// prevent accidental data loss. Check `git_status.status_check_failed` to
/// detect this condition and show appropriate warnings.
///
/// PR checks uses the `kild/<branch>` naming convention since KILD creates
/// branches with this prefix.
///
/// # Arguments
/// * `name` - Branch name or kild identifier (without the `kild/` prefix)
///
/// # Returns
/// * `Ok(DestroySafetyInfo)` - Safety information (always succeeds if session found)
/// * `Err(SessionError::NotFound)` - Session doesn't exist
pub fn get_destroy_safety_info(name: &str) -> Result<DestroySafetyInfo, SessionError> {
    info!(event = "core.session.safety_check_started", name = name);

    let config = Config::new();

    // 1. Find session by name (branch name)
    let session =
        persistence::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    let kild_branch = git::kild_branch_name(name);

    // 2. Get git worktree status (conservative fallback on failure)
    let git_status = if session.worktree_path.exists() {
        match get_worktree_status(&session.worktree_path) {
            Ok(status) => {
                debug!(
                    event = "core.session.safety_check_git_status",
                    has_uncommitted = status.has_uncommitted_changes,
                    unpushed_count = status.unpushed_commit_count,
                    has_remote = status.has_remote_branch,
                    status_check_failed = status.status_check_failed
                );
                status
            }
            Err(e) => {
                warn!(
                    event = "core.session.safety_check_git_failed",
                    name = name,
                    error = %e,
                    "Failed to open repository - assuming dirty to be safe"
                );
                // Conservative fallback: assume dirty
                crate::git::types::WorktreeStatus {
                    has_uncommitted_changes: true,
                    status_check_failed: true,
                    ..Default::default()
                }
            }
        }
    } else {
        warn!(
            event = "core.session.safety_check_worktree_missing",
            name = name,
            path = %session.worktree_path.display(),
            "Worktree missing - assuming dirty to be safe"
        );
        // Conservative fallback: assume dirty
        crate::git::types::WorktreeStatus {
            has_uncommitted_changes: true,
            status_check_failed: true,
            ..Default::default()
        }
    };

    // 3. Check if PR exists (best-effort, requires forge CLI)
    // Skip PR check for repos without a remote to avoid false "No PR found" warnings
    let forge_override = crate::config::KildConfig::load_hierarchy()
        .inspect_err(|e| {
            debug!(
                event = "core.session.config_load_failed",
                error = %e,
                "Could not load config for forge override — falling back to auto-detection"
            );
        })
        .ok()
        .and_then(|c| c.git.forge());
    let pr_status = if has_remote_configured(&session.worktree_path) {
        crate::forge::get_forge_backend(&session.worktree_path, forge_override)
            .map(|backend| backend.check_pr_exists(&session.worktree_path, &kild_branch))
            .unwrap_or(PrCheckResult::Unavailable)
    } else {
        debug!(
            event = "core.session.safety_check_no_remote",
            branch = kild_branch,
            "No remote configured — skipping PR check"
        );
        PrCheckResult::Unavailable
    };
    debug!(
        event = "core.session.safety_check_pr",
        branch = kild_branch,
        pr_status = ?pr_status
    );

    let safety_info = DestroySafetyInfo {
        git_status,
        pr_status,
    };

    info!(
        event = "core.session.safety_check_completed",
        name = name,
        should_block = safety_info.should_block(),
        has_warnings = safety_info.has_warnings(),
        status_check_failed = safety_info.git_status.status_check_failed
    );

    Ok(safety_info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_destroy_session_not_found() {
        let result = destroy_session("non-existent", false);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    #[test]
    fn test_uncommitted_changes_error_is_user_error() {
        use crate::errors::KildError;
        let error = SessionError::UncommittedChanges {
            name: "test-branch".to_string(),
        };
        assert!(error.is_user_error());
        assert_eq!(error.error_code(), "SESSION_UNCOMMITTED_CHANGES");
        assert!(error.to_string().contains("kild destroy --force"));
    }

    #[test]
    fn test_complete_blocks_on_uncommitted_via_safety_info() {
        // Verify that DestroySafetyInfo with uncommitted changes would block complete.
        use crate::git::types::WorktreeStatus;

        let dirty = DestroySafetyInfo {
            git_status: WorktreeStatus {
                has_uncommitted_changes: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(dirty.should_block());

        let clean = DestroySafetyInfo {
            git_status: WorktreeStatus {
                has_uncommitted_changes: false,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(!clean.should_block());
    }

    #[test]
    fn test_cleanup_task_list_removes_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let task_list_id = "tl_abc123";
        let task_dir = tmp.path().join(".claude").join("tasks").join(task_list_id);
        std::fs::create_dir_all(&task_dir).unwrap();
        std::fs::write(task_dir.join("task.json"), r#"{"id":"1"}"#).unwrap();

        assert!(task_dir.exists());
        cleanup_task_list("session-1", task_list_id, tmp.path());
        assert!(!task_dir.exists());
    }

    #[test]
    fn test_cleanup_task_list_handles_nonexistent_directory() {
        let tmp = tempfile::tempdir().unwrap();
        // No task directory created — should not panic or error
        cleanup_task_list("session-1", "tl_nonexistent", tmp.path());
    }

    #[test]
    fn test_destroy_session_force_not_found() {
        let result = destroy_session("non-existent", true);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    #[test]
    fn test_destroy_force_vs_non_force_both_return_not_found() {
        let result_non_force = destroy_session("test-force-behavior", false);
        assert!(result_non_force.is_err());
        assert!(matches!(
            result_non_force.unwrap_err(),
            SessionError::NotFound { .. }
        ));

        let result_force = destroy_session("test-force-behavior", true);
        assert!(result_force.is_err());
        assert!(matches!(
            result_force.unwrap_err(),
            SessionError::NotFound { .. }
        ));
    }

    #[test]
    fn test_cleanup_task_list_skipped_when_no_task_list_id() {
        // When session.task_list_id is None, cleanup_task_list is never called.
        // Verify the guard logic by simulating what destroy_session does.
        let task_list_id: Option<String> = None;
        let tmp = tempfile::tempdir().unwrap();

        // Create a directory that should NOT be removed
        let decoy_dir = tmp.path().join(".claude").join("tasks").join("decoy");
        std::fs::create_dir_all(&decoy_dir).unwrap();

        if let Some(id) = &task_list_id {
            cleanup_task_list("session-1", id, tmp.path());
        }

        // Decoy directory must still exist — cleanup was never called
        assert!(decoy_dir.exists());
    }

    #[test]
    fn test_cleanup_task_list_with_nested_files() {
        let tmp = tempfile::tempdir().unwrap();
        let task_list_id = "tl_nested_test";
        let task_dir = tmp.path().join(".claude").join("tasks").join(task_list_id);

        let sub_dir = task_dir.join("subtasks");
        std::fs::create_dir_all(&sub_dir).unwrap();
        std::fs::write(task_dir.join("task1.json"), r#"{"id":"1"}"#).unwrap();
        std::fs::write(task_dir.join("task2.json"), r#"{"id":"2"}"#).unwrap();
        std::fs::write(sub_dir.join("nested.json"), r#"{"id":"3"}"#).unwrap();

        assert!(task_dir.exists());
        assert!(sub_dir.exists());

        cleanup_task_list("session-nested", task_list_id, tmp.path());

        assert!(!task_dir.exists());
    }

    #[test]
    fn test_destroy_safety_info_default_does_not_block() {
        let info = DestroySafetyInfo::default();
        assert!(!info.should_block());
    }

    #[test]
    fn test_destroy_safety_info_fully_clean_no_warnings() {
        use crate::git::types::WorktreeStatus;

        let info = DestroySafetyInfo {
            git_status: WorktreeStatus {
                has_uncommitted_changes: false,
                unpushed_commit_count: 0,
                has_remote_branch: true,
                status_check_failed: false,
                ..Default::default()
            },
            pr_status: PrCheckResult::Exists,
        };
        assert!(!info.should_block());
        assert!(!info.has_warnings());
        assert!(info.warning_messages().is_empty());
    }

    #[test]
    fn test_has_warnings_each_condition_independently() {
        use crate::git::types::WorktreeStatus;

        let uncommitted = DestroySafetyInfo {
            git_status: WorktreeStatus {
                has_uncommitted_changes: true,
                has_remote_branch: true,
                ..Default::default()
            },
            pr_status: PrCheckResult::Exists,
        };
        assert!(uncommitted.has_warnings());

        let unpushed = DestroySafetyInfo {
            git_status: WorktreeStatus {
                unpushed_commit_count: 3,
                has_remote_branch: true,
                ..Default::default()
            },
            pr_status: PrCheckResult::Exists,
        };
        assert!(unpushed.has_warnings());

        let no_remote = DestroySafetyInfo {
            git_status: WorktreeStatus {
                has_remote_branch: false,
                ..Default::default()
            },
            pr_status: PrCheckResult::Exists,
        };
        assert!(no_remote.has_warnings());

        let no_pr = DestroySafetyInfo {
            git_status: WorktreeStatus {
                has_remote_branch: true,
                ..Default::default()
            },
            pr_status: PrCheckResult::NotFound,
        };
        assert!(no_pr.has_warnings());

        let status_failed = DestroySafetyInfo {
            git_status: WorktreeStatus {
                status_check_failed: true,
                has_remote_branch: true,
                ..Default::default()
            },
            pr_status: PrCheckResult::Exists,
        };
        assert!(status_failed.has_warnings());
    }

    #[test]
    fn test_warning_messages_severity_order() {
        use crate::git::types::WorktreeStatus;

        let info = DestroySafetyInfo {
            git_status: WorktreeStatus {
                has_uncommitted_changes: true,
                unpushed_commit_count: 2,
                has_remote_branch: true,
                status_check_failed: true,
                ..Default::default()
            },
            pr_status: PrCheckResult::NotFound,
        };

        let msgs = info.warning_messages();
        assert!(msgs.len() >= 3);
        assert!(msgs[0].contains("Git status check failed"));
        assert!(msgs[1].contains("unpushed"));
        assert!(msgs.last().unwrap().contains("No PR found"));
    }
}
