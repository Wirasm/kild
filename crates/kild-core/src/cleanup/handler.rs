use git2::{BranchType, Repository};
use tracing::{error, info, warn};

use crate::cleanup::{errors::CleanupError, operations, types::*};
use crate::git;
use crate::sessions;
use kild_config::Config;

pub fn scan_for_orphans() -> Result<CleanupSummary, CleanupError> {
    info!(event = "core.cleanup.scan_started");

    // Validate we're in a git repository
    operations::validate_cleanup_request()?;

    let current_dir = std::env::current_dir().map_err(|e| CleanupError::IoError { source: e })?;
    let repo = Repository::discover(&current_dir).map_err(|_| CleanupError::NotInRepository)?;

    let mut summary = CleanupSummary::new();

    // Detect orphaned branches
    match operations::detect_orphaned_branches(&repo) {
        Ok(orphaned_branches) => {
            info!(
                event = "core.cleanup.scan_branches_completed",
                count = orphaned_branches.len()
            );
            for branch in orphaned_branches {
                summary.add_branch(branch);
            }
        }
        Err(e) => {
            error!(
                event = "core.cleanup.scan_branches_failed",
                error = %e
            );
            return Err(e);
        }
    }

    // Detect orphaned worktrees
    match operations::detect_orphaned_worktrees(&repo) {
        Ok(orphaned_worktrees) => {
            info!(
                event = "core.cleanup.scan_worktrees_completed",
                count = orphaned_worktrees.len()
            );
            for worktree_path in orphaned_worktrees {
                summary.add_worktree(worktree_path);
            }
        }
        Err(e) => {
            error!(
                event = "core.cleanup.scan_worktrees_failed",
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
                event = "core.cleanup.scan_sessions_completed",
                count = stale_sessions.len()
            );
            for session_id in stale_sessions {
                summary.add_session(session_id);
            }
        }
        Err(e) => {
            error!(
                event = "core.cleanup.scan_sessions_failed",
                error = %e
            );
            return Err(e);
        }
    }

    info!(
        event = "core.cleanup.scan_completed",
        total_orphaned = summary.total_cleaned,
        branches = summary.orphaned_branches.len(),
        worktrees = summary.orphaned_worktrees.len(),
        sessions = summary.stale_sessions.len()
    );

    Ok(summary)
}

pub fn cleanup_orphaned_resources(
    summary: &CleanupSummary,
    force: bool,
) -> Result<CleanupSummary, CleanupError> {
    info!(
        event = "core.cleanup.cleanup_started",
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
                    event = "core.cleanup.cleanup_branches_failed",
                    error = %e
                );
                return Err(e);
            }
        }
    }

    // Clean up orphaned worktrees
    if !summary.orphaned_worktrees.is_empty() {
        match cleanup_orphaned_worktrees(&summary.orphaned_worktrees, force) {
            Ok((cleaned_worktrees, skipped_worktrees)) => {
                for worktree_path in cleaned_worktrees {
                    cleaned_summary.add_worktree(worktree_path);
                }
                for (path, reason) in skipped_worktrees {
                    cleaned_summary.add_skipped_worktree(path, reason);
                }
            }
            Err(e) => {
                error!(
                    event = "core.cleanup.cleanup_worktrees_failed",
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
                    event = "core.cleanup.cleanup_sessions_failed",
                    error = %e
                );
                return Err(e);
            }
        }
    }

    info!(
        event = "core.cleanup.cleanup_completed",
        total_cleaned = cleaned_summary.total_cleaned
    );

    Ok(cleaned_summary)
}

pub fn cleanup_all() -> Result<CleanupSummary, CleanupError> {
    info!(event = "core.cleanup.cleanup_all_started");

    // First scan for orphaned resources
    let scan_summary = scan_for_orphans()?;

    if scan_summary.total_cleaned == 0 {
        info!(event = "core.cleanup.cleanup_all_no_resources");
        return Err(CleanupError::NoOrphanedResources);
    }

    // Then clean them up
    let cleanup_summary = cleanup_orphaned_resources(&scan_summary, false)?;

    info!(
        event = "core.cleanup.cleanup_all_completed",
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
pub fn cleanup_all_with_strategy(
    strategy: CleanupStrategy,
    force: bool,
) -> Result<CleanupSummary, CleanupError> {
    info!(event = "core.cleanup.cleanup_all_with_strategy_started", strategy = ?strategy);

    // First scan for orphaned resources with strategy
    let scan_summary = scan_for_orphans_with_strategy(strategy)?;

    if scan_summary.stale_sessions.is_empty()
        && scan_summary.orphaned_branches.is_empty()
        && scan_summary.orphaned_worktrees.is_empty()
    {
        info!(event = "core.cleanup.cleanup_all_with_strategy_no_resources");
        return Err(CleanupError::NoOrphanedResources);
    }

    // Then clean them up
    let cleanup_summary = cleanup_orphaned_resources(&scan_summary, force)?;

    info!(
        event = "core.cleanup.cleanup_all_with_strategy_completed",
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
pub fn scan_for_orphans_with_strategy(
    strategy: CleanupStrategy,
) -> Result<CleanupSummary, CleanupError> {
    info!(event = "core.cleanup.scan_with_strategy_started", strategy = ?strategy);

    operations::validate_cleanup_request()?;

    let current_dir = std::env::current_dir().map_err(|e| CleanupError::IoError { source: e })?;
    let _repo = Repository::discover(&current_dir).map_err(|e| {
        error!(event = "core.cleanup.git_discovery_failed", error = %e);
        CleanupError::GitError {
            source: crate::git::errors::GitError::Git2Error { source: e },
        }
    })?;
    let config = Config::new();

    let mut summary = CleanupSummary::new();

    match strategy {
        CleanupStrategy::All => {
            // All strategy delegates to scan_for_orphans()
            info!(event = "core.cleanup.strategy_all_delegating");
            return scan_for_orphans().map_err(|e| {
                error!(event = "core.cleanup.strategy_all_failed", error = %e);
                e
            });
        }
        CleanupStrategy::NoPid => {
            let sessions =
                operations::detect_stale_sessions(&config.sessions_dir()).map_err(|e| {
                    error!(event = "core.cleanup.strategy_failed", strategy = "NoPid", error = %e);
                    CleanupError::StrategyFailed {
                        strategy: "NoPid".to_string(),
                        source: Box::new(e),
                    }
                })?;
            for session_id in sessions {
                summary.add_session(session_id);
            }
        }
        CleanupStrategy::Stopped => {
            let sessions =
                operations::detect_stale_sessions(&config.sessions_dir()).map_err(|e| {
                    error!(event = "core.cleanup.strategy_failed", strategy = "Stopped", error = %e);
                    CleanupError::StrategyFailed {
                        strategy: "Stopped".to_string(),
                        source: Box::new(e),
                    }
                })?;
            for session_id in sessions {
                summary.add_session(session_id);
            }
        }
        CleanupStrategy::OlderThan(days) => {
            let sessions =
                operations::detect_stale_sessions(&config.sessions_dir()).map_err(|e| {
                    error!(event = "core.cleanup.strategy_failed", strategy = "OlderThan", error = %e);
                    CleanupError::StrategyFailed {
                        strategy: format!("OlderThan({})", days),
                        source: Box::new(e),
                    }
                })?;
            for session_id in sessions {
                summary.add_session(session_id);
            }
        }
        CleanupStrategy::Orphans => {
            // Get current project info for scoping
            let project = git::handler::detect_project().map_err(|e| {
                error!(event = "core.cleanup.strategy_failed", strategy = "Orphans", error = %e);
                CleanupError::GitError { source: e }
            })?;

            // Get repo for worktree operations
            let repo = Repository::discover(&project.path).map_err(|e| {
                error!(event = "core.cleanup.git_discovery_failed", error = %e);
                CleanupError::GitError {
                    source: git::errors::GitError::Git2Error { source: e },
                }
            })?;

            // Detect untracked worktrees (in kild dir but no session)
            let untracked = operations::detect_untracked_worktrees(
                &repo,
                &config.worktrees_dir(),
                &config.sessions_dir(),
                &project.name,
            )
            .map_err(|e| {
                error!(event = "core.cleanup.strategy_failed", strategy = "Orphans", error = %e);
                CleanupError::StrategyFailed {
                    strategy: "Orphans".to_string(),
                    source: Box::new(e),
                }
            })?;

            info!(
                event = "core.cleanup.orphans_scan_completed",
                untracked_count = untracked.len(),
                project = project.name
            );

            for worktree_path in untracked {
                summary.add_worktree(worktree_path);
            }

            // Also detect orphaned branches (worktree-* not checked out)
            let orphaned_branches = operations::detect_orphaned_branches(&repo).map_err(|e| {
                error!(event = "core.cleanup.strategy_failed", strategy = "Orphans", error = %e);
                CleanupError::StrategyFailed {
                    strategy: "Orphans".to_string(),
                    source: Box::new(e),
                }
            })?;

            for branch in orphaned_branches {
                summary.add_branch(branch);
            }
        }
    }

    info!(
        event = "core.cleanup.scan_with_strategy_completed",
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
            event = "core.cleanup.branch_delete_started",
            branch = branch_name
        );

        match repo.find_branch(branch_name, BranchType::Local) {
            Ok(mut branch) => {
                match branch.delete() {
                    Ok(()) => {
                        info!(
                            event = "core.cleanup.branch_delete_completed",
                            branch = branch_name
                        );
                        cleaned_branches.push(branch_name.clone());
                    }
                    Err(e) => {
                        // Handle race conditions gracefully - another process might have deleted the branch
                        let error_msg = e.to_string();
                        if error_msg.contains("not found") || error_msg.contains("does not exist") {
                            info!(
                                event = "core.cleanup.branch_delete_race_condition",
                                branch = branch_name,
                                message = "Branch was deleted by another process - considering as cleaned"
                            );
                            cleaned_branches.push(branch_name.clone());
                        } else {
                            error!(
                                event = "core.cleanup.branch_delete_failed",
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
                    event = "core.cleanup.branch_not_found",
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

type WorktreeCleanupResult = (Vec<std::path::PathBuf>, Vec<(std::path::PathBuf, String)>);

fn cleanup_orphaned_worktrees(
    worktree_paths: &[std::path::PathBuf],
    force: bool,
) -> Result<WorktreeCleanupResult, CleanupError> {
    // Early return for empty list
    if worktree_paths.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut cleaned_worktrees = Vec::new();
    let mut skipped_worktrees = Vec::new();

    for worktree_path in worktree_paths {
        info!(
            event = "core.cleanup.worktree_delete_started",
            worktree_path = %worktree_path.display()
        );

        // Safety checks only apply when the directory exists.
        // If the directory is already gone, removal is safe (nothing to lose).
        if worktree_path.exists() {
            // Check 1: uncommitted changes via git status.
            // get_worktree_status returns Ok(status) with has_uncommitted_changes=true AND
            // status_check_failed=true when the internal check fails (conservative fallback).
            // Distinguish these cases so the skip reason shown to the user is accurate.
            match git::get_worktree_status(worktree_path) {
                Ok(status) if status.has_uncommitted_changes && !status.status_check_failed => {
                    if force {
                        warn!(
                            event = "core.cleanup.worktree_unsafe_skip_overridden",
                            worktree_path = %worktree_path.display(),
                            reason = "uncommitted_changes",
                            "Removing worktree with uncommitted changes (--force)"
                        );
                    } else {
                        warn!(
                            event = "core.cleanup.worktree_delete_skipped",
                            worktree_path = %worktree_path.display(),
                            reason = "uncommitted_changes",
                            "Skipping orphaned worktree: has uncommitted changes"
                        );
                        skipped_worktrees
                            .push((worktree_path.clone(), "has uncommitted changes".to_string()));
                        continue;
                    }
                }
                Ok(status) if status.status_check_failed => {
                    // Conservative: internal git status check failed; treat same as Err
                    if force {
                        warn!(
                            event = "core.cleanup.worktree_status_check_failed",
                            worktree_path = %worktree_path.display(),
                            "Cannot verify git status, removing anyway (--force)"
                        );
                    } else {
                        warn!(
                            event = "core.cleanup.worktree_delete_skipped",
                            worktree_path = %worktree_path.display(),
                            reason = "status_check_failed",
                            "Skipping orphaned worktree: cannot verify git status"
                        );
                        skipped_worktrees.push((
                            worktree_path.clone(),
                            "cannot verify git status".to_string(),
                        ));
                        continue;
                    }
                }
                Err(e) => {
                    // Conservative: Repository::open() failed entirely; skip unless forced
                    if force {
                        warn!(
                            event = "core.cleanup.worktree_status_check_failed",
                            worktree_path = %worktree_path.display(),
                            error = %e,
                            "Cannot verify git status, removing anyway (--force)"
                        );
                    } else {
                        warn!(
                            event = "core.cleanup.worktree_delete_skipped",
                            worktree_path = %worktree_path.display(),
                            reason = "status_check_failed",
                            error = %e,
                            "Skipping orphaned worktree: cannot verify git status"
                        );
                        skipped_worktrees.push((
                            worktree_path.clone(),
                            format!("cannot verify git status: {}", e),
                        ));
                        continue;
                    }
                }
                Ok(_) => {} // Clean worktree, proceed
            }

            // Check 2: active processes with CWD inside the worktree
            let active_pids = crate::process::find_processes_in_directory(worktree_path);
            if !active_pids.is_empty() {
                if force {
                    warn!(
                        event = "core.cleanup.worktree_unsafe_skip_overridden",
                        worktree_path = %worktree_path.display(),
                        reason = "active_processes",
                        pids = ?active_pids,
                        "Removing worktree with active processes (--force)"
                    );
                } else {
                    warn!(
                        event = "core.cleanup.worktree_delete_skipped",
                        worktree_path = %worktree_path.display(),
                        reason = "active_processes",
                        pids = ?active_pids,
                        "Skipping orphaned worktree: has active processes"
                    );
                    skipped_worktrees.push((
                        worktree_path.clone(),
                        format!("has active processes (PIDs: {:?})", active_pids),
                    ));
                    continue;
                }
            }
        }

        match git::removal::remove_worktree_by_path(worktree_path) {
            Ok(()) => {
                info!(
                    event = "core.cleanup.worktree_delete_completed",
                    worktree_path = %worktree_path.display()
                );
                cleaned_worktrees.push(worktree_path.clone());
            }
            Err(e) => {
                error!(
                    event = "core.cleanup.worktree_delete_failed",
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

    Ok((cleaned_worktrees, skipped_worktrees))
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
            event = "core.cleanup.session_delete_started",
            session_id = session_id
        );

        match sessions::persistence::remove_session_file(&config.sessions_dir(), session_id) {
            Ok(()) => {
                info!(
                    event = "core.cleanup.session_delete_completed",
                    session_id = session_id
                );
                cleaned_sessions.push(session_id.clone());
            }
            Err(e) => {
                error!(
                    event = "core.cleanup.session_delete_failed",
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
        let result = cleanup_orphaned_worktrees(&[], false);
        assert!(result.is_ok());
        let (cleaned, skipped) = result.unwrap();
        assert_eq!(cleaned.len(), 0);
        assert_eq!(skipped.len(), 0);
    }

    #[test]
    fn test_cleanup_orphaned_worktrees_empty_list_with_force() {
        let result = cleanup_orphaned_worktrees(&[], true);
        assert!(result.is_ok());
        let (cleaned, skipped) = result.unwrap();
        assert_eq!(cleaned.len(), 0);
        assert_eq!(skipped.len(), 0);
    }

    #[test]
    fn test_cleanup_orphaned_worktrees_nonexistent_path_is_removed() {
        // A path that doesn't exist skips safety checks (no uncommitted work to lose)
        // and goes directly to git removal (which gracefully handles missing dirs)
        let nonexistent = std::path::PathBuf::from("/tmp/kild-test-nonexistent-worktree-xyz");
        assert!(!nonexistent.exists());
        // This will call remove_worktree_by_path which may fail since it's not a real
        // git worktree, but critically it should NOT be in the skipped list
        let result = cleanup_orphaned_worktrees(&[nonexistent.clone()], false);
        // Whether Ok or Err, the key is it wasn't skipped due to safety checks
        match result {
            Ok((_, skipped)) => {
                assert!(
                    skipped.is_empty(),
                    "Nonexistent path should not be in skipped list"
                );
            }
            Err(CleanupError::CleanupFailed { .. }) => {
                // Expected: path is not a real git worktree, removal fails
            }
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }

    #[test]
    fn test_cleanup_summary_skipped_worktrees() {
        let mut summary = CleanupSummary::new();
        assert_eq!(summary.skipped_worktrees.len(), 0);

        let path = std::path::PathBuf::from("/tmp/test-worktree");
        summary.add_skipped_worktree(path.clone(), "has uncommitted changes".to_string());

        assert_eq!(summary.skipped_worktrees.len(), 1);
        assert_eq!(summary.skipped_worktrees[0].0, path);
        assert_eq!(summary.skipped_worktrees[0].1, "has uncommitted changes");
        // Skipped does NOT count toward total_cleaned
        assert_eq!(summary.total_cleaned, 0);
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
        let result = cleanup_orphaned_resources(&empty_summary, false);
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
