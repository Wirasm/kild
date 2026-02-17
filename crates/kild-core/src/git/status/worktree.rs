use std::path::Path;

use git2::{Repository, Status, StatusOptions};
use tracing::warn;

use crate::git::errors::GitError;
use crate::git::types::{DiffStats, UncommittedDetails, WorktreeStatus};

use super::count_unpushed_commits;

/// Get diff statistics for unstaged changes in a worktree.
///
/// Returns the number of insertions, deletions, and files changed
/// between the index (staging area) and the working directory.
/// This does not include staged changes.
///
/// # Errors
///
/// Returns `GitError::Git2Error` if the repository cannot be opened
/// or the diff cannot be computed.
pub fn get_diff_stats(worktree_path: &Path) -> Result<DiffStats, GitError> {
    let repo = Repository::open(worktree_path).map_err(|e| GitError::Git2Error { source: e })?;

    let diff = repo
        .diff_index_to_workdir(None, None)
        .map_err(|e| GitError::Git2Error { source: e })?;

    let stats = diff
        .stats()
        .map_err(|e| GitError::Git2Error { source: e })?;

    Ok(DiffStats {
        insertions: stats.insertions(),
        deletions: stats.deletions(),
        files_changed: stats.files_changed(),
    })
}

/// Get comprehensive worktree status for destroy safety checks.
///
/// Returns information about:
/// - Uncommitted changes (staged, modified, untracked files)
/// - Unpushed commits (commits ahead of remote tracking branch)
/// - Remote branch existence
///
/// # Conservative Fallback
///
/// If status checks fail, the function returns a conservative fallback that
/// assumes uncommitted changes exist. This prevents data loss by requiring
/// the user to verify manually. Check `status_check_failed` to detect this.
///
/// # Errors
///
/// Returns `GitError::Git2Error` if the repository cannot be opened.
pub fn get_worktree_status(worktree_path: &Path) -> Result<WorktreeStatus, GitError> {
    let repo = Repository::open(worktree_path).map_err(|e| GitError::Git2Error { source: e })?;

    // 1. Check for uncommitted changes using git2 status
    let (uncommitted_result, status_check_failed) = check_uncommitted_changes(&repo);

    // 2. Count unpushed/behind commits and check remote branch existence
    let commit_counts = count_unpushed_commits(&repo);

    // Determine if there are uncommitted changes
    // Conservative fallback: assume dirty if check failed
    let has_uncommitted = match &uncommitted_result {
        Some(details) => !details.is_empty(),
        None => true, // Conservative: assume dirty if check failed
    };

    Ok(WorktreeStatus {
        has_uncommitted_changes: has_uncommitted,
        unpushed_commit_count: commit_counts.ahead,
        behind_commit_count: commit_counts.behind,
        has_remote_branch: commit_counts.has_remote,
        uncommitted_details: uncommitted_result,
        behind_count_failed: commit_counts.behind_count_failed,
        status_check_failed,
    })
}

/// Check for uncommitted changes in the repository.
///
/// Returns (Option<details>, status_check_failed).
/// - `Some(details)` with file counts when check succeeds
/// - `None` when check fails (status_check_failed will be true)
///
/// The caller should treat `None` as "assume uncommitted changes exist"
/// to be conservative and prevent data loss.
pub(super) fn check_uncommitted_changes(repo: &Repository) -> (Option<UncommittedDetails>, bool) {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true);
    opts.include_ignored(false);

    let statuses = match repo.statuses(Some(&mut opts)) {
        Ok(s) => s,
        Err(e) => {
            warn!(
                event = "core.git.status_check_failed",
                error = %e,
                "Failed to get git status - assuming dirty to be safe"
            );
            // Return None to indicate check failed, true for status_check_failed
            return (None, true);
        }
    };

    let mut staged_files = 0;
    let mut modified_files = 0;
    let mut untracked_files = 0;

    for entry in statuses.iter() {
        let status = entry.status();

        // Check for staged changes (index changes)
        if status.intersects(
            Status::INDEX_NEW
                | Status::INDEX_MODIFIED
                | Status::INDEX_DELETED
                | Status::INDEX_RENAMED
                | Status::INDEX_TYPECHANGE,
        ) {
            staged_files += 1;
        }

        // Check for unstaged modifications to tracked files
        if status.intersects(
            Status::WT_MODIFIED | Status::WT_DELETED | Status::WT_RENAMED | Status::WT_TYPECHANGE,
        ) {
            modified_files += 1;
        }

        // Check for untracked files
        if status.contains(Status::WT_NEW) {
            untracked_files += 1;
        }
    }

    let details = UncommittedDetails {
        staged_files,
        modified_files,
        untracked_files,
    };

    // Return Some(details) even if empty - caller uses is_empty() to check
    (Some(details), false)
}
