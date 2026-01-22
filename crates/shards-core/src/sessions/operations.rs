use crate::sessions::{errors::SessionError, types::*};
use std::fs;
use std::path::Path;
use tracing::warn;

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

pub fn generate_session_id(project_id: &str, branch: &str) -> String {
    format!("{}/{}", project_id, branch)
}

pub fn calculate_port_range(session_index: u32) -> (u16, u16) {
    let base_port = 3000u16 + (session_index as u16 * 100);
    (base_port, base_port + 99)
}

pub fn allocate_port_range(
    sessions_dir: &Path,
    port_count: u16,
    base_port: u16,
) -> Result<(u16, u16), SessionError> {
    let (existing_sessions, _) = load_sessions_from_files(sessions_dir)?;

    // Find next available port range
    let (start_port, end_port) =
        find_next_available_range(&existing_sessions, port_count, base_port)?;

    Ok((start_port, end_port))
}

pub fn find_next_available_range(
    existing_sessions: &[Session],
    port_count: u16,
    base_port: u16,
) -> Result<(u16, u16), SessionError> {
    if port_count == 0 {
        return Err(SessionError::InvalidPortCount);
    }

    // Collect all allocated port ranges
    let mut allocated_ranges: Vec<(u16, u16)> = existing_sessions
        .iter()
        .map(|s| (s.port_range_start, s.port_range_end))
        .collect();

    // Sort by start port
    allocated_ranges.sort_by_key(|&(start, _)| start);

    // Try to find a gap starting from base_port
    let mut current_port = base_port;

    for &(allocated_start, allocated_end) in &allocated_ranges {
        let proposed_end = current_port
            .checked_add(port_count)
            .and_then(|sum| sum.checked_sub(1))
            .ok_or(SessionError::PortRangeExhausted)?;

        // Check if proposed range fits before this allocated range
        if proposed_end < allocated_start {
            return Ok((current_port, proposed_end));
        }

        // Move past this allocated range
        current_port = allocated_end + 1;
    }

    // Check if we can allocate after all existing ranges
    let proposed_end = current_port
        .checked_add(port_count)
        .and_then(|sum| sum.checked_sub(1))
        .ok_or(SessionError::PortRangeExhausted)?;

    Ok((current_port, proposed_end))
}

pub fn is_port_range_available(
    existing_sessions: &[Session],
    start_port: u16,
    end_port: u16,
) -> bool {
    for session in existing_sessions {
        // Check for overlap: ranges overlap if start1 <= end2 && start2 <= end1
        if start_port <= session.port_range_end && session.port_range_start <= end_port {
            return false;
        }
    }
    true
}

pub fn generate_port_env_vars(session: &Session) -> Vec<(String, String)> {
    vec![
        (
            "SHARD_PORT_RANGE_START".to_string(),
            session.port_range_start.to_string(),
        ),
        (
            "SHARD_PORT_RANGE_END".to_string(),
            session.port_range_end.to_string(),
        ),
        (
            "SHARD_PORT_COUNT".to_string(),
            session.port_count.to_string(),
        ),
    ]
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

pub fn ensure_sessions_directory(sessions_dir: &Path) -> Result<(), SessionError> {
    fs::create_dir_all(sessions_dir).map_err(|e| SessionError::IoError { source: e })?;
    Ok(())
}

pub fn save_session_to_file(session: &Session, sessions_dir: &Path) -> Result<(), SessionError> {
    let session_file = sessions_dir.join(format!("{}.json", session.id.replace('/', "_")));
    let session_json =
        serde_json::to_string_pretty(session).map_err(|e| SessionError::IoError {
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?;

    // Write atomically by writing to temp file first, then renaming
    let temp_file = session_file.with_extension("json.tmp");

    // Write to temp file
    if let Err(e) = fs::write(&temp_file, session_json) {
        // Clean up temp file if write failed
        let _ = fs::remove_file(&temp_file);
        return Err(SessionError::IoError { source: e });
    }

    // Rename temp file to final location
    if let Err(e) = fs::rename(&temp_file, &session_file) {
        // Clean up temp file if rename failed
        let _ = fs::remove_file(&temp_file);
        return Err(SessionError::IoError { source: e });
    }

    Ok(())
}

pub fn load_sessions_from_files(
    sessions_dir: &Path,
) -> Result<(Vec<Session>, usize), SessionError> {
    let mut sessions = Vec::new();
    let mut skipped_count = 0;

    // Return empty list if sessions directory doesn't exist
    if !sessions_dir.exists() {
        return Ok((sessions, skipped_count));
    }

    let entries = fs::read_dir(sessions_dir).map_err(|e| SessionError::IoError { source: e })?;

    for entry in entries {
        let entry = entry.map_err(|e| SessionError::IoError { source: e })?;
        let path = entry.path();

        // Only process .json files
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            match fs::read_to_string(&path) {
                Ok(content) => {
                    match serde_json::from_str::<Session>(&content) {
                        Ok(session) => {
                            // Validate session structure
                            match validate_session_structure(&session) {
                                Ok(()) => {
                                    sessions.push(session);
                                }
                                Err(validation_error) => {
                                    skipped_count += 1;
                                    tracing::warn!(
                                        event = "session.load_invalid_structure",
                                        file = %path.display(),
                                        worktree_path = %session.worktree_path.display(),
                                        validation_error = validation_error,
                                        message = "Session file has invalid structure, skipping"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            skipped_count += 1;
                            tracing::warn!(
                                event = "session.load_invalid_json",
                                file = %path.display(),
                                error = %e,
                                message = "Failed to parse session JSON, skipping"
                            );
                        }
                    }
                }
                Err(e) => {
                    skipped_count += 1;
                    tracing::warn!(
                        event = "session.load_read_error",
                        file = %path.display(),
                        error = %e,
                        message = "Failed to read session file, skipping"
                    );
                }
            }
        }
    }

    Ok((sessions, skipped_count))
}

pub fn load_session_from_file(name: &str, sessions_dir: &Path) -> Result<Session, SessionError> {
    // Find session by branch name
    let session =
        find_session_by_name(sessions_dir, name)?.ok_or_else(|| SessionError::NotFound {
            name: name.to_string(),
        })?;

    Ok(session)
}

fn validate_session_structure(session: &Session) -> Result<(), String> {
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

pub fn find_session_by_name(
    sessions_dir: &Path,
    name: &str,
) -> Result<Option<Session>, SessionError> {
    let (sessions, _) = load_sessions_from_files(sessions_dir)?;

    // Find session by branch name (the "name" parameter refers to branch name)
    for session in sessions {
        if session.branch == name {
            return Ok(Some(session));
        }
    }

    Ok(None)
}

pub fn remove_session_file(sessions_dir: &Path, session_id: &str) -> Result<(), SessionError> {
    let session_file = sessions_dir.join(format!("{}.json", session_id.replace('/', "_")));

    if session_file.exists() {
        fs::remove_file(&session_file).map_err(|e| SessionError::IoError { source: e })?;
    } else {
        warn!(
            "Attempted to remove session file that doesn't exist: {} - possible state inconsistency",
            session_file.display()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_generate_session_id() {
        let id = generate_session_id("my-project", "feature-branch");
        assert_eq!(id, "my-project/feature-branch");
    }

    #[test]
    fn test_calculate_port_range() {
        assert_eq!(calculate_port_range(0), (3000, 3099));
        assert_eq!(calculate_port_range(1), (3100, 3199));
        assert_eq!(calculate_port_range(5), (3500, 3599));
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
    fn test_ensure_sessions_directory() {
        use std::env;
        use std::path::PathBuf;

        let temp_dir = env::temp_dir().join("shards_test_sessions");

        // Clean up if exists
        let _ = std::fs::remove_dir_all(&temp_dir);

        // Should create directory
        assert!(ensure_sessions_directory(&temp_dir).is_ok());
        assert!(temp_dir.exists());

        // Should not error if directory already exists
        assert!(ensure_sessions_directory(&temp_dir).is_ok());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_session_to_file() {
        use std::env;
        use std::path::PathBuf;

        let temp_dir = env::temp_dir().join("shards_test_save_session");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path: worktree_path,
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

        // Save session
        assert!(save_session_to_file(&session, &temp_dir).is_ok());

        // Check file exists with correct name (/ replaced with _)
        let session_file = temp_dir.join("test_branch.json");
        assert!(session_file.exists());

        // Verify content
        let content = std::fs::read_to_string(&session_file).unwrap();
        let loaded_session: Session = serde_json::from_str(&content).unwrap();
        assert_eq!(loaded_session, session);

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_session_atomic_write_temp_cleanup() {
        use std::env;
        use std::path::PathBuf;

        let temp_dir = env::temp_dir().join("shards_test_atomic_write");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session {
            id: "test/atomic".to_string(),
            project_id: "test".to_string(),
            branch: "atomic".to_string(),
            worktree_path: worktree_path,
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

        // Save session
        assert!(save_session_to_file(&session, &temp_dir).is_ok());

        // Verify temp file is cleaned up after successful write
        let temp_file = temp_dir.join("test_atomic.json.tmp");
        assert!(
            !temp_file.exists(),
            "Temp file should be cleaned up after successful write"
        );

        // Verify final file exists
        let session_file = temp_dir.join("test_atomic.json");
        assert!(session_file.exists(), "Final session file should exist");

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_session_atomic_behavior() {
        use std::env;
        use std::path::PathBuf;

        let temp_dir = env::temp_dir().join("shards_test_atomic_behavior");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session {
            id: "test/atomic-behavior".to_string(),
            project_id: "test".to_string(),
            branch: "atomic-behavior".to_string(),
            worktree_path: worktree_path,
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

        let session_file = temp_dir.join("test_atomic-behavior.json");

        // Create existing file with different content
        std::fs::write(&session_file, "old content").unwrap();

        // Save session atomically
        assert!(save_session_to_file(&session, &temp_dir).is_ok());

        // Verify file was replaced atomically (no partial writes)
        let content = std::fs::read_to_string(&session_file).unwrap();
        assert!(content.contains("test/atomic-behavior"));
        assert!(!content.contains("old content"));

        // Verify it's valid JSON
        let loaded_session: Session = serde_json::from_str(&content).unwrap();
        assert_eq!(loaded_session, session);

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_session_temp_file_cleanup_on_failure() {
        use std::env;
        use std::path::PathBuf;

        let temp_dir = env::temp_dir().join("shards_test_temp_cleanup");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session {
            id: "test/cleanup".to_string(),
            project_id: "test".to_string(),
            branch: "cleanup".to_string(),
            worktree_path: worktree_path,
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

        // Create a directory where the final file should be to force rename failure
        let session_file = temp_dir.join("test_cleanup.json");
        std::fs::create_dir_all(&session_file).unwrap(); // Create as directory to force rename failure

        // Attempt to save session - should fail due to rename failure
        let result = save_session_to_file(&session, &temp_dir);
        assert!(result.is_err(), "Save should fail when rename fails");

        // Verify temp file is cleaned up after failure
        let temp_file = temp_dir.join("test_cleanup.json.tmp");
        assert!(
            !temp_file.exists(),
            "Temp file should be cleaned up after rename failure"
        );

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_sessions_from_files() {
        use std::env;
        use std::path::PathBuf;

        let temp_dir = env::temp_dir().join("shards_test_load_sessions");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Test empty directory
        let (sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();
        assert_eq!(sessions.len(), 0);
        assert_eq!(skipped, 0);

        // Create test sessions with existing worktree paths
        let worktree1 = temp_dir.join("worktree1");
        let worktree2 = temp_dir.join("worktree2");
        std::fs::create_dir_all(&worktree1).unwrap();
        std::fs::create_dir_all(&worktree2).unwrap();

        let session1 = Session {
            id: "test/branch1".to_string(),
            project_id: "test".to_string(),
            branch: "branch1".to_string(),
            worktree_path: worktree1,
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

        let session2 = Session {
            id: "test/branch2".to_string(),
            project_id: "test".to_string(),
            branch: "branch2".to_string(),
            worktree_path: worktree2,
            agent: "kiro".to_string(),
            status: SessionStatus::Stopped,
            created_at: "2024-01-02T00:00:00Z".to_string(),
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

        // Save sessions
        save_session_to_file(&session1, &temp_dir).unwrap();
        save_session_to_file(&session2, &temp_dir).unwrap();

        // Load sessions
        let (loaded_sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();
        assert_eq!(loaded_sessions.len(), 2);
        assert_eq!(skipped, 0);

        // Verify sessions (order might vary)
        let ids: Vec<String> = loaded_sessions.iter().map(|s| s.id.clone()).collect();
        assert!(ids.contains(&"test/branch1".to_string()));
        assert!(ids.contains(&"test/branch2".to_string()));

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_sessions_nonexistent_directory() {
        use std::env;

        let nonexistent_dir = env::temp_dir().join("shards_test_nonexistent");
        let _ = std::fs::remove_dir_all(&nonexistent_dir);

        let (sessions, skipped) = load_sessions_from_files(&nonexistent_dir).unwrap();
        assert_eq!(sessions.len(), 0);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_find_session_by_name() {
        use std::env;
        use std::path::PathBuf;

        let temp_dir = env::temp_dir().join("shards_test_find_session");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session {
            id: "test/feature-branch".to_string(),
            project_id: "test".to_string(),
            branch: "feature-branch".to_string(),
            worktree_path: worktree_path,
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

        // Save session
        save_session_to_file(&session, &temp_dir).unwrap();

        // Find by branch name
        let found = find_session_by_name(&temp_dir, "feature-branch").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "test/feature-branch");

        // Try to find non-existent session
        let not_found = find_session_by_name(&temp_dir, "non-existent").unwrap();
        assert!(not_found.is_none());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_remove_session_file() {
        use std::env;
        use std::path::PathBuf;

        let temp_dir = env::temp_dir().join("shards_test_remove_session");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path: worktree_path,
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

        // Save session
        save_session_to_file(&session, &temp_dir).unwrap();

        let session_file = temp_dir.join("test_branch.json");
        assert!(session_file.exists());

        // Remove session file
        remove_session_file(&temp_dir, &session.id).unwrap();
        assert!(!session_file.exists());

        // Removing non-existent file should not error
        assert!(remove_session_file(&temp_dir, "non-existent").is_ok());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_sessions_with_invalid_files() {
        use std::env;
        use std::path::PathBuf;

        let temp_dir = env::temp_dir().join("shards_test_invalid_files");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create a valid session with existing worktree path
        let worktree_path = temp_dir.join("valid_worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let valid_session = Session {
            id: "test/valid".to_string(),
            project_id: "test".to_string(),
            branch: "valid".to_string(),
            worktree_path: worktree_path,
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
        save_session_to_file(&valid_session, &temp_dir).unwrap();

        // Create invalid JSON file
        let invalid_json_file = temp_dir.join("invalid.json");
        std::fs::write(&invalid_json_file, "{ invalid json }").unwrap();

        // Create file with invalid session structure (missing required fields)
        let invalid_structure_file = temp_dir.join("invalid_structure.json");
        std::fs::write(
            &invalid_structure_file,
            r#"{"id": "", "project_id": "test"}"#,
        )
        .unwrap();

        // Load sessions - should only return the valid one
        let (sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "test/valid");
        assert_eq!(skipped, 2); // invalid JSON + invalid structure

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_validate_session_structure() {
        use std::env;
        use std::path::PathBuf;

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
