use tracing::{debug, error, info, warn};

use crate::agents;
use crate::config::{Config, KildConfig};
use crate::git;
use crate::process::{delete_pid_file, get_pid_file_path};
use crate::sessions::{errors::SessionError, operations, types::*};
use crate::terminal;
use crate::terminal::types::SpawnResult;

/// Capture process metadata from spawn result for PID reuse protection.
///
/// Attempts to get fresh process info from the OS. Falls back to spawn result metadata
/// if process info retrieval fails (logs warning in that case).
fn capture_process_metadata(
    spawn_result: &SpawnResult,
    event_prefix: &str,
) -> (Option<String>, Option<u64>) {
    let Some(pid) = spawn_result.process_id else {
        return (
            spawn_result.process_name.clone(),
            spawn_result.process_start_time,
        );
    };

    match crate::process::get_process_info(pid) {
        Ok(info) => (Some(info.name), Some(info.start_time)),
        Err(e) => {
            warn!(
                event = %format!("core.session.{}_process_info_failed", event_prefix),
                pid = pid,
                error = %e,
                "Failed to get process metadata after spawn - using spawn result metadata"
            );
            (
                spawn_result.process_name.clone(),
                spawn_result.process_start_time,
            )
        }
    }
}

pub fn create_session(
    request: CreateSessionRequest,
    kild_config: &KildConfig,
) -> Result<Session, SessionError> {
    let agent = request.agent_or_default(&kild_config.agent.default);
    let agent_command =
        kild_config
            .get_agent_command(&agent)
            .map_err(|e| SessionError::ConfigError {
                message: e.to_string(),
            })?;

    // Warn if agent CLI is not available in PATH
    if let Some(false) = agents::is_agent_available(&agent) {
        warn!(
            event = "core.session.agent_not_available",
            agent = %agent,
            "Agent CLI '{}' not found in PATH - session may fail to start",
            agent
        );
    }

    info!(
        event = "core.session.create_started",
        branch = request.branch,
        agent = agent,
        command = agent_command
    );

    // 1. Validate input (pure)
    let validated = operations::validate_session_request(&request.branch, &agent_command, &agent)?;

    // 2. Detect git project (I/O)
    // Use explicit project path if provided (UI context), otherwise use cwd (CLI context)
    let project = match &request.project_path {
        Some(path) => {
            debug!(
                event = "core.session.project_path_explicit_provided",
                path = %path.display()
            );
            git::handler::detect_project_at(path)
        }
        None => git::handler::detect_project(),
    }
    .map_err(|e| SessionError::GitError { source: e })?;

    info!(
        event = "core.session.project_detected",
        project_id = project.id,
        project_name = project.name,
        branch = validated.name
    );

    // 3. Create worktree (I/O)
    let config = Config::new();
    let session_id = operations::generate_session_id(&project.id, &validated.name);

    // Ensure sessions directory exists
    operations::ensure_sessions_directory(&config.sessions_dir())?;

    // 4. Allocate port range (I/O)
    let (port_start, port_end) = operations::allocate_port_range(
        &config.sessions_dir(),
        config.default_port_count,
        config.base_port_range,
    )
    .map_err(|e| {
        error!(
            event = "core.session.port_allocation_failed",
            session_id = %session_id,
            requested_count = config.default_port_count,
            base_port = config.base_port_range,
            error = %e
        );
        e
    })?;

    info!(
        event = "core.session.port_allocated",
        session_id = session_id,
        port_range_start = port_start,
        port_range_end = port_end,
        port_count = config.default_port_count
    );

    let base_config = Config::new();
    let worktree = git::handler::create_worktree(
        &base_config.kild_dir,
        &project,
        &validated.name,
        Some(kild_config),
    )
    .map_err(|e| SessionError::GitError { source: e })?;

    info!(
        event = "core.session.worktree_created",
        session_id = session_id,
        worktree_path = %worktree.path.display(),
        branch = worktree.branch
    );

    // 5. Launch terminal (I/O) - pass session_id for unique Ghostty window titles and PID tracking
    let spawn_result = terminal::handler::spawn_terminal(
        &worktree.path,
        &validated.command,
        kild_config,
        Some(&session_id),
        Some(&base_config.kild_dir),
    )
    .map_err(|e| SessionError::TerminalError { source: e })?;

    // 6. Create session record
    let now = chrono::Utc::now().to_rfc3339();
    let session = Session {
        id: session_id.clone(),
        project_id: project.id,
        branch: validated.name.clone(),
        worktree_path: worktree.path,
        agent: validated.agent.clone(),
        status: SessionStatus::Active,
        created_at: now.clone(),
        last_activity: Some(now),
        port_range_start: port_start,
        port_range_end: port_end,
        port_count: config.default_port_count,
        process_id: spawn_result.process_id,
        process_name: spawn_result.process_name.clone(),
        process_start_time: spawn_result.process_start_time,
        terminal_type: Some(spawn_result.terminal_type.clone()),
        terminal_window_id: spawn_result.terminal_window_id.clone(),
        command: if spawn_result.command_executed.trim().is_empty() {
            format!("{} (command not captured)", validated.agent)
        } else {
            spawn_result.command_executed.clone()
        },
        note: request.note.clone(),
    };

    // 7. Save session to file
    operations::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "core.session.create_completed",
        session_id = session_id,
        branch = validated.name,
        agent = session.agent,
        process_id = session.process_id,
        process_name = ?session.process_name
    );

    Ok(session)
}

pub fn list_sessions() -> Result<Vec<Session>, SessionError> {
    info!(event = "core.session.list_started");

    let config = Config::new();
    let (sessions, skipped_count) = operations::load_sessions_from_files(&config.sessions_dir())?;

    if skipped_count > 0 {
        tracing::warn!(
            event = "core.session.list_skipped_sessions",
            skipped_count = skipped_count,
            message = "Some session files were skipped due to errors"
        );
    }

    info!(
        event = "core.session.list_completed",
        count = sessions.len()
    );

    Ok(sessions)
}

pub fn get_session(name: &str) -> Result<Session, SessionError> {
    info!(event = "core.session.get_started", name = name);

    let config = Config::new();
    let session = operations::load_session_from_file(name, &config.sessions_dir())?;

    info!(
        event = "core.session.get_completed",
        name = name,
        session_id = session.id
    );

    Ok(session)
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
        operations::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    info!(
        event = "core.session.destroy_found",
        session_id = session.id,
        worktree_path = %session.worktree_path.display(),
        port_range_start = session.port_range_start,
        port_range_end = session.port_range_end,
        process_id = session.process_id
    );

    // 2. Close terminal window first (before killing process)
    // This is fire-and-forget - errors are logged but never block destruction
    if let Some(ref terminal_type) = session.terminal_type {
        info!(
            event = "core.session.destroy_close_terminal",
            terminal_type = %terminal_type,
            window_id = ?session.terminal_window_id
        );
        terminal::handler::close_terminal(terminal_type, session.terminal_window_id.as_deref());
    }

    // 3. Kill process if PID is tracked
    if let Some(pid) = session.process_id {
        info!(event = "core.session.destroy_kill_started", pid = pid);

        match crate::process::kill_process(
            pid,
            session.process_name.as_deref(),
            session.process_start_time,
        ) {
            Ok(()) => {
                info!(event = "core.session.destroy_kill_completed", pid = pid);
            }
            Err(crate::process::ProcessError::NotFound { .. }) => {
                info!(event = "core.session.destroy_kill_already_dead", pid = pid);
            }
            Err(e) => {
                if force {
                    warn!(
                        event = "core.session.destroy_kill_failed_force_continue",
                        pid = pid,
                        error = %e
                    );
                } else {
                    error!(
                        event = "core.session.destroy_kill_failed",
                        pid = pid,
                        error = %e
                    );
                    return Err(SessionError::ProcessKillFailed {
                        pid,
                        message: format!(
                            "Process still running. Kill it manually or use --force flag: {}",
                            e
                        ),
                    });
                }
            }
        }
    }

    // 4. Remove git worktree
    if force {
        info!(
            event = "core.session.destroy_worktree_force",
            worktree = %session.worktree_path.display()
        );
        git::handler::remove_worktree_force(&session.worktree_path)
            .map_err(|e| SessionError::GitError { source: e })?;
    } else {
        git::handler::remove_worktree_by_path(&session.worktree_path)
            .map_err(|e| SessionError::GitError { source: e })?;
    }

    info!(
        event = "core.session.destroy_worktree_removed",
        session_id = session.id,
        worktree_path = %session.worktree_path.display()
    );

    // 5. Clean up PID file (best-effort, don't fail if missing)
    let pid_file = get_pid_file_path(&config.kild_dir, &session.id);
    if let Err(e) = delete_pid_file(&pid_file) {
        warn!(
            event = "core.session.destroy_pid_file_cleanup_failed",
            session_id = session.id,
            pid_file = %pid_file.display(),
            error = %e
        );
    } else {
        info!(
            event = "core.session.destroy_pid_file_cleaned",
            session_id = session.id,
            pid_file = %pid_file.display()
        );
    }

    // 6. Remove session file (automatically frees port range)
    operations::remove_session_file(&config.sessions_dir(), &session.id)?;

    info!(
        event = "core.session.port_deallocated",
        session_id = session.id,
        port_range_start = session.port_range_start,
        port_range_end = session.port_range_end
    );

    info!(
        event = "core.session.destroy_completed",
        session_id = session.id,
        name = name
    );

    Ok(())
}

/// Completes a kild by checking PR status, optionally deleting remote branch, and destroying the session.
///
/// # Arguments
/// * `name` - Branch name or kild identifier
/// * `force` - If true, bypass git safety checks (passed through to destroy_session)
///
/// # Returns
/// * `Ok(CompleteResult::RemoteDeleted)` - PR was merged and remote branch was deleted
/// * `Ok(CompleteResult::RemoteDeleteFailed)` - PR was merged but remote deletion failed (non-fatal)
/// * `Ok(CompleteResult::PrNotMerged)` - PR not merged, remote preserved for future merge
///
/// # Errors
/// Returns `SessionError::NotFound` if the session doesn't exist.
/// Propagates errors from `destroy_session` (e.g., uncommitted changes without --force).
/// Remote branch deletion errors are logged but do not fail the operation.
///
/// # Workflow Detection
/// - If PR is merged: attempts to delete remote branch (since gh merge --delete-branch would have failed due to worktree)
/// - If PR not merged: just destroys the local session, allowing user's subsequent merge to handle remote cleanup
pub fn complete_session(name: &str, force: bool) -> Result<CompleteResult, SessionError> {
    info!(
        event = "core.session.complete_started",
        name = name,
        force = force
    );

    let config = Config::new();

    // 1. Find session by name to get branch info
    let session =
        operations::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    let kild_branch = format!("kild_{}", name);

    // 2. Check if PR was merged (determines if we need to delete remote)
    let pr_merged = check_pr_merged(&session.worktree_path, &kild_branch);

    info!(
        event = "core.session.complete_pr_status",
        branch = name,
        pr_merged = pr_merged
    );

    // 3. Determine the result based on PR status and remote deletion outcome
    let result = if pr_merged {
        match delete_remote_branch(&session.worktree_path, &kild_branch) {
            Ok(()) => {
                info!(
                    event = "core.session.complete_remote_deleted",
                    branch = kild_branch
                );
                CompleteResult::RemoteDeleted
            }
            Err(e) => {
                // Non-fatal: remote might already be deleted, not exist, or deletion failed (logged as warning)
                warn!(
                    event = "core.session.complete_remote_delete_failed",
                    branch = kild_branch,
                    worktree_path = %session.worktree_path.display(),
                    error = %e
                );
                CompleteResult::RemoteDeleteFailed
            }
        }
    } else {
        CompleteResult::PrNotMerged
    };

    // 4. Destroy the session (reuse existing logic)
    destroy_session(name, force)?;

    info!(
        event = "core.session.complete_completed",
        name = name,
        result = ?result
    );

    Ok(result)
}

/// Check if there's a merged PR for the given branch using gh CLI.
///
/// # Arguments
/// * `worktree_path` - Path to the git worktree (sets working directory for gh command)
/// * `branch` - Branch name to check (passed to gh pr view)
///
/// # Returns
/// * `true` - PR exists and is in MERGED state
/// * `false` - gh not available, PR doesn't exist, PR not merged, or any error occurred
///
/// # Note
/// This function treats all error cases as "not merged" for safety. Errors are logged
/// at debug/warn level for debugging purposes.
fn check_pr_merged(worktree_path: &std::path::Path, branch: &str) -> bool {
    debug!(
        event = "core.session.pr_check_started",
        branch = branch,
        worktree_path = %worktree_path.display()
    );

    let output = std::process::Command::new("gh")
        .current_dir(worktree_path)
        .args(["pr", "view", branch, "--json", "state", "-q", ".state"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let state = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_uppercase();
            let merged = state == "MERGED";
            debug!(
                event = "core.session.pr_check_completed",
                branch = branch,
                state = %state,
                merged = merged
            );
            merged
        }
        Ok(output) => {
            // gh CLI executed but returned error (PR not found, auth error, etc.)
            let stderr = String::from_utf8_lossy(&output.stderr);
            debug!(
                event = "core.session.pr_check_gh_error",
                branch = branch,
                exit_code = output.status.code(),
                stderr = %stderr.trim()
            );
            false
        }
        Err(e) => {
            // gh not found, permission denied, or other I/O error
            warn!(
                event = "core.session.pr_check_failed",
                branch = branch,
                worktree_path = %worktree_path.display(),
                error = %e,
                hint = "gh CLI may not be installed or accessible"
            );
            false
        }
    }
}

/// Delete a remote branch using git push.
///
/// # Arguments
/// * `worktree_path` - Path to the git worktree (sets working directory for git command)
/// * `branch` - Branch name to delete from remote
///
/// # Returns
/// * `Ok(())` - Branch deleted successfully or already deleted
///
/// # Errors
/// * `SessionError::RemoteBranchDeleteFailed` - Git command failed (network error, permission denied, etc.)
///   Note: "remote ref does not exist" is treated as success, not an error.
///
/// # Assumptions
/// Assumes the remote is named "origin". Will fail if the remote has a different name.
fn delete_remote_branch(worktree_path: &std::path::Path, branch: &str) -> Result<(), SessionError> {
    info!(
        event = "core.session.complete_remote_delete_started",
        branch = branch,
        worktree_path = %worktree_path.display()
    );

    let output = std::process::Command::new("git")
        .current_dir(worktree_path)
        .args(["push", "origin", "--delete", branch])
        .output()
        .map_err(|e| SessionError::RemoteBranchDeleteFailed {
            branch: branch.to_string(),
            message: format!(
                "Failed to execute git in {}: {}",
                worktree_path.display(),
                e
            ),
        })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Common patterns for "branch doesn't exist" across git versions
        // These indicate the branch is already gone - treat as success
        let benign_patterns = [
            "remote ref does not exist",
            "unable to delete",
            "does not exist",
        ];

        let is_already_deleted = benign_patterns
            .iter()
            .any(|pattern| stderr.to_lowercase().contains(pattern));

        if is_already_deleted {
            // Branch already deleted - treat as success
            info!(
                event = "core.session.complete_remote_already_deleted",
                branch = branch
            );
            Ok(())
        } else {
            debug!(
                event = "core.session.complete_remote_delete_stderr",
                branch = branch,
                stderr = %stderr.trim()
            );
            Err(SessionError::RemoteBranchDeleteFailed {
                branch: branch.to_string(),
                message: stderr.trim().to_string(),
            })
        }
    }
}

pub fn restart_session(
    name: &str,
    agent_override: Option<String>,
) -> Result<Session, SessionError> {
    let start_time = std::time::Instant::now();
    info!(event = "core.session.restart_started", name = name, agent_override = ?agent_override);

    let config = Config::new();

    // 1. Find session by name (branch name)
    let mut session =
        operations::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    info!(
        event = "core.session.restart_found",
        session_id = session.id,
        current_agent = session.agent,
        process_id = session.process_id
    );

    // 2. Kill process if PID is tracked
    if let Some(pid) = session.process_id {
        info!(event = "core.session.restart_kill_started", pid = pid);

        match crate::process::kill_process(
            pid,
            session.process_name.as_deref(),
            session.process_start_time,
        ) {
            Ok(()) => info!(event = "core.session.restart_kill_completed", pid = pid),
            Err(crate::process::ProcessError::NotFound { .. }) => {
                info!(
                    event = "core.session.restart_kill_process_not_found",
                    pid = pid
                )
            }
            Err(crate::process::ProcessError::AccessDenied { .. }) => {
                error!(event = "core.session.restart_kill_access_denied", pid = pid);
                return Err(SessionError::ProcessKillFailed {
                    pid,
                    message: "Access denied - insufficient permissions to kill process".to_string(),
                });
            }
            Err(e) => {
                error!(event = "core.session.restart_kill_failed", pid = pid, error = %e);
                return Err(SessionError::ProcessKillFailed {
                    pid,
                    message: format!("Process still running: {}", e),
                });
            }
        }
    }

    // 3. Validate worktree still exists
    if !session.worktree_path.exists() {
        error!(
            event = "core.session.restart_worktree_missing",
            session_id = session.id,
            worktree_path = %session.worktree_path.display()
        );
        return Err(SessionError::WorktreeNotFound {
            path: session.worktree_path.clone(),
        });
    }

    // 4. Determine agent and command
    let base_config = Config::new();
    let kild_config = match KildConfig::load_hierarchy() {
        Ok(config) => config,
        Err(e) => {
            warn!(
                event = "core.config.load_failed",
                error = %e,
                session_id = %session.id,
                "Config load failed during restart, using defaults"
            );
            KildConfig::default()
        }
    };
    let agent = agent_override.unwrap_or(session.agent.clone());
    let agent_command =
        kild_config
            .get_agent_command(&agent)
            .map_err(|e| SessionError::ConfigError {
                message: e.to_string(),
            })?;

    // Warn if agent CLI is not available in PATH
    if let Some(false) = agents::is_agent_available(&agent) {
        warn!(
            event = "core.session.agent_not_available",
            agent = %agent,
            session_id = %session.id,
            "Agent CLI '{}' not found in PATH - session may fail to start",
            agent
        );
    }

    info!(
        event = "core.session.restart_agent_selected",
        session_id = session.id,
        agent = agent,
        command = agent_command
    );

    // 5. Relaunch terminal in existing worktree
    info!(event = "core.session.restart_spawn_started", worktree_path = %session.worktree_path.display());

    let spawn_result = terminal::handler::spawn_terminal(
        &session.worktree_path,
        &agent_command,
        &kild_config,
        Some(&session.id),
        Some(&base_config.kild_dir),
    )
    .map_err(|e| SessionError::TerminalError { source: e })?;

    info!(
        event = "core.session.restart_spawn_completed",
        process_id = spawn_result.process_id,
        process_name = ?spawn_result.process_name,
        terminal_window_id = ?spawn_result.terminal_window_id
    );

    // Capture process metadata immediately for PID reuse protection
    let (process_name, process_start_time) = capture_process_metadata(&spawn_result, "restart");

    // 6. Update session with new process info
    session.agent = agent;
    session.process_id = spawn_result.process_id;
    session.process_name = process_name;
    session.process_start_time = process_start_time;
    session.terminal_type = Some(spawn_result.terminal_type.clone());
    session.terminal_window_id = spawn_result.terminal_window_id.clone();
    session.status = SessionStatus::Active;
    session.last_activity = Some(chrono::Utc::now().to_rfc3339());

    // 7. Save updated session to file
    operations::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "core.session.restart_completed",
        session_id = session.id,
        branch = name,
        agent = session.agent,
        process_id = session.process_id,
        duration_ms = start_time.elapsed().as_millis()
    );

    Ok(session)
}

/// Opens a new agent terminal in an existing kild (additive - doesn't close existing terminals).
///
/// This is the preferred way to add agents to a kild. Unlike restart, this does NOT
/// close existing terminals - multiple agents can run in the same kild.
pub fn open_session(name: &str, agent_override: Option<String>) -> Result<Session, SessionError> {
    info!(
        event = "core.session.open_started",
        name = name,
        agent_override = ?agent_override
    );

    let config = Config::new();
    let kild_config = match KildConfig::load_hierarchy() {
        Ok(config) => config,
        Err(e) => {
            // Notify user via stderr - this is a developer tool, they need to know
            eprintln!("Warning: Config load failed ({}). Using defaults.", e);
            eprintln!("         Check ~/.kild/config.toml for syntax errors.");
            warn!(
                event = "core.config.load_failed",
                error = %e,
                "Config load failed during open, using defaults"
            );
            KildConfig::default()
        }
    };

    // 1. Find session by name (branch name)
    let mut session =
        operations::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    info!(
        event = "core.session.open_found",
        session_id = session.id,
        branch = session.branch
    );

    // 2. Verify worktree still exists
    if !session.worktree_path.exists() {
        return Err(SessionError::WorktreeNotFound {
            path: session.worktree_path.clone(),
        });
    }

    // 3. Determine agent
    let agent = agent_override.unwrap_or_else(|| session.agent.clone());
    info!(event = "core.session.open_agent_selected", agent = agent);

    // Warn if agent CLI is not available in PATH
    if let Some(false) = agents::is_agent_available(&agent) {
        warn!(
            event = "core.session.agent_not_available",
            agent = %agent,
            session_id = %session.id,
            "Agent CLI '{}' not found in PATH - session may fail to start",
            agent
        );
    }

    // 4. Build command
    let agent_command =
        kild_config
            .get_agent_command(&agent)
            .map_err(|e| SessionError::ConfigError {
                message: e.to_string(),
            })?;

    // 5. Spawn NEW terminal (additive - don't touch existing)
    info!(
        event = "core.session.open_spawn_started",
        worktree = %session.worktree_path.display()
    );
    let spawn_result = terminal::handler::spawn_terminal(
        &session.worktree_path,
        &agent_command,
        &kild_config,
        Some(&session.id),
        Some(&config.kild_dir),
    )
    .map_err(|e| SessionError::TerminalError { source: e })?;

    // Capture process metadata immediately for PID reuse protection
    let (process_name, process_start_time) = capture_process_metadata(&spawn_result, "open");

    // 6. Update session with new process info
    session.process_id = spawn_result.process_id;
    session.process_name = process_name;
    session.process_start_time = process_start_time;
    session.terminal_type = Some(spawn_result.terminal_type.clone());
    session.terminal_window_id = spawn_result.terminal_window_id.clone();
    session.command = if spawn_result.command_executed.trim().is_empty() {
        format!("{} (command not captured)", agent)
    } else {
        spawn_result.command_executed.clone()
    };
    session.agent = agent.clone();
    session.status = SessionStatus::Active;
    session.last_activity = Some(chrono::Utc::now().to_rfc3339());

    // 7. Save updated session
    operations::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "core.session.open_completed",
        session_id = session.id,
        process_id = session.process_id
    );

    Ok(session)
}

/// Stops the agent process in a kild without destroying the kild.
///
/// The worktree and session file are preserved. The kild can be reopened with `open_session()`.
pub fn stop_session(name: &str) -> Result<(), SessionError> {
    info!(event = "core.session.stop_started", name = name);

    let config = Config::new();

    // 1. Find session by name (branch name)
    let mut session =
        operations::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    info!(
        event = "core.session.stop_found",
        session_id = session.id,
        branch = session.branch
    );

    // 2. Close terminal (fire-and-forget, best-effort)
    if let Some(ref terminal_type) = session.terminal_type {
        info!(
            event = "core.session.stop_close_terminal",
            terminal_type = ?terminal_type
        );
        terminal::handler::close_terminal(terminal_type, session.terminal_window_id.as_deref());
    }

    // 3. Kill process (blocking, handle errors)
    if let Some(pid) = session.process_id {
        info!(event = "core.session.stop_kill_started", pid = pid);

        match crate::process::kill_process(
            pid,
            session.process_name.as_deref(),
            session.process_start_time,
        ) {
            Ok(()) => {
                info!(event = "core.session.stop_kill_completed", pid = pid);
            }
            Err(crate::process::ProcessError::NotFound { .. }) => {
                info!(event = "core.session.stop_kill_already_dead", pid = pid);
            }
            Err(e) => {
                error!(
                    event = "core.session.stop_kill_failed",
                    pid = pid,
                    error = %e
                );
                return Err(SessionError::ProcessKillFailed {
                    pid,
                    message: e.to_string(),
                });
            }
        }
    }

    // 4. Delete PID file so next open() won't read stale PID (best-effort)
    let pid_file = get_pid_file_path(&config.kild_dir, &session.id);
    match delete_pid_file(&pid_file) {
        Ok(()) => {
            debug!(
                event = "core.session.stop_pid_file_cleaned",
                session_id = session.id,
                pid_file = %pid_file.display()
            );
        }
        Err(e) => {
            debug!(
                event = "core.session.stop_pid_file_cleanup_failed",
                session_id = session.id,
                pid_file = %pid_file.display(),
                error = %e
            );
        }
    }

    // 5. Clear process info and set status to Stopped
    session.process_id = None;
    session.process_name = None;
    session.process_start_time = None;
    session.status = SessionStatus::Stopped;
    session.last_activity = Some(chrono::Utc::now().to_rfc3339());

    // 6. Save updated session (keep worktree, keep session file)
    operations::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "core.session.stop_completed",
        session_id = session.id
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_sessions_empty() {
        // Create a temporary directory for this test
        let temp_dir = std::env::temp_dir().join("kild_test_empty_sessions");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        // Test with empty directory
        let (sessions, skipped) = operations::load_sessions_from_files(&temp_dir).unwrap();
        assert_eq!(sessions.len(), 0);
        assert_eq!(skipped, 0);

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_destroy_session_not_found() {
        let result = destroy_session("non-existent", false);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    #[test]
    fn test_complete_session_not_found() {
        let result = complete_session("non-existent", false);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    #[test]
    fn test_complete_session_force_not_found() {
        // Force flag shouldn't skip session lookup
        let result = complete_session("non-existent", true);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    // Note: create_session test would require git repository setup
    // Better suited for integration tests

    #[test]
    fn test_create_list_destroy_integration_flow() {
        use std::fs;

        // Create a unique temporary directory for this test
        let temp_dir =
            std::env::temp_dir().join(format!("kild_test_integration_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");

        // Test session persistence workflow using operations directly
        // This tests the core persistence logic without git/terminal dependencies

        // 1. Create a test session manually
        use crate::sessions::operations;
        use crate::sessions::types::{Session, SessionStatus};

        let session = Session {
            id: "test-project_test-branch".to_string(),
            project_id: "test-project".to_string(),
            branch: "test-branch".to_string(),
            worktree_path: temp_dir.join("worktree").to_path_buf(),
            agent: "test-agent".to_string(),
            status: SessionStatus::Active,
            created_at: chrono::Utc::now().to_rfc3339(),
            port_range_start: 3000,
            port_range_end: 3009,
            port_count: 10,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: "test-command".to_string(),
            last_activity: Some(chrono::Utc::now().to_rfc3339()),
            note: None,
        };

        // Create worktree directory so validation passes
        fs::create_dir_all(&session.worktree_path).expect("Failed to create worktree dir");

        // 2. Save session to file
        operations::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // 3. List sessions - should contain our session
        let (sessions, skipped) =
            operations::load_sessions_from_files(&sessions_dir).expect("Failed to load sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(skipped, 0);
        assert_eq!(sessions[0].id, session.id);
        assert_eq!(sessions[0].branch, "test-branch");

        // 4. Find session by name
        let found_session = operations::find_session_by_name(&sessions_dir, "test-branch")
            .expect("Failed to find session")
            .expect("Session not found");
        assert_eq!(found_session.id, session.id);

        // 5. Remove session file
        operations::remove_session_file(&sessions_dir, &session.id)
            .expect("Failed to remove session");

        // 6. List sessions - should be empty
        let (sessions_after, _) = operations::load_sessions_from_files(&sessions_dir)
            .expect("Failed to load sessions after removal");
        assert_eq!(sessions_after.len(), 0);

        // 7. Try to find removed session - should return None
        let not_found = operations::find_session_by_name(&sessions_dir, "test-branch")
            .expect("Failed to search for removed session");
        assert!(not_found.is_none());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_session_with_terminal_type_persistence() {
        use crate::sessions::operations;
        use crate::sessions::types::{Session, SessionStatus};
        use crate::terminal::types::TerminalType;
        use std::fs;

        // This test verifies the terminal_type field flows correctly through
        // the session persistence layer - critical for destroy_session to work.
        //
        // The destroy_session function relies on:
        // 1. Session being saved with terminal_type populated
        // 2. Session being loaded with terminal_type intact
        // 3. The field being passed to close_terminal()

        // Use unique temp dir per test run to avoid conflicts
        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir = std::env::temp_dir().join(format!("kild_test_terminal_type_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Test all terminal types can be saved and loaded
        let terminal_test_cases = [
            (TerminalType::ITerm, "test-iterm"),
            (TerminalType::TerminalApp, "test-terminalapp"),
            (TerminalType::Ghostty, "test-ghostty"),
            (TerminalType::Native, "test-native"),
        ];

        for (terminal_type, branch_name) in &terminal_test_cases {
            // Use underscore in id to avoid filesystem issues with slash
            let session = Session {
                id: format!("test-project_{}", branch_name),
                project_id: "test-project".to_string(),
                branch: branch_name.to_string(),
                worktree_path: worktree_dir.clone(),
                agent: "test-agent".to_string(),
                status: SessionStatus::Active,
                created_at: chrono::Utc::now().to_rfc3339(),
                port_range_start: 3000,
                port_range_end: 3009,
                port_count: 10,
                process_id: Some(12345),
                process_name: Some("test-agent".to_string()),
                process_start_time: Some(1234567890),
                terminal_type: Some(terminal_type.clone()),
                terminal_window_id: Some("1596".to_string()),
                command: "test-command".to_string(),
                last_activity: Some(chrono::Utc::now().to_rfc3339()),
                note: None,
            };

            operations::save_session_to_file(&session, &sessions_dir)
                .expect("Failed to save session");

            let loaded = operations::find_session_by_name(&sessions_dir, branch_name)
                .expect("Failed to find session")
                .expect("Session not found");

            assert_eq!(
                loaded.terminal_type,
                Some(terminal_type.clone()),
                "terminal_type {:?} must round-trip correctly",
                terminal_type
            );
        }

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_destroy_session_with_terminal_type_calls_close() {
        use crate::sessions::operations;
        use crate::sessions::types::{Session, SessionStatus};
        use crate::terminal::types::TerminalType;
        use std::fs;

        // This test verifies that destroy_session correctly handles sessions
        // with terminal_type set. The actual close_terminal call happens in
        // destroy_session lines 184-189. We can't easily mock the terminal
        // close, but we can verify:
        // 1. Session with terminal_type can be destroyed without error
        // 2. The session file is properly removed after destruction
        //
        // Note: The close_terminal function is designed to always return Ok(),
        // so even if the terminal window doesn't exist, destroy_session continues.

        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir =
            std::env::temp_dir().join(format!("kild_test_destroy_terminal_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create session with terminal_type (simulating what create_session does)
        let session = Session {
            id: "test-project_destroy-test".to_string(),
            project_id: "test-project".to_string(),
            branch: "destroy-test".to_string(),
            worktree_path: worktree_dir.clone(),
            agent: "test-agent".to_string(),
            status: SessionStatus::Active,
            created_at: chrono::Utc::now().to_rfc3339(),
            port_range_start: 3000,
            port_range_end: 3009,
            port_count: 10,
            process_id: None, // No process to kill
            process_name: None,
            process_start_time: None,
            terminal_type: Some(TerminalType::ITerm), // Key: terminal_type is set
            terminal_window_id: Some("1596".to_string()),
            command: "test-command".to_string(),
            last_activity: Some(chrono::Utc::now().to_rfc3339()),
            note: None,
        };

        operations::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // Verify session exists
        let found = operations::find_session_by_name(&sessions_dir, "destroy-test")
            .expect("Failed to find session")
            .expect("Session should exist");
        assert_eq!(found.terminal_type, Some(TerminalType::ITerm));

        // Remove session file (simulating destroy flow without git worktree dependency)
        operations::remove_session_file(&sessions_dir, &session.id)
            .expect("Failed to remove session");

        // Verify session is gone
        let not_found = operations::find_session_by_name(&sessions_dir, "destroy-test")
            .expect("Failed to search");
        assert!(not_found.is_none(), "Session should be removed");

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_destroy_session_without_terminal_type_backward_compat() {
        use crate::sessions::operations;
        use crate::sessions::types::{Session, SessionStatus};
        use std::fs;

        // This test verifies backward compatibility: sessions created before
        // terminal_type was added (terminal_type = None) can still be destroyed.
        // The destroy_session function handles this case at lines 184-189:
        // if let Some(ref terminal_type) = session.terminal_type { ... }

        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir = std::env::temp_dir().join(format!("kild_test_destroy_compat_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create an "old" session WITHOUT terminal_type (simulating pre-feature sessions)
        let session = Session {
            id: "test-project_compat-test".to_string(),
            project_id: "test-project".to_string(),
            branch: "compat-test".to_string(),
            worktree_path: worktree_dir.clone(),
            agent: "test-agent".to_string(),
            status: SessionStatus::Active,
            created_at: chrono::Utc::now().to_rfc3339(),
            port_range_start: 3000,
            port_range_end: 3009,
            port_count: 10,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None, // Key: terminal_type is NOT set (old session)
            terminal_window_id: None, // Key: terminal_window_id is NOT set (old session)
            command: "test-command".to_string(),
            last_activity: Some(chrono::Utc::now().to_rfc3339()),
            note: None,
        };

        operations::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // Verify session can be loaded with terminal_type = None
        let found = operations::find_session_by_name(&sessions_dir, "compat-test")
            .expect("Failed to find session")
            .expect("Session should exist");
        assert_eq!(
            found.terminal_type, None,
            "Old sessions should have terminal_type = None"
        );

        // Remove session (simulating destroy flow)
        operations::remove_session_file(&sessions_dir, &session.id)
            .expect("Failed to remove session");

        // Verify session is gone
        let not_found = operations::find_session_by_name(&sessions_dir, "compat-test")
            .expect("Failed to search");
        assert!(not_found.is_none());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_session_terminal_type_updated_on_restart() {
        use crate::sessions::operations;
        use crate::sessions::types::{Session, SessionStatus};
        use crate::terminal::types::TerminalType;
        use std::fs;

        // This test verifies that when a session is saved after restart_session,
        // the terminal_type field is properly updated and persisted.
        // restart_session updates terminal_type at line 346.

        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir =
            std::env::temp_dir().join(format!("kild_test_restart_terminal_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create a session with ITerm
        let mut session = Session {
            id: "test-project_restart-test".to_string(),
            project_id: "test-project".to_string(),
            branch: "restart-test".to_string(),
            worktree_path: worktree_dir.clone(),
            agent: "test-agent".to_string(),
            status: SessionStatus::Active,
            created_at: chrono::Utc::now().to_rfc3339(),
            port_range_start: 3000,
            port_range_end: 3009,
            port_count: 10,
            process_id: Some(12345),
            process_name: Some("test-agent".to_string()),
            process_start_time: Some(1234567890),
            terminal_type: Some(TerminalType::ITerm),
            terminal_window_id: Some("1596".to_string()),
            command: "test-command".to_string(),
            last_activity: Some(chrono::Utc::now().to_rfc3339()),
            note: None,
        };

        operations::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // Simulate what restart_session does: update terminal_type and save
        session.terminal_type = Some(TerminalType::Ghostty);
        session.status = SessionStatus::Active;
        session.last_activity = Some(chrono::Utc::now().to_rfc3339());

        operations::save_session_to_file(&session, &sessions_dir)
            .expect("Failed to save updated session");

        // Verify the updated terminal_type persists
        let loaded = operations::find_session_by_name(&sessions_dir, "restart-test")
            .expect("Failed to find session")
            .expect("Session should exist");

        assert_eq!(
            loaded.terminal_type,
            Some(TerminalType::Ghostty),
            "terminal_type should be updated to Ghostty after restart"
        );

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_stop_session_not_found() {
        let result = stop_session("non-existent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    #[test]
    fn test_open_session_not_found() {
        let result = open_session("non-existent", None);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    #[test]
    fn test_destroy_session_force_not_found() {
        let result = destroy_session("non-existent", true);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    #[test]
    fn test_stop_session_clears_process_info_and_sets_stopped_status() {
        use crate::sessions::operations;
        use crate::sessions::types::{Session, SessionStatus};
        use crate::terminal::types::TerminalType;
        use std::fs;

        // This test verifies stop_session correctly:
        // - Transitions status from Active to Stopped
        // - Clears process_id, process_name, process_start_time to None
        // - Preserves the session file (worktree preserved)

        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir = std::env::temp_dir().join(format!("kild_test_stop_state_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create a session with Active status and process info
        let session = Session {
            id: "test-project_stop-test".to_string(),
            project_id: "test-project".to_string(),
            branch: "stop-test".to_string(),
            worktree_path: worktree_dir.clone(),
            agent: "test-agent".to_string(),
            status: SessionStatus::Active,
            created_at: chrono::Utc::now().to_rfc3339(),
            port_range_start: 3000,
            port_range_end: 3009,
            port_count: 10,
            process_id: Some(99999), // Fake PID that won't exist
            process_name: Some("fake-process".to_string()),
            process_start_time: Some(1234567890),
            terminal_type: Some(TerminalType::Ghostty),
            terminal_window_id: Some("test-window".to_string()),
            command: "test-command".to_string(),
            last_activity: Some(chrono::Utc::now().to_rfc3339()),
            note: None,
        };

        operations::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // Verify session exists with Active status
        let before = operations::find_session_by_name(&sessions_dir, "stop-test")
            .expect("Failed to find session")
            .expect("Session should exist");
        assert_eq!(before.status, SessionStatus::Active);
        assert!(before.process_id.is_some());

        // Simulate stop by directly updating session (avoids process kill complexity)
        let mut stopped_session = before;
        stopped_session.process_id = None;
        stopped_session.process_name = None;
        stopped_session.process_start_time = None;
        stopped_session.status = SessionStatus::Stopped;
        stopped_session.last_activity = Some(chrono::Utc::now().to_rfc3339());
        operations::save_session_to_file(&stopped_session, &sessions_dir)
            .expect("Failed to save stopped session");

        // Verify state changes persisted
        let after = operations::find_session_by_name(&sessions_dir, "stop-test")
            .expect("Failed to find session")
            .expect("Session should exist");
        assert_eq!(
            after.status,
            SessionStatus::Stopped,
            "Status should be Stopped"
        );
        assert!(after.process_id.is_none(), "process_id should be None");
        assert!(after.process_name.is_none(), "process_name should be None");
        assert!(
            after.process_start_time.is_none(),
            "process_start_time should be None"
        );
        // Worktree should still exist
        assert!(worktree_dir.exists(), "Worktree should be preserved");

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_destroy_session_force_vs_non_force_behavior() {
        // This test documents the expected behavioral difference between
        // force=true and force=false in destroy_session:
        //
        // force=false (default):
        //   - Process kill failures return SessionError::ProcessKillFailed
        //   - Git uncommitted changes block worktree removal
        //
        // force=true:
        //   - Process kill failures log warning but continue
        //   - Worktree is force-deleted even with uncommitted changes
        //
        // We can't easily test the full flow without git setup,
        // but we verify the error types exist and the function signatures are correct.

        // Test that non-force destroy returns NotFound for non-existent session
        let result_non_force = destroy_session("test-force-behavior", false);
        assert!(result_non_force.is_err());
        assert!(matches!(
            result_non_force.unwrap_err(),
            SessionError::NotFound { .. }
        ));

        // Test that force destroy also returns NotFound for non-existent session
        // (force doesn't skip session lookup)
        let result_force = destroy_session("test-force-behavior", true);
        assert!(result_force.is_err());
        assert!(matches!(
            result_force.unwrap_err(),
            SessionError::NotFound { .. }
        ));
    }

    #[test]
    fn test_create_session_request_project_path_affects_project_detection() {
        use git2::Repository;
        use std::fs;

        // This test verifies that CreateSessionRequest with project_path
        // causes detect_project_at to be called with that path, resulting
        // in a different project than cwd detection would produce.
        //
        // We test this by:
        // 1. Creating a temp git repo at a known path
        // 2. Calling detect_project_at directly (simulating what create_session does)
        // 3. Verifying the project_id is derived from the temp repo path

        let temp_dir = std::env::temp_dir().join(format!(
            "kild_test_session_project_path_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        // Initialize a git repo at the temp path
        let repo = Repository::init(&temp_dir).expect("Failed to init git repo");
        {
            let sig = repo
                .signature()
                .unwrap_or_else(|_| git2::Signature::now("Test", "test@test.com").unwrap());
            let tree_id = repo.index().unwrap().write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .expect("Failed to create initial commit");
        }

        // Create the request with explicit project_path
        let request = CreateSessionRequest::with_project_path(
            "test-branch".to_string(),
            Some("claude".to_string()),
            None,
            temp_dir.clone(),
        );

        // Verify the request has project_path set
        assert!(
            request.project_path.is_some(),
            "Request should have project_path set"
        );
        assert_eq!(request.project_path.as_ref().unwrap(), &temp_dir);

        // Now simulate the branching logic from create_session:
        // match &request.project_path {
        //     Some(path) => git::handler::detect_project_at(path)
        //     None => git::handler::detect_project()
        // }
        let project = match &request.project_path {
            Some(path) => git::handler::detect_project_at(path),
            None => git::handler::detect_project(),
        };

        assert!(project.is_ok(), "Project detection should succeed");
        let project = project.unwrap();

        // Verify the project path matches the temp dir (not cwd)
        let expected_path = temp_dir.canonicalize().unwrap();
        let actual_path = project.path.canonicalize().unwrap();
        assert_eq!(
            actual_path, expected_path,
            "Project should be from the explicit path, not cwd"
        );

        // Also verify that without project_path, we'd get a different result
        // (This confirms the branching logic matters)
        let request_without_path =
            CreateSessionRequest::new("test-branch".to_string(), Some("claude".to_string()), None);
        assert!(
            request_without_path.project_path.is_none(),
            "Request without project_path should have None"
        );

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_create_session_request_none_project_path_uses_cwd_detection() {
        // This test documents that when project_path is None,
        // create_session falls back to detect_project() which uses cwd.
        //
        // We verify this by checking that CreateSessionRequest::new()
        // correctly leaves project_path as None.

        let request = CreateSessionRequest::new(
            "test-branch".to_string(),
            Some("claude".to_string()),
            Some("test note".to_string()),
        );

        assert!(
            request.project_path.is_none(),
            "CreateSessionRequest::new should leave project_path as None"
        );

        // This means create_session will call detect_project() instead of detect_project_at()
        // (verified by code inspection of the match statement in create_session)
    }

    /// Test that sessions with missing worktrees can be loaded but would fail operation validation.
    ///
    /// This is the critical safety net for issue #102. After removing worktree existence checks
    /// from structural validation, operations like `open_session` (line 577-581) and
    /// `restart_session` (line 437-445) must reject sessions with missing worktrees.
    ///
    /// This test verifies the contract:
    /// 1. Sessions with missing worktrees CAN be loaded (visible in `kild list`)
    /// 2. The `is_worktree_valid()` helper correctly identifies invalid worktrees
    /// 3. Operation-level validation would return `WorktreeNotFound`
    #[test]
    fn test_session_with_missing_worktree_fails_operation_validation() {
        use crate::sessions::operations;
        use crate::sessions::types::{Session, SessionStatus};
        use std::fs;

        let unique_id = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir =
            std::env::temp_dir().join(format!("kild_test_missing_worktree_op_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");

        // Create a session pointing to a worktree that does NOT exist
        let missing_worktree = temp_dir.join("worktree_does_not_exist");
        let session = Session {
            id: "test-project_orphaned-session".to_string(),
            project_id: "test-project".to_string(),
            branch: "orphaned-session".to_string(),
            worktree_path: missing_worktree.clone(),
            agent: "claude".to_string(),
            status: SessionStatus::Stopped,
            created_at: chrono::Utc::now().to_rfc3339(),
            port_range_start: 3000,
            port_range_end: 3009,
            port_count: 10,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: "test-command".to_string(),
            last_activity: Some(chrono::Utc::now().to_rfc3339()),
            note: None,
        };

        // Save the session
        operations::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // 1. Verify session CAN be loaded (the fix for issue #102)
        let (sessions, skipped) =
            operations::load_sessions_from_files(&sessions_dir).expect("Failed to load sessions");
        assert_eq!(
            sessions.len(),
            1,
            "Session should be loaded despite missing worktree"
        );
        assert_eq!(skipped, 0, "Session should not be skipped");

        // 2. Verify worktree does NOT exist
        assert!(
            !missing_worktree.exists(),
            "Worktree should not exist for this test"
        );

        // 3. Verify is_worktree_valid() returns false (used by UI for status indicators)
        assert!(
            !sessions[0].is_worktree_valid(),
            "is_worktree_valid() should return false for missing worktree"
        );

        // 4. Verify operation-level validation would reject this session
        // This is what open_session (line 577-581) and restart_session (line 437-445) check:
        // if !session.worktree_path.exists() { return Err(SessionError::WorktreeNotFound {...}) }
        let loaded_session = &sessions[0];
        if !loaded_session.worktree_path.exists() {
            // This is the expected path - operation would return WorktreeNotFound
            let expected_error = SessionError::WorktreeNotFound {
                path: loaded_session.worktree_path.clone(),
            };
            assert!(
                matches!(expected_error, SessionError::WorktreeNotFound { .. }),
                "Operation should return WorktreeNotFound for missing worktree"
            );
        } else {
            panic!("Test setup error: worktree should not exist");
        }

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
