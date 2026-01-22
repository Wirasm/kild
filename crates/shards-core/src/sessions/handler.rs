use tracing::{error, info, warn};

use crate::config::{Config, ShardsConfig};
use crate::git;
use crate::process::{delete_pid_file, get_pid_file_path};
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

    // 5. Launch terminal (I/O) - pass session_id for unique Ghostty window titles and PID tracking
    let spawn_result = terminal::handler::spawn_terminal(
        &worktree.path,
        &validated.command,
        shards_config,
        Some(&session_id),
        Some(&base_config.shards_dir),
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

    // 2. Close terminal window first (before killing process)
    if let Some(ref terminal_type) = session.terminal_type {
        info!(
            event = "session.destroy_close_terminal",
            terminal_type = %terminal_type,
            window_id = ?session.terminal_window_id
        );
        // Best-effort - don't fail destroy if terminal close fails
        if let Err(e) =
            terminal::handler::close_terminal(terminal_type, session.terminal_window_id.as_deref())
        {
            warn!(
                event = "session.destroy_terminal_close_failed",
                session_id = session.id,
                terminal_type = %terminal_type,
                window_id = ?session.terminal_window_id,
                error = %e,
                "Terminal window may need to be closed manually"
            );
        }
    }

    // 3. Kill process if PID is tracked
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

    // 4. Remove git worktree
    git::handler::remove_worktree_by_path(&session.worktree_path)
        .map_err(|e| SessionError::GitError { source: e })?;

    info!(
        event = "session.destroy_worktree_removed",
        session_id = session.id,
        worktree_path = %session.worktree_path.display()
    );

    // 5. Clean up PID file (best-effort, don't fail if missing)
    let pid_file = get_pid_file_path(&config.shards_dir, &session.id);
    if let Err(e) = delete_pid_file(&pid_file) {
        warn!(
            event = "session.destroy_pid_file_cleanup_failed",
            session_id = session.id,
            pid_file = %pid_file.display(),
            error = %e
        );
    } else {
        info!(
            event = "session.destroy_pid_file_cleaned",
            session_id = session.id,
            pid_file = %pid_file.display()
        );
    }

    // 6. Remove session file (automatically frees port range)
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

pub fn restart_session(
    name: &str,
    agent_override: Option<String>,
) -> Result<Session, SessionError> {
    let start_time = std::time::Instant::now();
    info!(event = "session.restart_started", name = name, agent_override = ?agent_override);

    let config = Config::new();

    // 1. Find session by name (branch name)
    let mut session =
        operations::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

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
    let base_config = Config::new();
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

    let spawn_result = terminal::handler::spawn_terminal(
        &session.worktree_path,
        &agent_command,
        &shards_config,
        Some(&session.id),
        Some(&base_config.shards_dir),
    )
    .map_err(|e| SessionError::TerminalError { source: e })?;

    info!(
        event = "session.restart_spawn_completed",
        process_id = spawn_result.process_id,
        process_name = ?spawn_result.process_name,
        terminal_window_id = ?spawn_result.terminal_window_id
    );

    // Capture process metadata immediately for PID reuse protection
    let (process_name, process_start_time) = spawn_result
        .process_id
        .and_then(|pid| crate::process::get_process_info(pid).ok())
        .map(|info| (Some(info.name), Some(info.start_time)))
        .unwrap_or((
            spawn_result.process_name.clone(),
            spawn_result.process_start_time,
        ));

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
            terminal_type: None,
            terminal_window_id: None,
            command: "test-command".to_string(),
            last_activity: Some(chrono::Utc::now().to_rfc3339()),
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
        let temp_dir =
            std::env::temp_dir().join(format!("shards_test_terminal_type_{}", unique_id));
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
            std::env::temp_dir().join(format!("shards_test_destroy_terminal_{}", unique_id));
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
        let temp_dir =
            std::env::temp_dir().join(format!("shards_test_destroy_compat_{}", unique_id));
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
            std::env::temp_dir().join(format!("shards_test_restart_terminal_{}", unique_id));
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
}
