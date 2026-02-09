use tracing::{debug, error, info, warn};

use crate::agents;
use crate::config::{Config, KildConfig};
use crate::git;
use crate::sessions::{errors::SessionError, persistence, ports, types::*, validation};
use crate::terminal;
use crate::terminal::types::SpawnResult;

// Re-export from submodules to preserve public API (session_ops::*)
pub use super::agent_status::{
    find_session_by_worktree_path, read_agent_status, update_agent_status,
};
pub use super::complete::{complete_session, fetch_pr_info, read_pr_info};
pub use super::destroy::{destroy_session, get_destroy_safety_info, has_remote_configured};

/// Compute a unique spawn ID for a given session and spawn index.
///
/// Each agent spawn within a session gets its own spawn ID, which is used for
/// per-agent PID file paths and window titles. This prevents race conditions
/// where `kild open` on a running kild would read the wrong PID.
fn compute_spawn_id(session_id: &str, spawn_index: usize) -> String {
    format!("{}_{}", session_id, spawn_index)
}

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
    // Determine agent name and command based on AgentMode
    let (agent, agent_command) = match &request.agent_mode {
        crate::state::types::AgentMode::BareShell => {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| {
                let fallback = "/bin/sh".to_string();
                warn!(
                    event = "core.session.shell_env_missing",
                    fallback = %fallback,
                    "$SHELL not set, falling back to /bin/sh"
                );
                fallback
            });
            info!(event = "core.session.create_shell_selected", shell = %shell);
            ("shell".to_string(), shell)
        }
        crate::state::types::AgentMode::Agent(name) => {
            let command =
                kild_config
                    .get_agent_command(name)
                    .map_err(|e| SessionError::ConfigError {
                        message: e.to_string(),
                    })?;

            if let Some(false) = agents::is_agent_available(name) {
                warn!(
                    event = "core.session.agent_not_available",
                    agent = %name,
                    "Agent CLI '{}' not found in PATH - session may fail to start",
                    name
                );
            }

            (name.clone(), command)
        }
        crate::state::types::AgentMode::DefaultAgent => {
            let name = kild_config.agent.default.clone();
            let command =
                kild_config
                    .get_agent_command(&name)
                    .map_err(|e| SessionError::ConfigError {
                        message: e.to_string(),
                    })?;

            if let Some(false) = agents::is_agent_available(&name) {
                warn!(
                    event = "core.session.agent_not_available",
                    agent = %name,
                    "Agent CLI '{}' not found in PATH - session may fail to start",
                    name
                );
            }

            (name, command)
        }
    };

    info!(
        event = "core.session.create_started",
        branch = request.branch,
        agent = agent,
        command = agent_command
    );

    // 1. Validate input (pure)
    let validated = validation::validate_session_request(&request.branch, &agent_command, &agent)?;

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
    let session_id = ports::generate_session_id(&project.id, &validated.name);

    // Ensure sessions directory exists
    persistence::ensure_sessions_directory(&config.sessions_dir())?;

    // 4. Allocate port range (I/O)
    let (port_start, port_end) = ports::allocate_port_range(
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

    // Build effective git config with CLI overrides
    let mut git_config = kild_config.git.clone();
    if let Some(base) = &request.base_branch {
        git_config.base_branch = Some(base.clone());
    }
    if request.no_fetch {
        git_config.fetch_before_create = Some(false);
    }

    let worktree = git::handler::create_worktree(
        &base_config.kild_dir,
        &project,
        &validated.name,
        Some(kild_config),
        &git_config,
    )
    .map_err(|e| SessionError::GitError { source: e })?;

    info!(
        event = "core.session.worktree_created",
        session_id = session_id,
        worktree_path = %worktree.path.display(),
        branch = worktree.branch
    );

    // 5. Launch terminal (I/O) - pass spawn_id for unique PID files and Ghostty window titles
    let spawn_id = compute_spawn_id(&session_id, 0);
    let spawn_result = terminal::handler::spawn_terminal(
        &worktree.path,
        &validated.command,
        kild_config,
        Some(&spawn_id),
        Some(&base_config.kild_dir),
    )
    .map_err(|e| SessionError::TerminalError { source: e })?;

    // 6. Create session record
    let now = chrono::Utc::now().to_rfc3339();
    let command = if spawn_result.command_executed.trim().is_empty() {
        format!("{} (command not captured)", validated.agent)
    } else {
        spawn_result.command_executed.clone()
    };
    let initial_agent = AgentProcess::new(
        validated.agent.clone(),
        spawn_id,
        spawn_result.process_id,
        spawn_result.process_name.clone(),
        spawn_result.process_start_time,
        Some(spawn_result.terminal_type.clone()),
        spawn_result.terminal_window_id.clone(),
        command.clone(),
        now.clone(),
    )?;
    let session = Session::new(
        session_id.clone(),
        project.id,
        validated.name.clone(),
        worktree.path,
        validated.agent.clone(),
        SessionStatus::Active,
        now.clone(),
        port_start,
        port_end,
        config.default_port_count,
        Some(now),
        request.note.clone(),
        vec![initial_agent],
    );

    // 7. Save session to file
    persistence::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "core.session.create_completed",
        session_id = session_id,
        branch = validated.name,
        agent = session.agent,
        process_id = session.latest_agent().and_then(|a| a.process_id()),
        process_name = ?session.latest_agent().map(|a| a.process_name())
    );

    Ok(session)
}

pub fn list_sessions() -> Result<Vec<Session>, SessionError> {
    info!(event = "core.session.list_started");

    let config = Config::new();
    let (sessions, skipped_count) = persistence::load_sessions_from_files(&config.sessions_dir())?;

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
    let session = persistence::load_session_from_file(name, &config.sessions_dir())?;

    info!(
        event = "core.session.get_completed",
        name = name,
        session_id = session.id
    );

    Ok(session)
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
        persistence::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    info!(
        event = "core.session.restart_found",
        session_id = session.id,
        current_agent = session.agent,
        agent_count = session.agent_count()
    );

    // 2. Kill process if PID is tracked (use latest agent from agents vec)
    if let Some(latest) = session.latest_agent()
        && let Some(pid) = latest.process_id()
    {
        info!(event = "core.session.restart_kill_started", pid = pid);

        match crate::process::kill_process(pid, latest.process_name(), latest.process_start_time())
        {
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

    let spawn_id = compute_spawn_id(&session.id, 0);
    let spawn_result = terminal::handler::spawn_terminal(
        &session.worktree_path,
        &agent_command,
        &kild_config,
        Some(&spawn_id),
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
    let now = chrono::Utc::now().to_rfc3339();
    let command = if spawn_result.command_executed.trim().is_empty() {
        format!("{} (command not captured)", agent)
    } else {
        spawn_result.command_executed.clone()
    };
    let new_agent = AgentProcess::new(
        agent.clone(),
        spawn_id,
        spawn_result.process_id,
        process_name.clone(),
        process_start_time,
        Some(spawn_result.terminal_type.clone()),
        spawn_result.terminal_window_id.clone(),
        command,
        now.clone(),
    )?;

    session.agent = agent;
    session.status = SessionStatus::Active;
    session.last_activity = Some(now);
    session.clear_agents();
    session.add_agent(new_agent);

    // 7. Save updated session to file
    persistence::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "core.session.restart_completed",
        session_id = session.id,
        branch = name,
        agent = session.agent,
        process_id = session.latest_agent().and_then(|a| a.process_id()),
        duration_ms = start_time.elapsed().as_millis()
    );

    Ok(session)
}

/// Opens a new agent terminal in an existing kild (additive - doesn't close existing terminals).
///
/// This is the preferred way to add agents to a kild. Unlike restart, this does NOT
/// close existing terminals - multiple agents can run in the same kild.
pub fn open_session(
    name: &str,
    mode: crate::state::types::OpenMode,
) -> Result<Session, SessionError> {
    info!(
        event = "core.session.open_started",
        name = name,
        mode = ?mode
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
        persistence::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
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

    // 3. Determine agent and command based on OpenMode
    let is_bare_shell = matches!(mode, crate::state::types::OpenMode::BareShell);
    let (agent, agent_command) =
        match mode {
            crate::state::types::OpenMode::BareShell => {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| {
                    let fallback = "/bin/sh".to_string();
                    warn!(
                        event = "core.session.shell_env_missing",
                        fallback = %fallback,
                        "$SHELL not set, falling back to /bin/sh"
                    );
                    fallback
                });
                info!(event = "core.session.open_shell_selected", shell = %shell);
                // Keep the session's original agent — no agent is actually running
                (session.agent.clone(), shell)
            }
            crate::state::types::OpenMode::Agent(name) => {
                info!(event = "core.session.open_agent_selected", agent = name);

                // Warn if agent CLI is not available in PATH
                if let Some(false) = agents::is_agent_available(&name) {
                    warn!(
                        event = "core.session.agent_not_available",
                        agent = %name,
                        session_id = %session.id,
                        "Agent CLI '{}' not found in PATH - session may fail to start",
                        name
                    );
                }

                let command = kild_config.get_agent_command(&name).map_err(|e| {
                    SessionError::ConfigError {
                        message: e.to_string(),
                    }
                })?;
                (name, command)
            }
            crate::state::types::OpenMode::DefaultAgent => {
                let agent = session.agent.clone();
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

                let command = kild_config.get_agent_command(&agent).map_err(|e| {
                    SessionError::ConfigError {
                        message: e.to_string(),
                    }
                })?;
                (agent, command)
            }
        };

    // 4. Spawn NEW terminal (additive - don't touch existing)
    let spawn_index = session.agent_count();
    let spawn_id = compute_spawn_id(&session.id, spawn_index);
    info!(
        event = "core.session.open_spawn_started",
        worktree = %session.worktree_path.display(),
        spawn_id = %spawn_id
    );
    let spawn_result = terminal::handler::spawn_terminal(
        &session.worktree_path,
        &agent_command,
        &kild_config,
        Some(&spawn_id),
        Some(&config.kild_dir),
    )
    .map_err(|e| SessionError::TerminalError { source: e })?;

    // Capture process metadata immediately for PID reuse protection
    let (process_name, process_start_time) = capture_process_metadata(&spawn_result, "open");

    // 5. Update session with new process info
    let now = chrono::Utc::now().to_rfc3339();
    let command = if spawn_result.command_executed.trim().is_empty() {
        format!("{} (command not captured)", agent)
    } else {
        spawn_result.command_executed.clone()
    };
    let new_agent = AgentProcess::new(
        agent.clone(),
        spawn_id,
        spawn_result.process_id,
        process_name.clone(),
        process_start_time,
        Some(spawn_result.terminal_type.clone()),
        spawn_result.terminal_window_id.clone(),
        command.clone(),
        now.clone(),
    )?;

    // When bare shell, keep session Stopped (no agent is running).
    // Otherwise, mark as Active.
    if !is_bare_shell {
        session.status = SessionStatus::Active;
    }
    session.last_activity = Some(now);
    session.add_agent(new_agent);

    // 6. Save updated session
    persistence::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "core.session.open_completed",
        session_id = session.id,
        agent_count = session.agent_count()
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
        persistence::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    info!(
        event = "core.session.stop_found",
        session_id = session.id,
        branch = session.branch
    );

    // 2. Close all terminal windows and kill all processes
    {
        if !session.has_agents() {
            warn!(
                event = "core.session.stop_no_agents",
                session_id = session.id,
                branch = session.branch,
                "Session has no tracked agents — skipping process/terminal cleanup"
            );
        }

        // Iterate all tracked agents
        for agent_proc in session.agents() {
            if let (Some(terminal_type), Some(window_id)) =
                (agent_proc.terminal_type(), agent_proc.terminal_window_id())
            {
                info!(
                    event = "core.session.stop_close_terminal",
                    terminal_type = ?terminal_type,
                    agent = agent_proc.agent(),
                );
                terminal::handler::close_terminal(terminal_type, Some(window_id));
            }
        }

        let mut kill_errors: Vec<(u32, String)> = Vec::new();
        for agent_proc in session.agents() {
            let Some(pid) = agent_proc.process_id() else {
                continue;
            };

            info!(
                event = "core.session.stop_kill_started",
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
                    info!(event = "core.session.stop_kill_completed", pid = pid);
                }
                Err(crate::process::ProcessError::NotFound { .. }) => {
                    info!(event = "core.session.stop_kill_already_dead", pid = pid);
                }
                Err(e) => {
                    error!(event = "core.session.stop_kill_failed", pid = pid, error = %e);
                    kill_errors.push((pid, e.to_string()));
                }
            }
        }

        if !kill_errors.is_empty() {
            for (pid, err) in &kill_errors {
                error!(
                    event = "core.session.stop_kill_failed_summary",
                    pid = pid,
                    error = %err
                );
            }

            let pids: Vec<String> = kill_errors.iter().map(|(p, _)| p.to_string()).collect();
            let (first_pid, first_msg) = kill_errors.into_iter().next().unwrap();

            let message = if pids.len() == 1 {
                first_msg
            } else {
                format!(
                    "{} processes failed to stop (PIDs: {}). Kill them manually.",
                    pids.len(),
                    pids.join(", ")
                )
            };

            return Err(SessionError::ProcessKillFailed {
                pid: first_pid,
                message,
            });
        }
    }

    // 3. Delete PID files so next open() won't read stale PIDs (best-effort)
    super::destroy::cleanup_session_pid_files(&session, &config.kild_dir, "stop");

    // 4. Clear process info and set status to Stopped
    session.clear_agents();
    session.status = SessionStatus::Stopped;
    session.last_activity = Some(chrono::Utc::now().to_rfc3339());

    // 6. Save updated session (keep worktree, keep session file)
    persistence::save_session_to_file(&session, &config.sessions_dir())?;

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
        let (sessions, skipped) = persistence::load_sessions_from_files(&temp_dir).unwrap();
        assert_eq!(sessions.len(), 0);
        assert_eq!(skipped, 0);

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
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
        use crate::sessions::persistence;
        use crate::sessions::types::{Session, SessionStatus};

        let session = Session::new(
            "test-project_test-branch".to_string(),
            "test-project".to_string(),
            "test-branch".to_string(),
            temp_dir.join("worktree").to_path_buf(),
            "test-agent".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            Some(chrono::Utc::now().to_rfc3339()),
            None,
            vec![],
        );

        // Create worktree directory so validation passes
        fs::create_dir_all(&session.worktree_path).expect("Failed to create worktree dir");

        // 2. Save session to file
        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // 3. List sessions - should contain our session
        let (sessions, skipped) =
            persistence::load_sessions_from_files(&sessions_dir).expect("Failed to load sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(skipped, 0);
        assert_eq!(sessions[0].id, session.id);
        assert_eq!(sessions[0].branch, "test-branch");

        // 4. Find session by name
        let found_session = persistence::find_session_by_name(&sessions_dir, "test-branch")
            .expect("Failed to find session")
            .expect("Session not found");
        assert_eq!(found_session.id, session.id);

        // 5. Remove session file
        persistence::remove_session_file(&sessions_dir, &session.id)
            .expect("Failed to remove session");

        // 6. List sessions - should be empty
        let (sessions_after, _) = persistence::load_sessions_from_files(&sessions_dir)
            .expect("Failed to load sessions after removal");
        assert_eq!(sessions_after.len(), 0);

        // 7. Try to find removed session - should return None
        let not_found = persistence::find_session_by_name(&sessions_dir, "test-branch")
            .expect("Failed to search for removed session");
        assert!(not_found.is_none());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_session_with_terminal_type_persistence() {
        use crate::sessions::persistence;
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
            let agent = AgentProcess::new(
                "test-agent".to_string(),
                format!("test-project_{}_{}", branch_name, 0),
                Some(12345),
                Some("test-agent".to_string()),
                Some(1234567890),
                Some(terminal_type.clone()),
                Some("1596".to_string()),
                "test-command".to_string(),
                chrono::Utc::now().to_rfc3339(),
            )
            .unwrap();
            let session = Session::new(
                format!("test-project_{}", branch_name),
                "test-project".to_string(),
                branch_name.to_string(),
                worktree_dir.clone(),
                "test-agent".to_string(),
                SessionStatus::Active,
                chrono::Utc::now().to_rfc3339(),
                3000,
                3009,
                10,
                Some(chrono::Utc::now().to_rfc3339()),
                None,
                vec![agent],
            );

            persistence::save_session_to_file(&session, &sessions_dir)
                .expect("Failed to save session");

            let loaded = persistence::find_session_by_name(&sessions_dir, branch_name)
                .expect("Failed to find session")
                .expect("Session not found");

            let loaded_terminal_type = loaded
                .latest_agent()
                .and_then(|a| a.terminal_type().cloned());
            assert_eq!(
                loaded_terminal_type,
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
        use crate::sessions::persistence;
        use crate::sessions::types::{Session, SessionStatus};
        use crate::terminal::types::TerminalType;
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
            std::env::temp_dir().join(format!("kild_test_destroy_terminal_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create session with terminal_type in agent (simulating what create_session does)
        let agent = AgentProcess::new(
            "test-agent".to_string(),
            "test-project_destroy-test_0".to_string(),
            None, // No process to kill
            None,
            None,
            Some(TerminalType::ITerm), // Key: terminal_type is set
            Some("1596".to_string()),
            "test-command".to_string(),
            chrono::Utc::now().to_rfc3339(),
        )
        .unwrap();
        let session = Session::new(
            "test-project_destroy-test".to_string(),
            "test-project".to_string(),
            "destroy-test".to_string(),
            worktree_dir.clone(),
            "test-agent".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            Some(chrono::Utc::now().to_rfc3339()),
            None,
            vec![agent],
        );

        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // Verify session exists
        let found = persistence::find_session_by_name(&sessions_dir, "destroy-test")
            .expect("Failed to find session")
            .expect("Session should exist");
        assert_eq!(
            found
                .latest_agent()
                .and_then(|a| a.terminal_type().cloned()),
            Some(TerminalType::ITerm)
        );

        // Remove session file (simulating destroy flow without git worktree dependency)
        persistence::remove_session_file(&sessions_dir, &session.id)
            .expect("Failed to remove session");

        // Verify session is gone
        let not_found = persistence::find_session_by_name(&sessions_dir, "destroy-test")
            .expect("Failed to search");
        assert!(not_found.is_none(), "Session should be removed");

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_destroy_session_without_terminal_type_backward_compat() {
        use crate::sessions::persistence;
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
        let temp_dir = std::env::temp_dir().join(format!("kild_test_destroy_compat_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create an "old" session WITHOUT agents (simulating pre-feature sessions)
        let session = Session::new(
            "test-project_compat-test".to_string(),
            "test-project".to_string(),
            "compat-test".to_string(),
            worktree_dir.clone(),
            "test-agent".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            Some(chrono::Utc::now().to_rfc3339()),
            None,
            vec![], // No agents (old session)
        );

        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // Verify session can be loaded without agents
        let found = persistence::find_session_by_name(&sessions_dir, "compat-test")
            .expect("Failed to find session")
            .expect("Session should exist");
        assert!(!found.has_agents(), "Old sessions should have no agents");

        // Remove session (simulating destroy flow)
        persistence::remove_session_file(&sessions_dir, &session.id)
            .expect("Failed to remove session");

        // Verify session is gone
        let not_found = persistence::find_session_by_name(&sessions_dir, "compat-test")
            .expect("Failed to search");
        assert!(not_found.is_none());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_session_terminal_type_updated_on_restart() {
        use crate::sessions::persistence;
        use crate::sessions::types::{Session, SessionStatus};
        use crate::terminal::types::TerminalType;
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
            std::env::temp_dir().join(format!("kild_test_restart_terminal_{}", unique_id));
        let _ = fs::remove_dir_all(&temp_dir);
        let sessions_dir = temp_dir.join("sessions");
        let worktree_dir = temp_dir.join("worktree");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");
        fs::create_dir_all(&worktree_dir).expect("Failed to create worktree dir");

        // Create a session with ITerm agent
        let iterm_agent = AgentProcess::new(
            "test-agent".to_string(),
            "test-project_restart-test_0".to_string(),
            Some(12345),
            Some("test-agent".to_string()),
            Some(1234567890),
            Some(TerminalType::ITerm),
            Some("1596".to_string()),
            "test-command".to_string(),
            chrono::Utc::now().to_rfc3339(),
        )
        .unwrap();
        let mut session = Session::new(
            "test-project_restart-test".to_string(),
            "test-project".to_string(),
            "restart-test".to_string(),
            worktree_dir.clone(),
            "test-agent".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            Some(chrono::Utc::now().to_rfc3339()),
            None,
            vec![iterm_agent],
        );

        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // Simulate what restart_session does: clear agents, add new agent with Ghostty
        let ghostty_agent = AgentProcess::new(
            "test-agent".to_string(),
            "test-project_restart-test_0".to_string(),
            Some(12345),
            Some("test-agent".to_string()),
            Some(1234567890),
            Some(TerminalType::Ghostty),
            Some("kild-restart-test".to_string()),
            "test-command".to_string(),
            chrono::Utc::now().to_rfc3339(),
        )
        .unwrap();
        session.status = SessionStatus::Active;
        session.last_activity = Some(chrono::Utc::now().to_rfc3339());
        session.clear_agents();
        session.add_agent(ghostty_agent);

        persistence::save_session_to_file(&session, &sessions_dir)
            .expect("Failed to save updated session");

        // Verify the updated terminal_type persists
        let loaded = persistence::find_session_by_name(&sessions_dir, "restart-test")
            .expect("Failed to find session")
            .expect("Session should exist");

        assert_eq!(
            loaded
                .latest_agent()
                .and_then(|a| a.terminal_type().cloned()),
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
        let result = open_session("non-existent", crate::state::types::OpenMode::DefaultAgent);
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
        use crate::sessions::persistence;
        use crate::sessions::types::{Session, SessionStatus};
        use crate::terminal::types::TerminalType;
        use std::fs;

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

        // Create a session with Active status and process info in agent
        let agent = AgentProcess::new(
            "test-agent".to_string(),
            "test-project_stop-test_0".to_string(),
            Some(99999), // Fake PID that won't exist
            Some("fake-process".to_string()),
            Some(1234567890),
            Some(TerminalType::Ghostty),
            Some("test-window".to_string()),
            "test-command".to_string(),
            chrono::Utc::now().to_rfc3339(),
        )
        .unwrap();
        let session = Session::new(
            "test-project_stop-test".to_string(),
            "test-project".to_string(),
            "stop-test".to_string(),
            worktree_dir.clone(),
            "test-agent".to_string(),
            SessionStatus::Active,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            Some(chrono::Utc::now().to_rfc3339()),
            None,
            vec![agent],
        );

        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // Verify session exists with Active status
        let before = persistence::find_session_by_name(&sessions_dir, "stop-test")
            .expect("Failed to find session")
            .expect("Session should exist");
        assert_eq!(before.status, SessionStatus::Active);
        assert!(before.has_agents());

        // Simulate stop by directly updating session (avoids process kill complexity)
        let mut stopped_session = before;
        stopped_session.clear_agents();
        stopped_session.status = SessionStatus::Stopped;
        stopped_session.last_activity = Some(chrono::Utc::now().to_rfc3339());
        persistence::save_session_to_file(&stopped_session, &sessions_dir)
            .expect("Failed to save stopped session");

        // Verify state changes persisted
        let after = persistence::find_session_by_name(&sessions_dir, "stop-test")
            .expect("Failed to find session")
            .expect("Session should exist");
        assert_eq!(
            after.status,
            SessionStatus::Stopped,
            "Status should be Stopped"
        );
        assert!(!after.has_agents(), "agents should be cleared");
        // Worktree should still exist
        assert!(worktree_dir.exists(), "Worktree should be preserved");

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_destroy_session_force_vs_non_force_behavior() {
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
    fn test_create_session_request_project_path_affects_project_detection() {
        use git2::Repository;
        use std::fs;

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
            crate::state::types::AgentMode::Agent("claude".to_string()),
            None,
            temp_dir.clone(),
        );

        // Verify the request has project_path set
        assert!(
            request.project_path.is_some(),
            "Request should have project_path set"
        );
        assert_eq!(request.project_path.as_ref().unwrap(), &temp_dir);

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
        let request_without_path = CreateSessionRequest::new(
            "test-branch".to_string(),
            crate::state::types::AgentMode::Agent("claude".to_string()),
            None,
        );
        assert!(
            request_without_path.project_path.is_none(),
            "Request without project_path should have None"
        );

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_create_session_request_none_project_path_uses_cwd_detection() {
        let request = CreateSessionRequest::new(
            "test-branch".to_string(),
            crate::state::types::AgentMode::Agent("claude".to_string()),
            Some("test note".to_string()),
        );

        assert!(
            request.project_path.is_none(),
            "CreateSessionRequest::new should leave project_path as None"
        );
    }

    #[test]
    fn test_session_with_missing_worktree_fails_operation_validation() {
        use crate::sessions::persistence;
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
        let session = Session::new(
            "test-project_orphaned-session".to_string(),
            "test-project".to_string(),
            "orphaned-session".to_string(),
            missing_worktree.clone(),
            "claude".to_string(),
            SessionStatus::Stopped,
            chrono::Utc::now().to_rfc3339(),
            3000,
            3009,
            10,
            Some(chrono::Utc::now().to_rfc3339()),
            None,
            vec![],
        );

        // Save the session
        persistence::save_session_to_file(&session, &sessions_dir).expect("Failed to save session");

        // 1. Verify session CAN be loaded (the fix for issue #102)
        let (sessions, skipped) =
            persistence::load_sessions_from_files(&sessions_dir).expect("Failed to load sessions");
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
        let loaded_session = &sessions[0];
        if !loaded_session.worktree_path.exists() {
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
