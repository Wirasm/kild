use std::path::Path;

use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::git;
use crate::sessions::{errors::SessionError, persistence, types::*};

/// Completes a kild by checking PR status, optionally deleting remote branch, and destroying the session.
///
/// # Arguments
/// * `name` - Branch name or kild identifier
///
/// # Returns
/// * `Ok(CompleteResult::RemoteDeleted)` - PR was merged and remote branch was deleted
/// * `Ok(CompleteResult::RemoteDeleteFailed)` - PR was merged but remote deletion failed (non-fatal)
/// * `Ok(CompleteResult::PrNotMerged)` - PR not merged, remote preserved for future merge
/// * `Ok(CompleteResult::PrCheckUnavailable)` - Could not verify PR merge status
///
/// # Errors
/// Returns `SessionError::NotFound` if the session doesn't exist.
/// Returns `SessionError::UncommittedChanges` if the worktree has uncommitted changes.
/// Propagates errors from `destroy_session`.
/// Remote branch deletion errors are logged but do not fail the operation.
///
/// # Workflow Detection
/// - If PR is merged: attempts to delete remote branch (since gh merge --delete-branch would have failed due to worktree)
/// - If PR not merged: just destroys the local session, allowing user's subsequent merge to handle remote cleanup
pub fn complete_session(name: &str) -> Result<CompleteResult, SessionError> {
    info!(event = "core.session.complete_started", name = name);

    let config = Config::new();
    let forge_override = crate::config::KildConfig::load_hierarchy()
        .ok()
        .and_then(|c| c.git.forge());

    // 1. Find session by name to get branch info
    let session =
        persistence::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    let kild_branch = git::operations::kild_branch_name(name);

    // 2. Check if PR was merged (determines if we need to delete remote)
    // Skip PR check entirely for repos without a remote configured
    let pr_merged = if super::destroy::has_remote_configured(&session.worktree_path) {
        match crate::forge::get_forge_backend(&session.worktree_path, forge_override) {
            Some(backend) => match backend.is_pr_merged(&session.worktree_path, &kild_branch) {
                Ok(merged) => Some(merged),
                Err(e) => {
                    warn!(
                        event = "core.session.complete_pr_check_failed",
                        branch = name,
                        error = %e,
                    );
                    None
                }
            },
            None => {
                debug!(event = "core.session.complete_no_forge", branch = name,);
                None
            }
        }
    } else {
        debug!(
            event = "core.session.complete_no_remote",
            branch = name,
            "No remote configured — skipping PR check"
        );
        None
    };

    info!(
        event = "core.session.complete_pr_status",
        branch = name,
        pr_merged = ?pr_merged
    );

    // 3. Determine the result based on PR status and remote deletion outcome
    let result = if pr_merged == Some(true) {
        // PR was merged - attempt to delete remote branch
        match crate::git::cli::delete_remote_branch(&session.worktree_path, "origin", &kild_branch)
        {
            Ok(()) => {
                info!(
                    event = "core.session.complete_remote_deleted",
                    branch = kild_branch
                );
                CompleteResult::RemoteDeleted
            }
            Err(e) => {
                warn!(
                    event = "core.session.complete_remote_delete_failed",
                    branch = kild_branch,
                    worktree_path = %session.worktree_path.display(),
                    error = %e
                );
                CompleteResult::RemoteDeleteFailed
            }
        }
    } else if pr_merged == Some(false) {
        CompleteResult::PrNotMerged
    } else {
        CompleteResult::PrCheckUnavailable
    };

    // 4. Safety check: always block on uncommitted changes (no --force bypass for complete)
    let safety_info = super::destroy::get_destroy_safety_info(name)?;
    if safety_info.should_block() {
        error!(
            event = "core.session.complete_blocked",
            name = name,
            reason = "uncommitted_changes"
        );
        return Err(SessionError::UncommittedChanges {
            name: name.to_string(),
        });
    }

    // 5. Destroy the session (reuse existing logic, always non-force since we already
    //    verified the worktree is clean above)
    super::destroy::destroy_session(name, false)?;

    info!(
        event = "core.session.complete_completed",
        name = name,
        result = ?result
    );

    Ok(result)
}

/// Fetch rich PR info via the forge backend.
///
/// Delegates to `forge::get_forge_backend()` to determine the correct forge
/// and calls its `fetch_pr_info()` method.
///
/// Returns `None` if no forge detected, CLI unavailable, no PR, or fetch error.
pub fn fetch_pr_info(worktree_path: &Path, branch: &str) -> Option<crate::forge::types::PrInfo> {
    let forge_override = crate::config::KildConfig::load_hierarchy()
        .ok()
        .and_then(|c| c.git.forge());

    let backend = crate::forge::get_forge_backend(worktree_path, forge_override)?;

    match backend.fetch_pr_info(worktree_path, branch) {
        Ok(pr_info) => pr_info,
        Err(e) => {
            warn!(
                event = "core.session.pr_info_fetch_failed",
                branch = branch,
                error = %e,
            );
            None
        }
    }
}

/// Read PR info for a session from the sidecar file.
///
/// Returns `None` if no PR info has been cached yet.
pub fn read_pr_info(session_id: &str) -> Option<crate::forge::types::PrInfo> {
    let config = Config::new();
    persistence::read_pr_info(&config.sessions_dir(), session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_session_not_found() {
        let result = complete_session("non-existent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }
}
