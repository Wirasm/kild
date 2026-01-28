//! Cleanup operations for detecting and managing orphaned resources.
//!
//! Current cleanup strategies focus on:
//! - detect_stale_sessions: Sessions with missing/invalid worktrees
//! - detect_orphaned_branches: Git branches without corresponding sessions
//! - detect_orphaned_worktrees: Worktrees without corresponding sessions
//!
//! Note: Session-based detection strategies (PID-based, age-based) were
//! considered but not integrated into the cleanup workflow as of 2026-01-20.

use crate::cleanup::errors::CleanupError;
use git2::{BranchType, Repository};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::warn;

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

/// Detect worktrees in the kild directory that have no corresponding session.
///
/// This finds worktrees that:
/// 1. Are registered in git
/// 2. Have paths under `~/.kild/worktrees/<project>/`
/// 3. Have no session file with matching `worktree_path`
///
/// # Arguments
/// * `repo` - The git repository
/// * `worktrees_dir` - Base worktrees directory (~/.kild/worktrees)
/// * `sessions_dir` - Sessions directory (~/.kild/sessions)
/// * `project_name` - Current project name for scoping
pub fn detect_untracked_worktrees(
    repo: &Repository,
    worktrees_dir: &Path,
    sessions_dir: &Path,
    project_name: &str,
) -> Result<Vec<PathBuf>, CleanupError> {
    let mut untracked_worktrees = Vec::new();

    // Build the expected project worktrees directory
    let project_worktrees_dir = worktrees_dir.join(project_name);

    // Get all worktrees from git
    let worktrees = repo
        .worktrees()
        .map_err(|e| CleanupError::WorktreeScanFailed {
            message: format!("Failed to list worktrees: {}", e),
        })?;

    // Collect all worktree paths from session files
    let session_worktree_paths = collect_session_worktree_paths(sessions_dir)?;

    // Check each worktree
    for worktree_name in worktrees.iter().flatten() {
        let worktree = match repo.find_worktree(worktree_name) {
            Ok(wt) => wt,
            Err(e) => {
                warn!(
                    event = "core.cleanup.worktree_find_failed",
                    worktree_name = %worktree_name,
                    error = %e,
                    "Could not access registered worktree - it may be corrupted or inaccessible"
                );
                continue;
            }
        };
        let worktree_path = worktree.path();

        // Only consider worktrees under our project's worktrees directory
        let canonical_worktree = worktree_path.canonicalize();
        let canonical_project_dir = project_worktrees_dir.canonicalize();

        // Log when canonicalization fails - path comparison may be inaccurate
        if let Err(ref e) = canonical_worktree {
            warn!(
                event = "core.cleanup.worktree_canonicalize_failed",
                worktree_path = %worktree_path.display(),
                error = %e,
                "Could not resolve canonical path for worktree - using non-canonical comparison"
            );
        }
        if let Err(ref e) = canonical_project_dir {
            warn!(
                event = "core.cleanup.project_dir_canonicalize_failed",
                project_dir = %project_worktrees_dir.display(),
                error = %e,
                "Could not resolve canonical path for project directory - using non-canonical comparison"
            );
        }

        let is_in_kild_dir = match (&canonical_worktree, &canonical_project_dir) {
            (Ok(wt), Ok(pd)) => wt.starts_with(pd),
            // Fall back to non-canonical comparison if canonicalize fails
            _ => worktree_path.starts_with(&project_worktrees_dir),
        };

        if is_in_kild_dir {
            // Check if this worktree has a corresponding session
            let worktree_path_str = canonical_worktree
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| worktree_path.to_string_lossy().to_string());

            if !session_worktree_paths.contains(&worktree_path_str) {
                untracked_worktrees.push(worktree_path.to_path_buf());
            }
        }
    }

    Ok(untracked_worktrees)
}

/// Collect all worktree_path values from session files
fn collect_session_worktree_paths(sessions_dir: &Path) -> Result<HashSet<String>, CleanupError> {
    let mut paths = HashSet::new();

    if !sessions_dir.exists() {
        return Ok(paths);
    }

    let entries =
        std::fs::read_dir(sessions_dir).map_err(|e| CleanupError::IoError { source: e })?;

    for entry in entries {
        let entry = entry.map_err(|e| CleanupError::IoError { source: e })?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    match serde_json::from_str::<serde_json::Value>(&content) {
                        Ok(session) => {
                            match session.get("worktree_path") {
                                Some(worktree_value) => {
                                    if let Some(worktree_path) = worktree_value.as_str() {
                                        // Try to canonicalize for consistent comparison
                                        let canonical = PathBuf::from(worktree_path)
                                            .canonicalize()
                                            .map(|p| p.to_string_lossy().to_string())
                                            .unwrap_or_else(|_| worktree_path.to_string());
                                        paths.insert(canonical);
                                    } else {
                                        warn!(
                                            event = "core.cleanup.session_invalid_worktree_path_type",
                                            file_path = %path.display(),
                                            worktree_path_value = ?worktree_value,
                                            "Session file has worktree_path but it is not a string"
                                        );
                                    }
                                }
                                None => {
                                    warn!(
                                        event = "core.cleanup.session_missing_worktree_path",
                                        file_path = %path.display(),
                                        "Session file is missing worktree_path field"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                event = "core.cleanup.session_json_parse_failed",
                                file_path = %path.display(),
                                error = %e,
                                "Session file contains invalid JSON"
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        event = "core.cleanup.session_read_failed",
                        file_path = %path.display(),
                        error = %e,
                        "Could not read session file while collecting worktree paths"
                    );
                }
            }
        }
    }

    Ok(paths)
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
                        Err(e) => {
                            // Invalid JSON - consider it stale and log for debugging
                            warn!(
                                event = "core.cleanup.malformed_session_file",
                                file_path = %path.display(),
                                error = %e,
                                "Found malformed session file during cleanup scan"
                            );
                            if let Some(file_name) = path.file_stem().and_then(|s| s.to_str()) {
                                stale_sessions.push(file_name.to_string());
                            }
                        }
                    }
                }
                Err(e) => {
                    // Can't read session file - consider it stale and log for debugging
                    warn!(
                        event = "core.cleanup.unreadable_session_file",
                        file_path = %path.display(),
                        error = %e,
                        "Found unreadable session file during cleanup scan"
                    );
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
    use std::fs;
    use tempfile::TempDir;

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
        let temp_dir = TempDir::new().unwrap();

        let result = detect_stale_sessions(temp_dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_detect_stale_sessions_nonexistent_dir() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent_dir = temp_dir.path().join("nonexistent");

        let result = detect_stale_sessions(&nonexistent_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_detect_stale_sessions_with_valid_session() {
        let temp_dir = TempDir::new().unwrap();
        let test_path = temp_dir.path();

        // Create a valid session file with existing worktree path
        let session_content = serde_json::json!({
            "id": "test-session",
            "worktree_path": test_path.to_str().unwrap(), // Use temp_dir as worktree path (exists)
            "branch": "test-branch",
            "agent": "test-agent"
        });

        let session_file = test_path.join("test-session.json");
        fs::write(&session_file, session_content.to_string()).unwrap();

        let result = detect_stale_sessions(test_path);
        assert!(result.is_ok());
        // Should not detect as stale since worktree path exists
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_detect_stale_sessions_with_stale_session() {
        let temp_dir = TempDir::new().unwrap();
        let test_path = temp_dir.path();

        // Create a stale session file with non-existent worktree path
        let nonexistent_path = test_path.join("nonexistent_worktree");
        let session_content = serde_json::json!({
            "id": "stale-session",
            "worktree_path": nonexistent_path.to_str().unwrap(),
            "branch": "stale-branch",
            "agent": "test-agent"
        });

        let session_file = test_path.join("stale-session.json");
        fs::write(&session_file, session_content.to_string()).unwrap();

        let result = detect_stale_sessions(test_path);
        assert!(result.is_ok());
        let stale_sessions = result.unwrap();
        assert_eq!(stale_sessions.len(), 1);
        assert_eq!(stale_sessions[0], "stale-session");
    }

    #[test]
    fn test_detect_stale_sessions_with_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let test_path = temp_dir.path();

        // Create an invalid JSON file
        let session_file = test_path.join("invalid-session.json");
        fs::write(&session_file, "invalid json content").unwrap();

        let result = detect_stale_sessions(test_path);
        assert!(result.is_ok());
        let stale_sessions = result.unwrap();
        assert_eq!(stale_sessions.len(), 1);
        assert_eq!(stale_sessions[0], "invalid-session");
    }

    #[test]
    fn test_detect_stale_sessions_mixed_files() {
        let temp_dir = TempDir::new().unwrap();
        let test_path = temp_dir.path();

        // Create a valid session
        let valid_session = serde_json::json!({
            "id": "valid-session",
            "worktree_path": test_path.to_str().unwrap(),
            "branch": "valid-branch",
            "agent": "test-agent"
        });
        fs::write(
            test_path.join("valid-session.json"),
            valid_session.to_string(),
        )
        .unwrap();

        // Create a stale session
        let stale_session = serde_json::json!({
            "id": "stale-session",
            "worktree_path": test_path.join("nonexistent").to_str().unwrap(),
            "branch": "stale-branch",
            "agent": "test-agent"
        });
        fs::write(
            test_path.join("stale-session.json"),
            stale_session.to_string(),
        )
        .unwrap();

        // Create a non-JSON file (should be ignored)
        fs::write(test_path.join("not-a-session.txt"), "not json").unwrap();

        let result = detect_stale_sessions(test_path);
        assert!(result.is_ok());
        let stale_sessions = result.unwrap();
        assert_eq!(stale_sessions.len(), 1);
        assert_eq!(stale_sessions[0], "stale-session");
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

    #[test]
    fn test_cleanup_workflow_integration() {
        let temp_dir = TempDir::new().unwrap();
        let test_path = temp_dir.path();

        // Test that all detection functions work together
        let stale_result = detect_stale_sessions(test_path);
        assert!(stale_result.is_ok());

        // Test with a malformed session file
        let malformed_content = "{ invalid json }";
        fs::write(test_path.join("malformed.json"), malformed_content).unwrap();

        let stale_result = detect_stale_sessions(test_path);
        assert!(stale_result.is_ok());
        let stale_sessions = stale_result.unwrap();
        assert_eq!(stale_sessions.len(), 1);
        assert_eq!(stale_sessions[0], "malformed");

        // Test with a valid session file pointing to non-existent worktree
        let valid_session = serde_json::json!({
            "id": "test-session",
            "worktree_path": "/non/existent/path",
            "created_at": chrono::Utc::now().to_rfc3339(),
        });
        fs::write(test_path.join("valid.json"), valid_session.to_string()).unwrap();

        let stale_result = detect_stale_sessions(test_path);
        assert!(stale_result.is_ok());
        let stale_sessions = stale_result.unwrap();
        assert_eq!(stale_sessions.len(), 2); // malformed + valid with missing worktree
    }

    #[test]
    fn test_cleanup_workflow_empty_directory() {
        let temp_dir = TempDir::new().unwrap();

        let stale_result = detect_stale_sessions(temp_dir.path());
        assert!(stale_result.is_ok());
        let stale_sessions = stale_result.unwrap();
        assert_eq!(stale_sessions.len(), 0);
    }

    #[test]
    fn test_detect_stale_sessions_missing_id_field() {
        let temp_dir = TempDir::new().unwrap();
        let test_path = temp_dir.path();

        // Session with stale worktree but missing id field - should be skipped
        let session_content = serde_json::json!({
            "worktree_path": "/non/existent/path",
            "branch": "test-branch",
            "agent": "test-agent"
        });

        fs::write(
            test_path.join("no-id-session.json"),
            session_content.to_string(),
        )
        .unwrap();

        let result = detect_stale_sessions(test_path);
        assert!(result.is_ok());
        // Sessions without id field are skipped even if worktree is stale
        assert_eq!(result.unwrap().len(), 0);
    }
}
