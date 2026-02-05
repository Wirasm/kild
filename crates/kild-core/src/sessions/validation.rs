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

    // NOTE: We intentionally do NOT check if worktree_path.exists() here.
    // Worktree existence is a runtime state, not a structural property.
    // Sessions with missing worktrees are still valid session files - they
    // just can't be operated on until the worktree issue is resolved.
    // Operation-level validation (open_session, restart_session, etc.) handles this.

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
        let temp_dir = env::temp_dir().join("kild_test_validation");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Valid session with existing worktree path
        let valid_session = Session::new(
            "test/branch".to_string(),
            "test".to_string(),
            "branch".to_string(),
            temp_dir.clone(),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
        );
        assert!(validate_session_structure(&valid_session).is_ok());

        // Invalid session - empty id
        let invalid_session = Session::new(
            "".to_string(),
            "test".to_string(),
            "branch".to_string(),
            temp_dir.clone(),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
        );
        let result = validate_session_structure(&invalid_session);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SessionError::InvalidStructure { field } if field == "session ID is empty"
        ));

        // Invalid session - empty worktree path
        let invalid_session2 = Session::new(
            "test/branch".to_string(),
            "test".to_string(),
            "branch".to_string(),
            PathBuf::new(),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
        );
        let result2 = validate_session_structure(&invalid_session2);
        assert!(result2.is_err());
        assert!(matches!(
            result2.unwrap_err(),
            SessionError::InvalidStructure { field } if field == "worktree path is empty"
        ));

        // Sessions with non-existing worktrees should pass structural validation.
        // Worktree existence is checked at operation time, not during loading.
        let nonexistent_path = temp_dir.join("nonexistent");
        let session_missing_worktree = Session::new(
            "test/branch".to_string(),
            "test".to_string(),
            "branch".to_string(),
            nonexistent_path.clone(),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
        );
        assert!(validate_session_structure(&session_missing_worktree).is_ok());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_validate_session_structure_all_fields() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_validation_fields");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Test empty project_id
        let session_empty_project = Session::new(
            "test/branch".to_string(),
            "".to_string(),
            "branch".to_string(),
            temp_dir.clone(),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            None,
            None,
            vec![],
        );
        let result = validate_session_structure(&session_empty_project);
        assert!(matches!(
            result.unwrap_err(),
            SessionError::InvalidStructure { field } if field == "project ID is empty"
        ));

        // Test empty branch
        let session_empty_branch = Session::new(
            "test/branch".to_string(),
            "test".to_string(),
            "".to_string(),
            temp_dir.clone(),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            None,
            None,
            vec![],
        );
        let result = validate_session_structure(&session_empty_branch);
        assert!(matches!(
            result.unwrap_err(),
            SessionError::InvalidStructure { field } if field == "branch name is empty"
        ));

        // Test empty agent
        let session_empty_agent = Session::new(
            "test/branch".to_string(),
            "test".to_string(),
            "branch".to_string(),
            temp_dir.clone(),
            "".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            None,
            None,
            vec![],
        );
        let result = validate_session_structure(&session_empty_agent);
        assert!(matches!(
            result.unwrap_err(),
            SessionError::InvalidStructure { field } if field == "agent name is empty"
        ));

        // Test empty created_at
        let session_empty_created_at = Session::new(
            "test/branch".to_string(),
            "test".to_string(),
            "branch".to_string(),
            temp_dir.clone(),
            "claude".to_string(),
            SessionStatus::Active,
            "".to_string(),
            0,
            0,
            0,
            None,
            None,
            vec![],
        );
        let result = validate_session_structure(&session_empty_created_at);
        assert!(matches!(
            result.unwrap_err(),
            SessionError::InvalidStructure { field } if field == "created_at timestamp is empty"
        ));

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
