use tracing::{error, info};

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

    // 3. Check if session already exists (would need database here)
    let session_id = operations::generate_session_id(&project.id, &validated.name);

    // TODO: Check database for existing session
    // if database::session_exists(&session_id)? {
    //     return Err(SessionError::AlreadyExists { name: validated.name });
    // }

    // 4. Create worktree (I/O)
    let config = Config::new();
    
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

    // TODO: Implement database query
    // For now, return empty list
    let sessions = Vec::new();

    info!(event = "session.list_completed", count = sessions.len());

    Ok(sessions)
}

pub fn destroy_session(name: &str) -> Result<(), SessionError> {
    info!(event = "session.destroy_started", name = name);

    // TODO: Implement session destruction
    // 1. Find session in database
    // 2. Remove worktree
    // 3. Update database record

    error!(
        event = "session.destroy_failed",
        name = name,
        error = "not implemented"
    );

    Err(SessionError::NotFound {
        name: name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_sessions_empty() {
        let result = list_sessions();
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_destroy_session_not_implemented() {
        let result = destroy_session("test");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    // Note: create_session test would require git repository setup
    // Better suited for integration tests
}
