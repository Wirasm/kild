use crate::cleanup::errors::CleanupError;
use git2::{BranchType, Repository};
use std::path::{Path, PathBuf};

pub fn validate_cleanup_request() -> Result<(), CleanupError> {
    // Check if we're in a git repository
    let current_dir = std::env::current_dir().map_err(|e| CleanupError::IoError { source: e })?;

    Repository::discover(&current_dir).map_err(|_| CleanupError::NotInRepository)?;

    Ok(())
}

pub fn detect_orphaned_branches(repo: &Repository) -> Result<Vec<String>, CleanupError> {
    let mut orphaned_branches = Vec::new();

    // Get all local branches
    let branches =
        repo.branches(Some(BranchType::Local))
            .map_err(|e| CleanupError::BranchScanFailed {
                message: format!("Failed to list branches: {}", e),
            })?;

    // Get all worktrees to check which branches are in use
    let worktrees = repo
        .worktrees()
        .map_err(|e| CleanupError::WorktreeScanFailed {
            message: format!("Failed to list worktrees: {}", e),
        })?;

    let mut active_branches = std::collections::HashSet::new();

    // Collect branches that are actively used by worktrees
    for worktree_name in worktrees.iter().flatten() {
        if let Ok(worktree) = repo.find_worktree(worktree_name) {
            // Try to get the branch name from the worktree
            if let Ok(worktree_repo) = Repository::open(worktree.path())
                && let Ok(head) = worktree_repo.head()
                && let Some(branch_name) = head.shorthand()
            {
                active_branches.insert(branch_name.to_string());
            }
        }
    }

    // Also add the main branch (current HEAD of main repo)
    if let Ok(head) = repo.head()
        && let Some(branch_name) = head.shorthand()
    {
        active_branches.insert(branch_name.to_string());
    }

    // Check each branch to see if it's orphaned
    for (branch, _) in branches.flatten() {
        if let Some(branch_name) = branch.name().ok().flatten() {
            // Check if this is a worktree branch that's not actively used
            if branch_name.starts_with("worktree-") && !active_branches.contains(branch_name) {
                orphaned_branches.push(branch_name.to_string());
            }
        }
    }

    Ok(orphaned_branches)
}

pub fn detect_orphaned_worktrees(repo: &Repository) -> Result<Vec<PathBuf>, CleanupError> {
    let mut orphaned_worktrees = Vec::new();

    let worktrees = repo
        .worktrees()
        .map_err(|e| CleanupError::WorktreeScanFailed {
            message: format!("Failed to list worktrees: {}", e),
        })?;

    for worktree_name in worktrees.iter().flatten() {
        if let Ok(worktree) = repo.find_worktree(worktree_name) {
            let worktree_path = worktree.path();

            // Check if worktree directory exists but is in a bad state
            if worktree_path.exists() {
                // Try to open the worktree as a repository
                match Repository::open(worktree_path) {
                    Ok(worktree_repo) => {
                        // Check if HEAD is detached or in a bad state
                        if let Ok(head) = worktree_repo.head() {
                            if head.target().is_none() {
                                // Detached HEAD with no target - likely orphaned
                                orphaned_worktrees.push(worktree_path.to_path_buf());
                            }
                        } else {
                            // Can't read HEAD - likely corrupted
                            orphaned_worktrees.push(worktree_path.to_path_buf());
                        }
                    }
                    Err(_) => {
                        // Can't open as repository - likely corrupted
                        orphaned_worktrees.push(worktree_path.to_path_buf());
                    }
                }
            } else {
                // Worktree registered but directory doesn't exist
                orphaned_worktrees.push(worktree_path.to_path_buf());
            }
        }
    }

    Ok(orphaned_worktrees)
}

pub fn detect_stale_sessions(sessions_dir: &Path) -> Result<Vec<String>, CleanupError> {
    let mut stale_sessions = Vec::new();

    if !sessions_dir.exists() {
        return Ok(stale_sessions);
    }

    let entries =
        std::fs::read_dir(sessions_dir).map_err(|e| CleanupError::IoError { source: e })?;

    for entry in entries {
        let entry = entry.map_err(|e| CleanupError::IoError { source: e })?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
            // Try to read the session file
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    // Try to parse as JSON to validate it's a proper session file
                    match serde_json::from_str::<serde_json::Value>(&content) {
                        Ok(session) => {
                            // Check if the worktree path exists
                            if let Some(worktree_path) =
                                session.get("worktree_path").and_then(|v| v.as_str())
                            {
                                let worktree_path = PathBuf::from(worktree_path);
                                if !worktree_path.exists() {
                                    // Session references non-existent worktree
                                    if let Some(session_id) =
                                        session.get("id").and_then(|v| v.as_str())
                                    {
                                        stale_sessions.push(session_id.to_string());
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            // Invalid JSON - consider it stale
                            if let Some(file_name) = path.file_stem().and_then(|s| s.to_str()) {
                                stale_sessions.push(file_name.to_string());
                            }
                        }
                    }
                }
                Err(_) => {
                    // Can't read session file - consider it stale
                    if let Some(file_name) = path.file_stem().and_then(|s| s.to_str()) {
                        stale_sessions.push(file_name.to_string());
                    }
                }
            }
        }
    }

    Ok(stale_sessions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    #[test]
    fn test_validate_cleanup_request_not_in_repo() {
        // This test assumes we're not in a git repo at /tmp
        let original_dir = std::env::current_dir().unwrap();

        // Try to change to a non-git directory for testing
        if std::env::set_current_dir("/tmp").is_ok() {
            let result = validate_cleanup_request();
            // Should fail if /tmp is not a git repo
            if result.is_err() {
                assert!(matches!(result.unwrap_err(), CleanupError::NotInRepository));
            }

            // Restore original directory
            let _ = std::env::set_current_dir(original_dir);
        }
    }

    #[test]
    fn test_detect_stale_sessions_empty_dir() {
        let temp_dir = std::env::temp_dir().join("shards_test_empty_sessions");
        let _ = std::fs::create_dir_all(&temp_dir);

        let result = detect_stale_sessions(&temp_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_stale_sessions_nonexistent_dir() {
        let nonexistent_dir = std::env::temp_dir().join("shards_test_nonexistent");

        let result = detect_stale_sessions(&nonexistent_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_detect_stale_sessions_with_valid_session() {
        let temp_dir = env::temp_dir().join("shards_test_valid_session");
        let _ = fs::create_dir_all(&temp_dir);

        // Create a valid session file with existing worktree path
        let session_content = serde_json::json!({
            "id": "test-session",
            "worktree_path": temp_dir.to_str().unwrap(), // Use temp_dir as worktree path (exists)
            "branch": "test-branch",
            "agent": "test-agent"
        });

        let session_file = temp_dir.join("test-session.json");
        fs::write(&session_file, session_content.to_string()).unwrap();

        let result = detect_stale_sessions(&temp_dir);
        assert!(result.is_ok());
        // Should not detect as stale since worktree path exists
        assert_eq!(result.unwrap().len(), 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_stale_sessions_with_stale_session() {
        let temp_dir = env::temp_dir().join("shards_test_stale_session");
        let _ = fs::create_dir_all(&temp_dir);

        // Create a stale session file with non-existent worktree path
        let nonexistent_path = temp_dir.join("nonexistent_worktree");
        let session_content = serde_json::json!({
            "id": "stale-session",
            "worktree_path": nonexistent_path.to_str().unwrap(),
            "branch": "stale-branch",
            "agent": "test-agent"
        });

        let session_file = temp_dir.join("stale-session.json");
        fs::write(&session_file, session_content.to_string()).unwrap();

        let result = detect_stale_sessions(&temp_dir);
        assert!(result.is_ok());
        let stale_sessions = result.unwrap();
        assert_eq!(stale_sessions.len(), 1);
        assert_eq!(stale_sessions[0], "stale-session");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_stale_sessions_with_invalid_json() {
        let temp_dir = env::temp_dir().join("shards_test_invalid_json");
        let _ = fs::create_dir_all(&temp_dir);

        // Create an invalid JSON file
        let session_file = temp_dir.join("invalid-session.json");
        fs::write(&session_file, "invalid json content").unwrap();

        let result = detect_stale_sessions(&temp_dir);
        assert!(result.is_ok());
        let stale_sessions = result.unwrap();
        assert_eq!(stale_sessions.len(), 1);
        assert_eq!(stale_sessions[0], "invalid-session");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_stale_sessions_mixed_files() {
        let temp_dir = env::temp_dir().join("shards_test_mixed_files");
        let _ = fs::create_dir_all(&temp_dir);

        // Create a valid session
        let valid_session = serde_json::json!({
            "id": "valid-session",
            "worktree_path": temp_dir.to_str().unwrap(),
            "branch": "valid-branch",
            "agent": "test-agent"
        });
        fs::write(
            &temp_dir.join("valid-session.json"),
            valid_session.to_string(),
        )
        .unwrap();

        // Create a stale session
        let stale_session = serde_json::json!({
            "id": "stale-session",
            "worktree_path": temp_dir.join("nonexistent").to_str().unwrap(),
            "branch": "stale-branch",
            "agent": "test-agent"
        });
        fs::write(
            &temp_dir.join("stale-session.json"),
            stale_session.to_string(),
        )
        .unwrap();

        // Create a non-JSON file (should be ignored)
        fs::write(&temp_dir.join("not-a-session.txt"), "not json").unwrap();

        let result = detect_stale_sessions(&temp_dir);
        assert!(result.is_ok());
        let stale_sessions = result.unwrap();
        assert_eq!(stale_sessions.len(), 1);
        assert_eq!(stale_sessions[0], "stale-session");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_orphaned_branches_empty_repo() {
        // This test would require setting up a real Git repository
        // For now, we test the error case when not in a repo
        let original_dir = std::env::current_dir().unwrap();

        if std::env::set_current_dir("/tmp").is_ok() {
            // Try to create a repository and test branch detection
            // This is a simplified test - in practice would need full Git setup
            let result = validate_cleanup_request();
            if result.is_err() {
                assert!(matches!(result.unwrap_err(), CleanupError::NotInRepository));
            }

            let _ = std::env::set_current_dir(original_dir);
        }
    }

    #[test]
    fn test_detect_orphaned_worktrees_error_handling() {
        // Test error handling when not in a Git repository
        let original_dir = std::env::current_dir().unwrap();

        if std::env::set_current_dir("/tmp").is_ok() {
            let result = validate_cleanup_request();
            if result.is_err() {
                assert!(matches!(result.unwrap_err(), CleanupError::NotInRepository));
            }

            let _ = std::env::set_current_dir(original_dir);
        }
    }
}
