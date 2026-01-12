use tracing::info;

use crate::core::config::Config;
use crate::git;
use crate::sessions::{errors::SessionError, operations, types::*};
use crate::terminal;

pub fn create_session(request: CreateSessionRequest) -> Result<Session, SessionError> {
    let agent = request.agent();
    let agent_command = operations::get_agent_command(&agent);

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
    
    let worktree = git::handler::create_worktree(&config.shards_dir, &project, &validated.name)
        .map_err(|e| SessionError::GitError { source: e })?;

    info!(
        event = "session.worktree_created",
        session_id = session_id,
        worktree_path = %worktree.path.display(),
        branch = worktree.branch
    );

    // 5. Launch terminal (I/O)
    let _spawn_result = terminal::handler::spawn_terminal(&worktree.path, &validated.command)
        .map_err(|e| SessionError::TerminalError { source: e })?;

    // 6. Create session record
    let session = Session {
        id: session_id.clone(),
        project_id: project.id,
        branch: validated.name.clone(),
        worktree_path: worktree.path,
        agent: validated.agent,
        status: SessionStatus::Active,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    // 7. Save session to file
    operations::save_session_to_file(&session, &config.sessions_dir())?;

    info!(
        event = "session.create_completed",
        session_id = session_id,
        branch = validated.name,
        agent = session.agent
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

pub fn destroy_session(name: &str) -> Result<(), SessionError> {
    info!(event = "session.destroy_started", name = name);

    let config = Config::new();
    
    // 1. Find session by name (branch name)
    let session = operations::find_session_by_name(&config.sessions_dir(), name)?
        .ok_or_else(|| SessionError::NotFound { name: name.to_string() })?;

    info!(
        event = "session.destroy_found",
        session_id = session.id,
        worktree_path = %session.worktree_path.display()
    );

    // 2. Remove git worktree
    git::handler::remove_worktree_by_path(&session.worktree_path)
        .map_err(|e| SessionError::GitError { source: e })?;

    info!(
        event = "session.destroy_worktree_removed",
        session_id = session.id,
        worktree_path = %session.worktree_path.display()
    );

    // 3. Remove session file
    operations::remove_session_file(&config.sessions_dir(), &session.id)?;

    info!(
        event = "session.destroy_completed",
        session_id = session.id,
        name = name
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_sessions_empty() {
        // This test now verifies that list_sessions handles empty/nonexistent sessions directory
        let result = list_sessions();
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
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
        use crate::core::config::Config;

        // This test verifies session persistence across operations
        // by testing the file-based storage directly
        
        // Setup temporary sessions directory
        let temp_dir = std::env::temp_dir().join(format!("shards_test_{}", std::process::id()));
        let sessions_dir = temp_dir.join("sessions");
        fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");

        // Override sessions directory for test
        unsafe {
            std::env::set_var("SHARDS_SESSIONS_DIR", sessions_dir.to_str().unwrap());
        }

        let result = std::panic::catch_unwind(|| {
            let config = Config::new();
            
            // Test session persistence workflow using operations directly
            // This tests the core persistence logic without git/terminal dependencies
            
            // 1. Create a test session manually
            use crate::sessions::types::{Session, SessionStatus};
            use crate::sessions::operations;
            
            let session = Session {
                id: "test-project_test-branch".to_string(),
                project_id: "test-project".to_string(),
                branch: "test-branch".to_string(),
                worktree_path: temp_dir.join("worktree").to_path_buf(),
                agent: "test-agent".to_string(),
                status: SessionStatus::Active,
                created_at: chrono::Utc::now().to_rfc3339(),
            };

            // Create worktree directory so validation passes
            fs::create_dir_all(&session.worktree_path).expect("Failed to create worktree dir");

            // 2. Save session to file
            operations::save_session_to_file(&session, &config.sessions_dir())
                .expect("Failed to save session");

            // 3. List sessions - should contain our session
            let (sessions, skipped) = operations::load_sessions_from_files(&config.sessions_dir())
                .expect("Failed to load sessions");
            assert_eq!(sessions.len(), 1);
            assert_eq!(skipped, 0);
            assert_eq!(sessions[0].id, session.id);
            assert_eq!(sessions[0].branch, "test-branch");

            // 4. Find session by name
            let found_session = operations::find_session_by_name(&config.sessions_dir(), "test-branch")
                .expect("Failed to find session")
                .expect("Session not found");
            assert_eq!(found_session.id, session.id);

            // 5. Remove session file
            operations::remove_session_file(&config.sessions_dir(), &session.id)
                .expect("Failed to remove session");

            // 6. List sessions - should be empty
            let (sessions_after, _) = operations::load_sessions_from_files(&config.sessions_dir())
                .expect("Failed to load sessions after removal");
            assert_eq!(sessions_after.len(), 0);

            // 7. Try to find removed session - should return None
            let not_found = operations::find_session_by_name(&config.sessions_dir(), "test-branch")
                .expect("Failed to search for removed session");
            assert!(not_found.is_none());
        });

        // Cleanup
        unsafe {
            std::env::remove_var("SHARDS_SESSIONS_DIR");
        }
        let _ = fs::remove_dir_all(&temp_dir);

        // Check if test passed
        result.expect("Integration test failed");
    }
}
