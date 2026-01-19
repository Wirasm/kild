use tracing::{error, info};

use crate::core::config::{Config, ShardsConfig};
use crate::git;
use crate::sessions::{errors::SessionError, operations, types::*};
use crate::terminal;

pub fn create_session(
    request: CreateSessionRequest,
    shards_config: &ShardsConfig,
) -> Result<Session, SessionError> {
    let agent = request.agent_or_default(&shards_config.agent.default);
    let agent_command = shards_config.get_agent_command(&agent);

    info!(
        event = "session.create_started",
        branch = request.branch,
        agent = agent,
        command = agent_command
    );

    // 1. Validate input (pure)
    let validated = operations::validate_session_request(&request.branch, &agent_command, &agent)?;

    // 2. Detect git project (I/O)
    let project =
        git::handler::detect_project().map_err(|e| SessionError::GitError { source: e })?;

    info!(
        event = "session.project_detected",
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
            event = "session.port_allocation_failed",
            session_id = %session_id,
            requested_count = config.default_port_count,
            base_port = config.base_port_range,
            error = %e
        );
        e
    })?;

    info!(
        event = "session.port_allocated",
        session_id = session_id,
        port_range_start = port_start,
        port_range_end = port_end,
        port_count = config.default_port_count
    );

    let base_config = Config::new();
    let worktree = git::handler::create_worktree(
        &base_config.shards_dir,
        &project,
        &validated.name,
        Some(shards_config),
    )
    .map_err(|e| SessionError::GitError { source: e })?;

    info!(
        event = "session.worktree_created",
        session_id = session_id,
        worktree_path = %worktree.path.display(),
        branch = worktree.branch
    );

    // 5. Launch terminal (I/O)
    let spawn_result =
        terminal::handler::spawn_terminal(&worktree.path, &validated.command, shards_config)
            .map_err(|e| SessionError::TerminalError { source: e })?;

    // 6. Create session record
    let session = Session {
        id: session_id.clone(),
        project_id: project.id,
        branch: validated.name.clone(),
        worktree_path: worktree.path,
        agent: validated.agent.clone(),
        status: SessionStatus::Active,
        created_at: chrono::Utc::now().to_rfc3339(),
        port_range_start: port_start,
        port_range_end: port_end,
        port_count: config.default_port_count,
        process_id: spawn_result.process_id,
        process_name: spawn_result.process_name.clone(),
        process_start_time: spawn_result.process_start_time,
        command: spawn_result.command_executed.trim()
            .is_empty()
            .then(|| format!("{} (command not captured)", validated.agent))
            .unwrap_or_else(|| spawn_result.command_executed.clone()),
    };

    // 7. Save session to file
    operations::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "session.create_completed",
        session_id = session_id,
        branch = validated.name,
        agent = session.agent,
        process_id = session.process_id,
        process_name = ?session.process_name
    );

    Ok(session)
}

pub fn list_sessions() -> Result<Vec<Session>, SessionError> {
    info!(event = "session.list_started");

    let config = Config::new();
    let (sessions, skipped_count) = operations::load_sessions_from_files(&config.sessions_dir())?;

    if skipped_count > 0 {
        tracing::warn!(
            event = "session.list_skipped_sessions",
            skipped_count = skipped_count,
            message = "Some session files were skipped due to errors"
        );
    }

    info!(event = "session.list_completed", count = sessions.len());

    Ok(sessions)
}

pub fn get_session(name: &str) -> Result<Session, SessionError> {
    info!(event = "session.get_started", name = name);

    let config = Config::new();
    let session = operations::load_session_from_file(name, &config.sessions_dir())?;

    info!(
        event = "session.get_completed",
        name = name,
        session_id = session.id
    );

    Ok(session)
}

pub fn destroy_session(name: &str) -> Result<(), SessionError> {
    info!(event = "session.destroy_started", name = name);

    let config = Config::new();

    // 1. Find session by name (branch name)
    let session =
        operations::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    info!(
        event = "session.destroy_found",
        session_id = session.id,
        worktree_path = %session.worktree_path.display(),
        port_range_start = session.port_range_start,
        port_range_end = session.port_range_end,
        process_id = session.process_id
    );

    // 2. Kill process if PID is tracked
    if let Some(pid) = session.process_id {
        info!(event = "session.destroy_kill_started", pid = pid);

        match crate::process::kill_process(
            pid,
            session.process_name.as_deref(),
            session.process_start_time,
        ) {
            Ok(()) => {
                info!(event = "session.destroy_kill_completed", pid = pid);
            }
            Err(crate::process::ProcessError::NotFound { .. }) => {
                info!(event = "session.destroy_kill_already_dead", pid = pid);
            }
            Err(e) => {
                error!(
                    event = "session.destroy_kill_failed",
                    pid = pid,
                    error = %e
                );
                return Err(SessionError::ProcessKillFailed {
                    pid,
                    message: format!(
                        "Process still running. Kill it manually or use --force flag (not yet implemented): {}",
                        e
                    ),
                });
            }
        }
    }

    // 3. Remove git worktree
    git::handler::remove_worktree_by_path(&session.worktree_path)
        .map_err(|e| SessionError::GitError { source: e })?;

    info!(
        event = "session.destroy_worktree_removed",
        session_id = session.id,
        worktree_path = %session.worktree_path.display()
    );

    // 4. Remove session file (automatically frees port range)
    operations::remove_session_file(&config.sessions_dir(), &session.id)?;

    info!(
        event = "session.port_deallocated",
        session_id = session.id,
        port_range_start = session.port_range_start,
        port_range_end = session.port_range_end
    );

    info!(
        event = "session.destroy_completed",
        session_id = session.id,
        name = name
    );

    Ok(())
}

pub fn restart_session(name: &str, agent_override: Option<String>) -> Result<Session, SessionError> {
    let start_time = std::time::Instant::now();
    info!(event = "session.restart_started", name = name, agent_override = ?agent_override);

    let config = Config::new();
    
    // 1. Find session by name (branch name)
    let mut session = operations::find_session_by_name(&config.sessions_dir(), name)?
        .ok_or_else(|| SessionError::NotFound { name: name.to_string() })?;

    info!(
        event = "session.restart_found",
        session_id = session.id,
        current_agent = session.agent,
        process_id = session.process_id
    );

    // 2. Kill process if PID is tracked
    if let Some(pid) = session.process_id {
        info!(event = "session.restart_kill_started", pid = pid);

        match crate::process::kill_process(
            pid,
            session.process_name.as_deref(),
            session.process_start_time,
        ) {
            Ok(()) => info!(event = "session.restart_kill_completed", pid = pid),
            Err(crate::process::ProcessError::NotFound { .. }) => {
                info!(event = "session.restart_kill_process_not_found", pid = pid)
            }
            Err(crate::process::ProcessError::AccessDenied { .. }) => {
                error!(event = "session.restart_kill_access_denied", pid = pid);
                return Err(SessionError::ProcessKillFailed {
                    pid,
                    message: "Access denied - insufficient permissions to kill process".to_string(),
                });
            }
            Err(e) => {
                error!(event = "session.restart_kill_failed", pid = pid, error = %e);
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
            event = "session.restart_worktree_missing",
            session_id = session.id,
            worktree_path = %session.worktree_path.display()
        );
        return Err(SessionError::WorktreeNotFound {
            path: session.worktree_path.clone(),
        });
    }

    // 4. Determine agent and command
    let shards_config = ShardsConfig::load_hierarchy().unwrap_or_default();
    let agent = agent_override.unwrap_or(session.agent.clone());
    let agent_command = shards_config.get_agent_command(&agent);

    info!(
        event = "session.restart_agent_selected",
        session_id = session.id,
        agent = agent,
        command = agent_command
    );

    // 5. Relaunch terminal in existing worktree
    info!(event = "session.restart_spawn_started", worktree_path = %session.worktree_path.display());

    let spawn_result = terminal::handler::spawn_terminal(&session.worktree_path, &agent_command, &shards_config)
        .map_err(|e| SessionError::TerminalError { source: e })?;

    info!(
        event = "session.restart_spawn_completed",
        process_id = spawn_result.process_id,
        process_name = ?spawn_result.process_name
    );

    // Capture process metadata immediately for PID reuse protection
    let (process_name, process_start_time) = spawn_result.process_id
        .and_then(|pid| crate::process::get_process_info(pid).ok())
        .map(|info| (Some(info.name), Some(info.start_time)))
        .unwrap_or((spawn_result.process_name.clone(), spawn_result.process_start_time));

    // 6. Update session with new process info
    session.agent = agent;
    session.process_id = spawn_result.process_id;
    session.process_name = process_name;
    session.process_start_time = process_start_time;
    session.status = SessionStatus::Active;

    // 7. Save updated session to file
    operations::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "session.restart_completed",
        session_id = session.id,
        branch = name,
        agent = session.agent,
        process_id = session.process_id,
        duration_ms = start_time.elapsed().as_millis()
    );

    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_sessions_empty() {
        // Create a temporary directory for this test
        let temp_dir = std::env::temp_dir().join("shards_test_empty_sessions");
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
        let result = destroy_session("non-existent");
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
            std::env::temp_dir().join(format!("shards_test_integration_{}", std::process::id()));
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
            command: "test-command".to_string(),
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
}
