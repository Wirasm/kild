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

fn validate_field_not_empty(field_value: &str, field_name: &str) -> Result<(), SessionError> {
    if field_value.trim().is_empty() {
        return Err(SessionError::InvalidStructure {
            field: format!("{} is empty", field_name),
        });
    }
    Ok(())
}

pub(crate) fn validate_session_structure(session: &Session) -> Result<(), SessionError> {
    validate_field_not_empty(&session.id, "session ID")?;
    validate_field_not_empty(&session.project_id, "project ID")?;
    validate_field_not_empty(&session.branch, "branch name")?;
    validate_field_not_empty(&session.agent, "agent name")?;
    validate_field_not_empty(&session.created_at, "created_at timestamp")?;

    if session.worktree_path.as_os_str().is_empty() {
        return Err(SessionError::InvalidStructure {
            field: "worktree path is empty".to_string(),
        });
    }

    if !session.worktree_path.exists() {
        return Err(SessionError::InvalidStructure {
            field: format!(
                "worktree path does not exist: {}",
                session.worktree_path.display()
            ),
        });
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
        assert!(matches!(
            result.unwrap_err(),
            SessionError::InvalidStructure { field } if field == "session ID is empty"
        ));

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
        assert!(matches!(
            result2.unwrap_err(),
            SessionError::InvalidStructure { field } if field == "worktree path is empty"
        ));

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
        assert!(matches!(
            result3.unwrap_err(),
            SessionError::InvalidStructure { field } if field.contains("worktree path does not exist")
        ));

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_validate_session_structure_all_fields() {
        use std::env;

        let temp_dir = env::temp_dir().join("shards_test_validation_fields");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Test empty project_id
        let session_empty_project = Session {
            id: "test/branch".to_string(),
            project_id: "".to_string(),
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
            last_activity: None,
        };
        let result = validate_session_structure(&session_empty_project);
        assert!(matches!(
            result.unwrap_err(),
            SessionError::InvalidStructure { field } if field == "project ID is empty"
        ));

        // Test empty branch
        let session_empty_branch = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "".to_string(),
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
            last_activity: None,
        };
        let result = validate_session_structure(&session_empty_branch);
        assert!(matches!(
            result.unwrap_err(),
            SessionError::InvalidStructure { field } if field == "branch name is empty"
        ));

        // Test empty agent
        let session_empty_agent = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path: temp_dir.clone(),
            agent: "".to_string(),
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
            last_activity: None,
        };
        let result = validate_session_structure(&session_empty_agent);
        assert!(matches!(
            result.unwrap_err(),
            SessionError::InvalidStructure { field } if field == "agent name is empty"
        ));

        // Test empty created_at
        let session_empty_created_at = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path: temp_dir.clone(),
            agent: "claude".to_string(),
            status: SessionStatus::Active,
            created_at: "".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: "test-command".to_string(),
            last_activity: None,
        };
        let result = validate_session_structure(&session_empty_created_at);
        assert!(matches!(
            result.unwrap_err(),
            SessionError::InvalidStructure { field } if field == "created_at timestamp is empty"
        ));

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
