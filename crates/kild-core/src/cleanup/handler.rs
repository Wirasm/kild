use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

use crate::cleanup::{errors::CleanupError, operations, types::*};
use crate::git;
use crate::sessions;
use kild_config::Config;

pub fn scan_for_orphans() -> Result<CleanupSummary, CleanupError> {
    info!(event = "core.cleanup.scan_started");

    // Validate we're in a git repository
    operations::validate_cleanup_request()?;

    let current_dir = std::env::current_dir().map_err(|e| CleanupError::IoError { source: e })?;

    let mut summary = CleanupSummary::new();

    // Detect orphaned branches
    let orphaned_branches = operations::detect_orphaned_branches(&current_dir).map_err(|e| {
        error!(event = "core.cleanup.scan_branches_failed", error = %e);
        e
    })?;
    info!(
        event = "core.cleanup.scan_branches_completed",
        count = orphaned_branches.len()
    );
    for branch in orphaned_branches {
        summary.add_branch(branch);
    }

    // Detect orphaned worktrees
    let orphaned_worktrees = operations::detect_orphaned_worktrees(&current_dir).map_err(|e| {
        error!(event = "core.cleanup.scan_worktrees_failed", error = %e);
        e
    })?;
    info!(
        event = "core.cleanup.scan_worktrees_completed",
        count = orphaned_worktrees.len()
    );
    for worktree_path in orphaned_worktrees {
        summary.add_worktree(worktree_path);
    }

    // Detect stale sessions
    let config = Config::new();
    let stale_sessions =
        operations::detect_stale_sessions(&config.sessions_dir()).map_err(|e| {
            error!(event = "core.cleanup.scan_sessions_failed", error = %e);
            e
        })?;
    info!(
        event = "core.cleanup.scan_sessions_completed",
        count = stale_sessions.len()
    );
    for session_id in stale_sessions {
        summary.add_session(session_id);
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
        let cleaned_branches =
            cleanup_orphaned_branches(&summary.orphaned_branches).map_err(|e| {
                error!(event = "core.cleanup.cleanup_branches_failed", error = %e);
                e
            })?;
        for branch in cleaned_branches {
            cleaned_summary.add_branch(branch);
        }
    }

    // Clean up orphaned worktrees
    if !summary.orphaned_worktrees.is_empty() {
        let (cleaned_worktrees, skipped_worktrees) =
            cleanup_orphaned_worktrees(&summary.orphaned_worktrees, force).map_err(|e| {
                error!(event = "core.cleanup.cleanup_worktrees_failed", error = %e);
                e
            })?;
        for worktree_path in cleaned_worktrees {
            cleaned_summary.add_worktree(worktree_path);
        }
        for (path, reason) in skipped_worktrees {
            cleaned_summary.add_skipped_worktree(path, reason);
        }
    }

    // Clean up stale sessions (also removes associated worktrees)
    if !summary.stale_sessions.is_empty() {
        let (cleaned_sessions, skipped) = cleanup_stale_sessions(&summary.stale_sessions, force)
            .map_err(|e| {
                error!(event = "core.cleanup.cleanup_sessions_failed", error = %e);
                e
            })?;
        for session_id in cleaned_sessions {
            cleaned_summary.add_session(session_id);
        }
        for (path, reason) in skipped {
            cleaned_summary.add_skipped_worktree(path, reason);
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
    git::ensure_in_repo(&current_dir).map_err(|e| {
        error!(event = "core.cleanup.git_discovery_failed", error = %e);
        CleanupError::GitError { source: e }
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
                operations::detect_sessions_older_than(&config.sessions_dir(), days).map_err(
                    |e| {
                        error!(event = "core.cleanup.strategy_failed", strategy = "OlderThan", error = %e);
                        CleanupError::StrategyFailed {
                            strategy: format!("OlderThan({})", days),
                            source: Box::new(e),
                        }
                    },
                )?;
            for session_id in sessions {
                summary.add_session(session_id);
            }
        }
        CleanupStrategy::Orphans => {
            // Get current project info for scoping
            let project = git::detect_project().map_err(|e| {
                error!(event = "core.cleanup.strategy_failed", strategy = "Orphans", error = %e);
                CleanupError::GitError { source: e }
            })?;

            // Detect untracked worktrees (in kild dir but no session)
            let untracked = operations::detect_untracked_worktrees(
                &project.path,
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
            let orphaned_branches =
                operations::detect_orphaned_branches(&project.path).map_err(|e| {
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

    let mut cleaned_branches = Vec::new();

    for branch_name in branches {
        info!(
            event = "core.cleanup.branch_delete_started",
            branch = branch_name
        );

        match git::delete_local_branch(&current_dir, branch_name) {
            Ok(true) => {
                info!(
                    event = "core.cleanup.branch_delete_completed",
                    branch = branch_name
                );
                cleaned_branches.push(branch_name.clone());
            }
            Ok(false) => {
                // Branch already gone (not found or race condition)
                info!(
                    event = "core.cleanup.branch_delete_race_condition",
                    branch = branch_name,
                    message = "Branch was already deleted - considering as cleaned"
                );
                cleaned_branches.push(branch_name.clone());
            }
            Err(e) => {
                error!(
                    event = "core.cleanup.branch_delete_failed",
                    branch = branch_name,
                    error = %e,
                    error_type = "permission_or_lock_error"
                );
                return Err(CleanupError::CleanupFailed {
                    name: branch_name.clone(),
                    message: format!("Failed to delete branch: {}", e),
                });
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
                    // Conservative: git status check failed entirely; skip unless forced
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

/// Load minimal session data needed for worktree cleanup.
///
/// Returns `(worktree_path, use_main_worktree, branch)` or `None` if the
/// session file can't be read or parsed.
fn load_session_for_cleanup(
    sessions_dir: &Path,
    session_id: &str,
) -> Option<(PathBuf, bool, String)> {
    let safe_id = session_id.replace('/', "_");

    // Try new format: <sessions_dir>/<safe_id>/kild.json
    let new_path = sessions_dir.join(&safe_id).join("kild.json");
    let content = if new_path.exists() {
        match std::fs::read_to_string(&new_path) {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    event = "core.cleanup.session_file_read_failed",
                    session_id = session_id,
                    path = %new_path.display(),
                    error = %e,
                );
                return None;
            }
        }
    } else {
        // Try legacy format: <sessions_dir>/<safe_id>.json
        let legacy_path = sessions_dir.join(format!("{safe_id}.json"));
        match std::fs::read_to_string(&legacy_path) {
            Ok(c) => c,
            Err(e) => {
                debug!(
                    event = "core.cleanup.session_file_not_found",
                    session_id = session_id,
                    path = %legacy_path.display(),
                    error = %e,
                );
                return None;
            }
        }
    };

    let session: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            warn!(
                event = "core.cleanup.session_file_parse_failed",
                session_id = session_id,
                error = %e,
            );
            return None;
        }
    };

    let worktree_path = session.get("worktree_path")?.as_str().map(PathBuf::from)?;
    let use_main_worktree = session
        .get("use_main_worktree")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let branch = session.get("branch")?.as_str()?.to_string();

    Some((worktree_path, use_main_worktree, branch))
}

type SessionCleanupResult = (Vec<String>, Vec<(PathBuf, String)>);

fn cleanup_stale_sessions(
    session_ids: &[String],
    force: bool,
) -> Result<SessionCleanupResult, CleanupError> {
    let config = Config::new();
    cleanup_stale_sessions_in(&config.sessions_dir(), session_ids, force)
}

/// Inner implementation that accepts `sessions_dir` for testability.
fn cleanup_stale_sessions_in(
    sessions_dir: &Path,
    session_ids: &[String],
    force: bool,
) -> Result<SessionCleanupResult, CleanupError> {
    // Early return for empty list
    if session_ids.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut cleaned_sessions = Vec::new();
    let mut skipped_worktrees: Vec<(PathBuf, String)> = Vec::new();

    for session_id in session_ids {
        info!(
            event = "core.cleanup.session_delete_started",
            session_id = session_id
        );

        // Load session data for worktree cleanup
        let session_data = load_session_for_cleanup(sessions_dir, session_id);

        if let Some((ref worktree_path, use_main_worktree, ref branch)) = session_data {
            if use_main_worktree {
                // --main sessions: worktree_path IS the project root.
                // Never remove the project root directory.
                info!(
                    event = "core.cleanup.session_worktree_skipped",
                    session_id = session_id,
                    reason = "main_worktree",
                );
            } else if worktree_path.exists() {
                // Safety: check for uncommitted changes unless --force
                if !force {
                    match git::get_worktree_status(worktree_path) {
                        Ok(status)
                            if status.has_uncommitted_changes && !status.status_check_failed =>
                        {
                            warn!(
                                event = "core.cleanup.session_delete_skipped",
                                session_id = session_id,
                                worktree_path = %worktree_path.display(),
                                reason = "uncommitted_changes",
                            );
                            skipped_worktrees.push((
                                worktree_path.clone(),
                                "has uncommitted changes".to_string(),
                            ));
                            continue;
                        }
                        Ok(status) if status.status_check_failed => {
                            warn!(
                                event = "core.cleanup.session_delete_skipped",
                                session_id = session_id,
                                worktree_path = %worktree_path.display(),
                                reason = "status_check_failed",
                            );
                            skipped_worktrees.push((
                                worktree_path.clone(),
                                "cannot verify git status".to_string(),
                            ));
                            continue;
                        }
                        Err(e) => {
                            warn!(
                                event = "core.cleanup.session_delete_skipped",
                                session_id = session_id,
                                worktree_path = %worktree_path.display(),
                                reason = "status_check_failed",
                                error = %e,
                            );
                            skipped_worktrees.push((
                                worktree_path.clone(),
                                format!("cannot verify git status: {e}"),
                            ));
                            continue;
                        }
                        Ok(_) => {} // Clean worktree, proceed
                    }
                }

                // Resolve main repo path before worktree removal (needed for
                // branch cleanup — the .git file disappears with the worktree).
                let main_repo_path = git::removal::find_main_repo_root(worktree_path);

                // Remove worktree (force removes even with uncommitted changes)
                let removal_result = if force {
                    git::removal::remove_worktree_force(worktree_path)
                } else {
                    git::removal::remove_worktree_by_path(worktree_path)
                };

                match removal_result {
                    Ok(()) => {
                        info!(
                            event = "core.cleanup.session_worktree_remove_completed",
                            session_id = session_id,
                            worktree_path = %worktree_path.display(),
                        );

                        // Delete kild branch (best-effort, same as destroy)
                        match &main_repo_path {
                            Some(repo_path) => {
                                let kild_branch = git::naming::kild_branch_name(branch);
                                git::removal::delete_branch_if_exists(repo_path, &kild_branch);
                            }
                            None => {
                                warn!(
                                    event = "core.cleanup.session_branch_cleanup_skipped",
                                    session_id = session_id,
                                    branch = branch,
                                    reason = "could not resolve main repo root",
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            event = "core.cleanup.session_worktree_remove_failed",
                            session_id = session_id,
                            worktree_path = %worktree_path.display(),
                            error = %e,
                        );
                        skipped_worktrees.push((
                            worktree_path.clone(),
                            format!("worktree removal failed: {e}"),
                        ));
                        continue;
                    }
                }
            } else {
                // Worktree directory already gone — just clean up session file
                debug!(
                    event = "core.cleanup.session_worktree_remove_skipped",
                    session_id = session_id,
                    worktree_path = %worktree_path.display(),
                );
            }
        } else {
            // Session data couldn't be loaded — still remove the session file
            // to clean up corrupted/incomplete session entries.
            warn!(
                event = "core.cleanup.session_data_load_failed",
                session_id = session_id,
                "Could not load session data for worktree cleanup — removing session file only"
            );
        }

        // Remove session file
        match sessions::persistence::remove_session_file(sessions_dir, session_id) {
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

    Ok((cleaned_sessions, skipped_worktrees))
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
        let result = cleanup_stale_sessions(&[], false);
        assert!(result.is_ok());
        let (cleaned, skipped) = result.unwrap();
        assert_eq!(cleaned.len(), 0);
        assert_eq!(skipped.len(), 0);
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

    #[test]
    fn test_load_session_for_cleanup_new_format() {
        let tmp = tempfile::tempdir().unwrap();
        let sessions_dir = tmp.path();

        // Create a session in new format: <safe_id>/kild.json
        let session_dir = sessions_dir.join("test-session");
        std::fs::create_dir_all(&session_dir).unwrap();
        let content = serde_json::json!({
            "id": "test-session",
            "worktree_path": "/tmp/kild-test-wt",
            "branch": "test-branch",
            "use_main_worktree": false,
        });
        std::fs::write(session_dir.join("kild.json"), content.to_string()).unwrap();

        let result = load_session_for_cleanup(sessions_dir, "test-session");
        assert!(result.is_some());
        let (wt_path, use_main, branch) = result.unwrap();
        assert_eq!(wt_path, PathBuf::from("/tmp/kild-test-wt"));
        assert!(!use_main);
        assert_eq!(branch, "test-branch");
    }

    #[test]
    fn test_load_session_for_cleanup_legacy_format() {
        let tmp = tempfile::tempdir().unwrap();
        let sessions_dir = tmp.path();

        // Create a session in legacy format: <safe_id>.json
        let content = serde_json::json!({
            "id": "legacy-session",
            "worktree_path": "/tmp/kild-legacy-wt",
            "branch": "legacy-branch",
        });
        std::fs::write(
            sessions_dir.join("legacy-session.json"),
            content.to_string(),
        )
        .unwrap();

        let result = load_session_for_cleanup(sessions_dir, "legacy-session");
        assert!(result.is_some());
        let (wt_path, use_main, branch) = result.unwrap();
        assert_eq!(wt_path, PathBuf::from("/tmp/kild-legacy-wt"));
        assert!(!use_main); // default when field is absent
        assert_eq!(branch, "legacy-branch");
    }

    #[test]
    fn test_load_session_for_cleanup_main_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let sessions_dir = tmp.path();

        let session_dir = sessions_dir.join("main-session");
        std::fs::create_dir_all(&session_dir).unwrap();
        let content = serde_json::json!({
            "id": "main-session",
            "worktree_path": "/home/user/project",
            "branch": "main-branch",
            "use_main_worktree": true,
        });
        std::fs::write(session_dir.join("kild.json"), content.to_string()).unwrap();

        let result = load_session_for_cleanup(sessions_dir, "main-session");
        assert!(result.is_some());
        let (_, use_main, _) = result.unwrap();
        assert!(use_main);
    }

    #[test]
    fn test_load_session_for_cleanup_missing_session() {
        let tmp = tempfile::tempdir().unwrap();
        let result = load_session_for_cleanup(tmp.path(), "nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_load_session_for_cleanup_invalid_json() {
        let tmp = tempfile::tempdir().unwrap();
        let sessions_dir = tmp.path();

        let session_dir = sessions_dir.join("bad-session");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(session_dir.join("kild.json"), "not valid json").unwrap();

        let result = load_session_for_cleanup(sessions_dir, "bad-session");
        assert!(result.is_none());
    }

    /// Helper: create a git repo with initial commit, a kild branch, and worktree.
    /// Returns (canonical_worktree_path, _repo_dir_guard, _worktree_base_guard).
    fn create_test_worktree(
        branch_suffix: &str,
    ) -> (PathBuf, tempfile::TempDir, tempfile::TempDir) {
        let repo_dir = tempfile::tempdir().unwrap();
        let worktree_base = tempfile::tempdir().unwrap();

        git::test_support::init_repo_with_commit(repo_dir.path()).unwrap();

        let branch_name = format!("kild/{branch_suffix}");
        let admin_name = format!("kild-{branch_suffix}");
        git::test_support::create_branch(repo_dir.path(), &branch_name).unwrap();

        let worktree_path = worktree_base.path().join(&admin_name);
        git::test_support::create_worktree_for_branch(
            repo_dir.path(),
            &admin_name,
            &worktree_path,
            &branch_name,
        )
        .unwrap();

        let canonical = worktree_path.canonicalize().unwrap();
        (canonical, repo_dir, worktree_base)
    }

    /// Helper: write a minimal session file for cleanup tests.
    fn write_cleanup_session(
        sessions_dir: &std::path::Path,
        session_id: &str,
        worktree_path: &std::path::Path,
        branch: &str,
        use_main_worktree: bool,
    ) {
        let session_dir = sessions_dir.join(session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        let content = serde_json::json!({
            "id": session_id,
            "project_id": "test-project",
            "branch": branch,
            "worktree_path": worktree_path.to_str().unwrap(),
            "agent": "claude",
            "status": "stopped",
            "created_at": "2025-01-01T00:00:00Z",
            "use_main_worktree": use_main_worktree,
        });
        std::fs::write(session_dir.join("kild.json"), content.to_string()).unwrap();
    }

    #[test]
    fn test_cleanup_stale_sessions_removes_worktree_and_session() {
        let (canonical_worktree, _repo_guard, _wt_guard) =
            create_test_worktree("cleanup-full-test");
        assert!(canonical_worktree.exists());

        let sessions_dir = tempfile::tempdir().unwrap();
        write_cleanup_session(
            sessions_dir.path(),
            "full-test-session",
            &canonical_worktree,
            "cleanup-full-test",
            false,
        );

        // Call the inner function directly with our temp sessions dir
        let result = cleanup_stale_sessions_in(
            sessions_dir.path(),
            &["full-test-session".to_string()],
            false,
        );

        assert!(result.is_ok(), "Cleanup should succeed");
        let (cleaned, skipped) = result.unwrap();
        assert_eq!(cleaned, vec!["full-test-session"]);
        assert!(skipped.is_empty(), "Nothing should be skipped");

        // Worktree directory must be gone
        assert!(
            !canonical_worktree.exists(),
            "Worktree directory should be removed"
        );
        // Session file must be gone
        assert!(
            !sessions_dir.path().join("full-test-session").exists(),
            "Session directory should be removed"
        );
    }

    #[test]
    fn test_cleanup_stale_sessions_skips_uncommitted_changes() {
        let (canonical_worktree, _repo_guard, _wt_guard) =
            create_test_worktree("cleanup-dirty-test");
        assert!(canonical_worktree.exists());

        // Create an uncommitted file in the worktree
        std::fs::write(
            canonical_worktree.join("dirty-file.txt"),
            "uncommitted work",
        )
        .unwrap();

        let sessions_dir = tempfile::tempdir().unwrap();
        write_cleanup_session(
            sessions_dir.path(),
            "dirty-session",
            &canonical_worktree,
            "cleanup-dirty-test",
            false,
        );

        // Without --force: should skip
        let result =
            cleanup_stale_sessions_in(sessions_dir.path(), &["dirty-session".to_string()], false);

        assert!(result.is_ok());
        let (cleaned, skipped) = result.unwrap();
        assert!(
            cleaned.is_empty(),
            "Session should NOT be cleaned (uncommitted changes)"
        );
        assert_eq!(skipped.len(), 1, "Worktree should be in skipped list");
        assert_eq!(skipped[0].0, canonical_worktree);
        assert!(
            skipped[0].1.contains("uncommitted"),
            "Skip reason should mention uncommitted changes, got: {}",
            skipped[0].1,
        );

        // Worktree must still exist
        assert!(canonical_worktree.exists(), "Worktree should be preserved");
        // Session file must still exist
        assert!(
            sessions_dir.path().join("dirty-session").exists(),
            "Session file should be preserved"
        );
    }

    #[test]
    fn test_cleanup_stale_sessions_force_removes_uncommitted_changes() {
        let (canonical_worktree, _repo_guard, _wt_guard) =
            create_test_worktree("cleanup-force-test");

        // Create an uncommitted file
        std::fs::write(canonical_worktree.join("dirty-file.txt"), "will be lost").unwrap();

        let sessions_dir = tempfile::tempdir().unwrap();
        write_cleanup_session(
            sessions_dir.path(),
            "force-session",
            &canonical_worktree,
            "cleanup-force-test",
            false,
        );

        // With --force: should remove despite uncommitted changes
        let result =
            cleanup_stale_sessions_in(sessions_dir.path(), &["force-session".to_string()], true);

        assert!(result.is_ok());
        let (cleaned, skipped) = result.unwrap();
        assert_eq!(cleaned, vec!["force-session"]);
        assert!(skipped.is_empty(), "Nothing should be skipped with --force");

        // Worktree must be gone
        assert!(
            !canonical_worktree.exists(),
            "Worktree should be force-removed"
        );
    }

    #[test]
    fn test_cleanup_stale_sessions_worktree_already_gone() {
        let sessions_dir = tempfile::tempdir().unwrap();
        let nonexistent = PathBuf::from("/tmp/kild-test-nonexistent-cleanup-wt-xyz");
        assert!(!nonexistent.exists());

        write_cleanup_session(
            sessions_dir.path(),
            "gone-session",
            &nonexistent,
            "gone-branch",
            false,
        );

        // Worktree doesn't exist — should still clean up session file
        let result =
            cleanup_stale_sessions_in(sessions_dir.path(), &["gone-session".to_string()], false);

        assert!(result.is_ok());
        let (cleaned, skipped) = result.unwrap();
        assert_eq!(cleaned, vec!["gone-session"]);
        assert!(skipped.is_empty());
        // Session file must be gone
        assert!(!sessions_dir.path().join("gone-session").exists());
    }

    #[test]
    fn test_cleanup_stale_sessions_main_worktree_preserves_directory() {
        let project_dir = tempfile::tempdir().unwrap();
        let sessions_dir = tempfile::tempdir().unwrap();

        // Create a sentinel file to verify the directory isn't deleted
        std::fs::write(project_dir.path().join("Cargo.toml"), "[package]").unwrap();

        write_cleanup_session(
            sessions_dir.path(),
            "main-session",
            project_dir.path(),
            "main-branch",
            true, // use_main_worktree = true
        );

        let result =
            cleanup_stale_sessions_in(sessions_dir.path(), &["main-session".to_string()], false);

        assert!(result.is_ok());
        let (cleaned, skipped) = result.unwrap();
        // Session file should still be cleaned up
        assert_eq!(cleaned, vec!["main-session"]);
        assert!(skipped.is_empty());
        // Project directory must still exist (NOT removed!)
        assert!(
            project_dir.path().exists(),
            "Main worktree directory should NOT be removed"
        );
        assert!(
            project_dir.path().join("Cargo.toml").exists(),
            "Project files should be preserved"
        );
    }

    #[test]
    fn test_load_session_for_cleanup_escapes_slashes() {
        let tmp = tempfile::tempdir().unwrap();
        let sessions_dir = tmp.path();

        // Session ID with slash (gets escaped to underscore in file path)
        let session_dir = sessions_dir.join("kild_test-branch");
        std::fs::create_dir_all(&session_dir).unwrap();
        let content = serde_json::json!({
            "id": "kild/test-branch",
            "worktree_path": "/tmp/kild-test-wt",
            "branch": "test-branch",
        });
        std::fs::write(session_dir.join("kild.json"), content.to_string()).unwrap();

        let result = load_session_for_cleanup(sessions_dir, "kild/test-branch");
        assert!(result.is_some());
    }
}
