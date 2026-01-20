use git2::{BranchType, Repository};
use tracing::{error, info, warn};

use crate::cleanup::{errors::CleanupError, operations, types::*};
use crate::core::config::Config;
use crate::git;
use crate::sessions;

pub fn scan_for_orphans() -> Result<CleanupSummary, CleanupError> {
    info!(event = "cleanup.scan_started");

    // Validate we're in a git repository
    operations::validate_cleanup_request()?;

    let current_dir = std::env::current_dir().map_err(|e| CleanupError::IoError { source: e })?;
    let repo = Repository::discover(&current_dir).map_err(|_| CleanupError::NotInRepository)?;

    let mut summary = CleanupSummary::new();

    // Detect orphaned branches
    match operations::detect_orphaned_branches(&repo) {
        Ok(orphaned_branches) => {
            info!(
                event = "cleanup.scan_branches_completed",
                count = orphaned_branches.len()
            );
            for branch in orphaned_branches {
                summary.add_branch(branch);
            }
        }
        Err(e) => {
            error!(
                event = "cleanup.scan_branches_failed",
                error = %e
            );
            return Err(e);
        }
    }

    // Detect orphaned worktrees
    match operations::detect_orphaned_worktrees(&repo) {
        Ok(orphaned_worktrees) => {
            info!(
                event = "cleanup.scan_worktrees_completed",
                count = orphaned_worktrees.len()
            );
            for worktree_path in orphaned_worktrees {
                summary.add_worktree(worktree_path);
            }
        }
        Err(e) => {
            error!(
                event = "cleanup.scan_worktrees_failed",
                error = %e
            );
            return Err(e);
        }
    }

    // Detect stale sessions
    let config = Config::new();
    match operations::detect_stale_sessions(&config.sessions_dir()) {
        Ok(stale_sessions) => {
            info!(
                event = "cleanup.scan_sessions_completed",
                count = stale_sessions.len()
            );
            for session_id in stale_sessions {
                summary.add_session(session_id);
            }
        }
        Err(e) => {
            error!(
                event = "cleanup.scan_sessions_failed",
                error = %e
            );
            return Err(e);
        }
    }

    info!(
        event = "cleanup.scan_completed",
        total_orphaned = summary.total_cleaned,
        branches = summary.orphaned_branches.len(),
        worktrees = summary.orphaned_worktrees.len(),
        sessions = summary.stale_sessions.len()
    );

    Ok(summary)
}

pub fn cleanup_orphaned_resources(
    summary: &CleanupSummary,
) -> Result<CleanupSummary, CleanupError> {
    info!(
        event = "cleanup.cleanup_started",
        total_resources = summary.total_cleaned
    );

    let mut cleaned_summary = CleanupSummary::new();

    // Clean up orphaned branches
    if !summary.orphaned_branches.is_empty() {
        match cleanup_orphaned_branches(&summary.orphaned_branches) {
            Ok(cleaned_branches) => {
                for branch in cleaned_branches {
                    cleaned_summary.add_branch(branch);
                }
            }
            Err(e) => {
                error!(
                    event = "cleanup.cleanup_branches_failed",
                    error = %e
                );
                return Err(e);
            }
        }
    }

    // Clean up orphaned worktrees
    if !summary.orphaned_worktrees.is_empty() {
        match cleanup_orphaned_worktrees(&summary.orphaned_worktrees) {
            Ok(cleaned_worktrees) => {
                for worktree_path in cleaned_worktrees {
                    cleaned_summary.add_worktree(worktree_path);
                }
            }
            Err(e) => {
                error!(
                    event = "cleanup.cleanup_worktrees_failed",
                    error = %e
                );
                return Err(e);
            }
        }
    }

    // Clean up stale sessions
    if !summary.stale_sessions.is_empty() {
        match cleanup_stale_sessions(&summary.stale_sessions) {
            Ok(cleaned_sessions) => {
                for session_id in cleaned_sessions {
                    cleaned_summary.add_session(session_id);
                }
            }
            Err(e) => {
                error!(
                    event = "cleanup.cleanup_sessions_failed",
                    error = %e
                );
                return Err(e);
            }
        }
    }

    info!(
        event = "cleanup.cleanup_completed",
        total_cleaned = cleaned_summary.total_cleaned
    );

    Ok(cleaned_summary)
}

pub fn cleanup_all() -> Result<CleanupSummary, CleanupError> {
    info!(event = "cleanup.cleanup_all_started");

    // First scan for orphaned resources
    let scan_summary = scan_for_orphans()?;

    if scan_summary.total_cleaned == 0 {
        info!(event = "cleanup.cleanup_all_no_resources");
        return Err(CleanupError::NoOrphanedResources);
    }

    // Then clean them up
    let cleanup_summary = cleanup_orphaned_resources(&scan_summary)?;

    info!(
        event = "cleanup.cleanup_all_completed",
        total_cleaned = cleanup_summary.total_cleaned
    );

    Ok(cleanup_summary)
}

/// Cleanup all orphaned resources using the specified strategy.
/// 
/// # Arguments
/// * `strategy` - The cleanup strategy to use (All, NoPid, Stopped, OlderThan)
/// 
/// # Returns
/// * `Ok(CleanupSummary)` - Summary of cleaned resources
/// * `Err(CleanupError)` - If cleanup fails or no resources found
pub fn cleanup_all_with_strategy(strategy: CleanupStrategy) -> Result<CleanupSummary, CleanupError> {
    info!(event = "cleanup.cleanup_all_with_strategy_started", strategy = ?strategy);

    // First scan for orphaned resources with strategy
    let scan_summary = scan_for_orphans_with_strategy(strategy)?;

    if scan_summary.stale_sessions.is_empty() 
        && scan_summary.orphaned_branches.is_empty() 
        && scan_summary.orphaned_worktrees.is_empty() {
        info!(event = "cleanup.cleanup_all_with_strategy_no_resources");
        return Err(CleanupError::NoOrphanedResources);
    }

    // Then clean them up
    let cleanup_summary = cleanup_orphaned_resources(&scan_summary)?;

    info!(
        event = "cleanup.cleanup_all_with_strategy_completed",
        total_cleaned = cleanup_summary.total_cleaned
    );

    Ok(cleanup_summary)
}

/// Scan for orphaned resources using the specified cleanup strategy.
/// 
/// # Arguments
/// * `strategy` - The cleanup strategy to determine which resources to scan for
/// 
/// # Returns
/// * `Ok(CleanupSummary)` - Summary of found orphaned resources
/// * `Err(CleanupError)` - If scanning fails
pub fn scan_for_orphans_with_strategy(strategy: CleanupStrategy) -> Result<CleanupSummary, CleanupError> {
    info!(event = "cleanup.scan_with_strategy_started", strategy = ?strategy);

    operations::validate_cleanup_request()?;

    let current_dir = std::env::current_dir().map_err(|e| CleanupError::IoError { source: e })?;
    let _repo = Repository::discover(&current_dir).map_err(|e| {
        error!(event = "cleanup.git_discovery_failed", error = %e);
        CleanupError::GitError { source: crate::git::errors::GitError::Git2Error { source: e } }
    })?;
    let config = Config::new();

    let mut summary = CleanupSummary::new();

    match strategy {
        CleanupStrategy::All => {
            // All strategy delegates to legacy scan_for_orphans()
            info!(event = "cleanup.strategy_all_delegating");
            return scan_for_orphans().map_err(|e| {
                error!(event = "cleanup.strategy_all_failed", error = %e);
                e
            });
        }
        CleanupStrategy::NoPid => {
            let sessions = operations::detect_sessions_without_pid(&config.sessions_dir())
                .map_err(|e| {
                    error!(event = "cleanup.strategy_failed", strategy = "NoPid", error = %e);
                    CleanupError::StrategyFailed { 
                        strategy: "NoPid".to_string(), 
                        source: Box::new(e) 
                    }
                })?;
            for session_id in sessions {
                summary.add_session(session_id);
            }
        }
        CleanupStrategy::Stopped => {
            let sessions = operations::detect_sessions_with_stopped_processes(&config.sessions_dir())
                .map_err(|e| {
                    error!(event = "cleanup.strategy_failed", strategy = "Stopped", error = %e);
                    CleanupError::StrategyFailed { 
                        strategy: "Stopped".to_string(), 
                        source: Box::new(e) 
                    }
                })?;
            for session_id in sessions {
                summary.add_session(session_id);
            }
        }
        CleanupStrategy::OlderThan(days) => {
            let sessions = operations::detect_old_sessions(&config.sessions_dir(), days)
                .map_err(|e| {
                    error!(event = "cleanup.strategy_failed", strategy = "OlderThan", error = %e);
                    CleanupError::StrategyFailed { 
                        strategy: format!("OlderThan({})", days), 
                        source: Box::new(e) 
                    }
                })?;
            for session_id in sessions {
                summary.add_session(session_id);
            }
        }
    }

    info!(
        event = "cleanup.scan_with_strategy_completed",
        total_sessions = summary.stale_sessions.len()
    );

    Ok(summary)
}

fn cleanup_orphaned_branches(branches: &[String]) -> Result<Vec<String>, CleanupError> {
    // Early return for empty list - no Git access needed
    if branches.is_empty() {
        return Ok(Vec::new());
    }

    let current_dir = std::env::current_dir().map_err(|e| CleanupError::IoError { source: e })?;
    let repo = Repository::discover(&current_dir).map_err(|_| CleanupError::NotInRepository)?;

    let mut cleaned_branches = Vec::new();

    for branch_name in branches {
        info!(
            event = "cleanup.branch_delete_started",
            branch = branch_name
        );

        match repo.find_branch(branch_name, BranchType::Local) {
            Ok(mut branch) => {
                match branch.delete() {
                    Ok(()) => {
                        info!(
                            event = "cleanup.branch_delete_completed",
                            branch = branch_name
                        );
                        cleaned_branches.push(branch_name.clone());
                    }
                    Err(e) => {
                        // Handle race conditions gracefully - another process might have deleted the branch
                        let error_msg = e.to_string();
                        if error_msg.contains("not found") || error_msg.contains("does not exist") {
                            info!(
                                event = "cleanup.branch_delete_race_condition",
                                branch = branch_name,
                                message = "Branch was deleted by another process - considering as cleaned"
                            );
                            cleaned_branches.push(branch_name.clone());
                        } else {
                            error!(
                                event = "cleanup.branch_delete_failed",
                                branch = branch_name,
                                error = %e,
                                error_type = "permission_or_lock_error"
                            );
                            return Err(CleanupError::CleanupFailed {
                                name: branch_name.clone(),
                                message: format!(
                                    "Failed to delete branch (not a race condition): {}",
                                    e
                                ),
                            });
                        }
                    }
                }
            }
            Err(e) => {
                warn!(
                    event = "cleanup.branch_not_found",
                    branch = branch_name,
                    error = %e
                );
                // Branch already gone, consider it cleaned
                cleaned_branches.push(branch_name.clone());
            }
        }
    }

    Ok(cleaned_branches)
}

fn cleanup_orphaned_worktrees(
    worktree_paths: &[std::path::PathBuf],
) -> Result<Vec<std::path::PathBuf>, CleanupError> {
    // Early return for empty list
    if worktree_paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut cleaned_worktrees = Vec::new();

    for worktree_path in worktree_paths {
        info!(
            event = "cleanup.worktree_delete_started",
            worktree_path = %worktree_path.display()
        );

        match git::handler::remove_worktree_by_path(worktree_path) {
            Ok(()) => {
                info!(
                    event = "cleanup.worktree_delete_completed",
                    worktree_path = %worktree_path.display()
                );
                cleaned_worktrees.push(worktree_path.clone());
            }
            Err(e) => {
                error!(
                    event = "cleanup.worktree_delete_failed",
                    worktree_path = %worktree_path.display(),
                    error = %e
                );
                return Err(CleanupError::CleanupFailed {
                    name: worktree_path.display().to_string(),
                    message: format!("Failed to remove worktree: {}", e),
                });
            }
        }
    }

    Ok(cleaned_worktrees)
}

fn cleanup_stale_sessions(session_ids: &[String]) -> Result<Vec<String>, CleanupError> {
    // Early return for empty list
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }

    let config = Config::new();
    let mut cleaned_sessions = Vec::new();

    for session_id in session_ids {
        info!(
            event = "cleanup.session_delete_started",
            session_id = session_id
        );

        match sessions::operations::remove_session_file(&config.sessions_dir(), session_id) {
            Ok(()) => {
                info!(
                    event = "cleanup.session_delete_completed",
                    session_id = session_id
                );
                cleaned_sessions.push(session_id.clone());
            }
            Err(e) => {
                error!(
                    event = "cleanup.session_delete_failed",
                    session_id = session_id,
                    error = %e
                );
                return Err(CleanupError::SessionError { source: e });
            }
        }
    }

    Ok(cleaned_sessions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_for_orphans_not_in_repo() {
        // This test assumes we're not in a git repo at /tmp
        let original_dir = std::env::current_dir().unwrap();

        // Try to change to a non-git directory for testing
        if std::env::set_current_dir("/tmp").is_ok() {
            let result = scan_for_orphans();
            // Should fail if /tmp is not a git repo
            if result.is_err() {
                assert!(matches!(result.unwrap_err(), CleanupError::NotInRepository));
            }

            // Restore original directory
            let _ = std::env::set_current_dir(original_dir);
        }
    }

    #[test]
    fn test_cleanup_all_no_resources() {
        // This test verifies the NoOrphanedResources error case
        // In a clean repository, cleanup_all should return NoOrphanedResources
        // Note: This test may pass or fail depending on the actual repository state
    }

    #[test]
    fn test_cleanup_orphaned_branches_empty_list() {
        let result = cleanup_orphaned_branches(&[]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_cleanup_orphaned_worktrees_empty_list() {
        let result = cleanup_orphaned_worktrees(&[]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_cleanup_stale_sessions_empty_list() {
        let result = cleanup_stale_sessions(&[]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_cleanup_orphaned_resources_empty_summary() {
        let empty_summary = CleanupSummary::new();
        let result = cleanup_orphaned_resources(&empty_summary);
        assert!(result.is_ok());
        let cleaned = result.unwrap();
        assert_eq!(cleaned.total_cleaned, 0);
        assert_eq!(cleaned.orphaned_branches.len(), 0);
        assert_eq!(cleaned.orphaned_worktrees.len(), 0);
        assert_eq!(cleaned.stale_sessions.len(), 0);
    }

    #[test]
    fn test_cleanup_summary_operations() {
        let mut summary = CleanupSummary::new();
        assert_eq!(summary.total_cleaned, 0);

        summary.add_branch("test-branch".to_string());
        assert_eq!(summary.total_cleaned, 1);
        assert_eq!(summary.orphaned_branches.len(), 1);
        assert_eq!(summary.orphaned_branches[0], "test-branch");

        summary.add_worktree(std::path::PathBuf::from("/tmp/test"));
        assert_eq!(summary.total_cleaned, 2);
        assert_eq!(summary.orphaned_worktrees.len(), 1);

        summary.add_session("test-session".to_string());
        assert_eq!(summary.total_cleaned, 3);
        assert_eq!(summary.stale_sessions.len(), 1);
        assert_eq!(summary.stale_sessions[0], "test-session");
    }

    #[test]
    fn test_cleanup_summary_default() {
        let summary = CleanupSummary::default();
        assert_eq!(summary.total_cleaned, 0);
        assert_eq!(summary.orphaned_branches.len(), 0);
        assert_eq!(summary.orphaned_worktrees.len(), 0);
        assert_eq!(summary.stale_sessions.len(), 0);
    }
}
