//! Session file persistence
//!
//! Handles reading/writing session data to disk with atomic operations.

use crate::sessions::{errors::SessionError, types::*};
use std::fs;
use std::path::Path;

pub fn ensure_sessions_directory(sessions_dir: &Path) -> Result<(), SessionError> {
    fs::create_dir_all(sessions_dir).map_err(|e| SessionError::IoError { source: e })?;
    Ok(())
}

fn cleanup_temp_file(temp_file: &Path, original_error: &std::io::Error) {
    if let Err(cleanup_err) = fs::remove_file(temp_file) {
        tracing::warn!(
            event = "core.session.temp_file_cleanup_failed",
            temp_file = %temp_file.display(),
            original_error = %original_error,
            cleanup_error = %cleanup_err,
            message = "Failed to clean up temp file after operation error"
        );
    }
}

pub fn save_session_to_file(session: &Session, sessions_dir: &Path) -> Result<(), SessionError> {
    let session_file = sessions_dir.join(format!("{}.json", session.id.replace('/', "_")));
    let session_json = serde_json::to_string_pretty(session).map_err(|e| {
        tracing::error!(
            event = "core.session.serialization_failed",
            session_id = %session.id,
            error = %e,
            message = "Failed to serialize session to JSON"
        );
        SessionError::IoError {
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        }
    })?;

    let temp_file = session_file.with_extension("json.tmp");

    // Write to temp file
    if let Err(e) = fs::write(&temp_file, &session_json) {
        cleanup_temp_file(&temp_file, &e);
        return Err(SessionError::IoError { source: e });
    }

    // Rename temp file to final location
    if let Err(e) = fs::rename(&temp_file, &session_file) {
        cleanup_temp_file(&temp_file, &e);
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
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) => {
                skipped_count += 1;
                tracing::warn!(
                    event = "core.session.load_read_error",
                    file = %path.display(),
                    error = %e,
                    message = "Failed to read session file, skipping"
                );
                continue;
            }
        };

        let session = match serde_json::from_str::<Session>(&content) {
            Ok(session) => session,
            Err(e) => {
                skipped_count += 1;
                tracing::warn!(
                    event = "core.session.load_invalid_json",
                    file = %path.display(),
                    error = %e,
                    message = "Failed to parse session JSON, skipping"
                );
                continue;
            }
        };

        if let Err(validation_error) = super::validation::validate_session_structure(&session) {
            skipped_count += 1;
            tracing::warn!(
                event = "core.session.load_invalid_structure",
                file = %path.display(),
                worktree_path = %session.worktree_path.display(),
                validation_error = %validation_error,
                message = "Session file has invalid structure, skipping"
            );
            continue;
        }

        sessions.push(session);
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
        tracing::warn!(
            event = "core.session.remove_nonexistent_file",
            session_id = %session_id,
            file = %session_file.display(),
            message = "Attempted to remove session file that doesn't exist - possible state inconsistency"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_sessions_directory() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_sessions");

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

        let temp_dir = env::temp_dir().join("kild_test_save_session");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path,
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
            note: None,
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

        let temp_dir = env::temp_dir().join("kild_test_atomic_write");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session {
            id: "test/atomic".to_string(),
            project_id: "test".to_string(),
            branch: "atomic".to_string(),
            worktree_path,
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
            note: None,
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

        let temp_dir = env::temp_dir().join("kild_test_atomic_behavior");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session {
            id: "test/atomic-behavior".to_string(),
            project_id: "test".to_string(),
            branch: "atomic-behavior".to_string(),
            worktree_path,
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
            note: None,
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

        let temp_dir = env::temp_dir().join("kild_test_temp_cleanup");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session {
            id: "test/cleanup".to_string(),
            project_id: "test".to_string(),
            branch: "cleanup".to_string(),
            worktree_path,
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
            note: None,
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

        let temp_dir = env::temp_dir().join("kild_test_load_sessions");
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
            note: None,
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
            note: None,
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

        let nonexistent_dir = env::temp_dir().join("kild_test_nonexistent");
        let _ = std::fs::remove_dir_all(&nonexistent_dir);

        let (sessions, skipped) = load_sessions_from_files(&nonexistent_dir).unwrap();
        assert_eq!(sessions.len(), 0);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_find_session_by_name() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_find_session");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session {
            id: "test/feature-branch".to_string(),
            project_id: "test".to_string(),
            branch: "feature-branch".to_string(),
            worktree_path,
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
            note: None,
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

        let temp_dir = env::temp_dir().join("kild_test_remove_session");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path,
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
            note: None,
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

        let temp_dir = env::temp_dir().join("kild_test_invalid_files");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create a valid session with existing worktree path
        let worktree_path = temp_dir.join("valid_worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let valid_session = Session {
            id: "test/valid".to_string(),
            project_id: "test".to_string(),
            branch: "valid".to_string(),
            worktree_path,
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
            note: None,
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

    /// Test that sessions with missing worktree paths are still loaded (issue #102).
    ///
    /// Sessions with non-existent worktrees should be included in load results
    /// so users can see them and clean up as needed.
    #[test]
    fn test_load_sessions_includes_missing_worktree() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_missing_worktree");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let nonexistent_worktree = temp_dir.join("worktree_that_does_not_exist");

        let session_missing_worktree = Session {
            id: "test/orphaned".to_string(),
            project_id: "test".to_string(),
            branch: "orphaned".to_string(),
            worktree_path: nonexistent_worktree.clone(),
            agent: "claude".to_string(),
            status: SessionStatus::Stopped,
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
            note: None,
        };

        let session_file = temp_dir.join("test_orphaned.json");
        let json = serde_json::to_string_pretty(&session_missing_worktree).unwrap();
        std::fs::write(&session_file, json).unwrap();

        assert!(session_file.exists());
        assert!(!nonexistent_worktree.exists());

        let (sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(skipped, 0);
        assert_eq!(sessions[0].id, "test/orphaned");
        assert_eq!(sessions[0].branch, "orphaned");
        assert!(!sessions[0].is_worktree_valid());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// Test loading sessions with mixed worktree states (valid and invalid together).
    ///
    /// Verifies that `load_sessions_from_files` correctly handles a directory containing
    /// both sessions with valid worktrees and sessions with missing worktrees.
    #[test]
    fn test_load_sessions_mixed_valid_and_missing_worktrees() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_mixed_worktrees");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create a valid worktree directory
        let valid_worktree = temp_dir.join("valid_worktree");
        std::fs::create_dir_all(&valid_worktree).unwrap();

        // Missing worktree path (not created)
        let missing_worktree = temp_dir.join("missing_worktree");

        // Session 1: valid worktree
        let session_valid = Session {
            id: "test/valid-session".to_string(),
            project_id: "test".to_string(),
            branch: "valid-session".to_string(),
            worktree_path: valid_worktree.clone(),
            agent: "claude".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 3000,
            port_range_end: 3009,
            port_count: 10,
            process_id: Some(12345),
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: "claude".to_string(),
            last_activity: Some("2024-01-01T00:00:00Z".to_string()),
            note: None,
        };

        // Session 2: missing worktree
        let session_missing = Session {
            id: "test/missing-session".to_string(),
            project_id: "test".to_string(),
            branch: "missing-session".to_string(),
            worktree_path: missing_worktree.clone(),
            agent: "claude".to_string(),
            status: SessionStatus::Stopped,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 3010,
            port_range_end: 3019,
            port_count: 10,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: "claude".to_string(),
            last_activity: Some("2024-01-01T00:00:00Z".to_string()),
            note: None,
        };

        // Save both sessions
        let valid_file = temp_dir.join("test_valid-session.json");
        let missing_file = temp_dir.join("test_missing-session.json");
        std::fs::write(
            &valid_file,
            serde_json::to_string_pretty(&session_valid).unwrap(),
        )
        .unwrap();
        std::fs::write(
            &missing_file,
            serde_json::to_string_pretty(&session_missing).unwrap(),
        )
        .unwrap();

        // Verify preconditions
        assert!(valid_worktree.exists());
        assert!(!missing_worktree.exists());

        // Load all sessions
        let (sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();

        // Both sessions should be loaded
        assert_eq!(sessions.len(), 2, "Both sessions should be loaded");
        assert_eq!(skipped, 0, "No sessions should be skipped");

        // Find each session and verify is_worktree_valid()
        let valid = sessions
            .iter()
            .find(|s| s.branch == "valid-session")
            .expect("Valid session should be loaded");
        let missing = sessions
            .iter()
            .find(|s| s.branch == "missing-session")
            .expect("Missing session should be loaded");

        assert!(
            valid.is_worktree_valid(),
            "Valid session should have valid worktree"
        );
        assert!(
            !missing.is_worktree_valid(),
            "Missing session should have invalid worktree"
        );

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
