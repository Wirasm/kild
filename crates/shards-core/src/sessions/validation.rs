//! Session input validation
//!
//! Validates session requests, branch names, and session structure.

use crate::sessions::{errors::SessionError, types::*};

pub fn validate_session_request(
    name: &str,
    command: &str,
    agent: &str,
) -> Result<ValidatedRequest, SessionError> {
    if name.trim().is_empty() {
        return Err(SessionError::InvalidName);
    }

    if command.trim().is_empty() {
        return Err(SessionError::InvalidCommand);
    }

    Ok(ValidatedRequest {
        name: name.trim().to_string(),
        command: command.trim().to_string(),
        agent: agent.to_string(),
    })
}

pub fn validate_branch_name(branch: &str) -> Result<String, SessionError> {
    let trimmed = branch.trim();

    if trimmed.is_empty() {
        return Err(SessionError::InvalidName);
    }

    // Basic git branch name validation
    if trimmed.contains("..") || trimmed.starts_with('-') || trimmed.contains(' ') {
        return Err(SessionError::InvalidName);
    }

    Ok(trimmed.to_string())
}

pub(crate) fn validate_session_structure(session: &Session) -> Result<(), String> {
    // Validate required fields are not empty
    if session.id.trim().is_empty() {
        return Err("session ID is empty".to_string());
    }
    if session.project_id.trim().is_empty() {
        return Err("project ID is empty".to_string());
    }
    if session.branch.trim().is_empty() {
        return Err("branch name is empty".to_string());
    }
    if session.agent.trim().is_empty() {
        return Err("agent name is empty".to_string());
    }
    if session.created_at.trim().is_empty() {
        return Err("created_at timestamp is empty".to_string());
    }
    if session.worktree_path.as_os_str().is_empty() {
        return Err("worktree path is empty".to_string());
    }
    if !session.worktree_path.exists() {
        return Err(format!(
            "worktree path does not exist: {}",
            session.worktree_path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_validate_session_request_success() {
        let result = validate_session_request("test", "echo hello", "claude");
        assert!(result.is_ok());

        let validated = result.unwrap();
        assert_eq!(validated.name, "test");
        assert_eq!(validated.command, "echo hello");
        assert_eq!(validated.agent, "claude");
    }

    #[test]
    fn test_validate_session_request_empty_name() {
        let result = validate_session_request("", "echo hello", "claude");
        assert!(matches!(result, Err(SessionError::InvalidName)));
    }

    #[test]
    fn test_validate_session_request_empty_command() {
        let result = validate_session_request("test", "", "claude");
        assert!(matches!(result, Err(SessionError::InvalidCommand)));
    }

    #[test]
    fn test_validate_session_request_whitespace() {
        let result = validate_session_request("  test  ", "  echo hello  ", "claude");
        assert!(result.is_ok());

        let validated = result.unwrap();
        assert_eq!(validated.name, "test");
        assert_eq!(validated.command, "echo hello");
    }

    #[test]
    fn test_validate_branch_name() {
        assert!(validate_branch_name("feature-branch").is_ok());
        assert!(validate_branch_name("feat/auth").is_ok());

        assert!(validate_branch_name("").is_err());
        assert!(validate_branch_name("  ").is_err());
        assert!(validate_branch_name("branch..name").is_err());
        assert!(validate_branch_name("-branch").is_err());
        assert!(validate_branch_name("branch name").is_err());
    }

    #[test]
    fn test_validate_session_structure() {
        use std::env;

        // Create a temporary directory that exists
        let temp_dir = env::temp_dir().join("shards_test_validation");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Valid session with existing worktree path
        let valid_session = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path: temp_dir.clone(),
            agent: "claude".to_string(),
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
            command: "test-command".to_string(),
            last_activity: Some("2024-01-01T00:00:00Z".to_string()),
        };
        assert!(validate_session_structure(&valid_session).is_ok());

        // Invalid session - empty id
        let invalid_session = Session {
            id: "".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path: temp_dir.clone(),
            agent: "claude".to_string(),
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
            command: "test-command".to_string(),
            last_activity: Some("2024-01-01T00:00:00Z".to_string()),
        };
        let result = validate_session_structure(&invalid_session);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "session ID is empty");

        // Invalid session - empty worktree path
        let invalid_session2 = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path: PathBuf::new(),
            agent: "claude".to_string(),
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
            command: "test-command".to_string(),
            last_activity: Some("2024-01-01T00:00:00Z".to_string()),
        };
        let result2 = validate_session_structure(&invalid_session2);
        assert!(result2.is_err());
        assert_eq!(result2.unwrap_err(), "worktree path is empty");

        // Invalid session - non-existing worktree path
        let nonexistent_path = temp_dir.join("nonexistent");
        let invalid_session3 = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path: nonexistent_path.clone(),
            agent: "claude".to_string(),
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
            command: "test-command".to_string(),
            last_activity: Some("2024-01-01T00:00:00Z".to_string()),
        };
        let result3 = validate_session_structure(&invalid_session3);
        assert!(result3.is_err());
        assert!(
            result3
                .unwrap_err()
                .contains("worktree path does not exist")
        );

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
