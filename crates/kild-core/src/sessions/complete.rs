use std::path::Path;

use tracing::{debug, error, info, warn};

use crate::forge::types::{CiStatus, PrCheckResult, PrState};
use crate::git;
use crate::sessions::{errors::SessionError, persistence, types::*};
use kild_config::Config;

/// Completes a kild through the full merge lifecycle.
///
/// Default behavior (merge enabled):
/// 1. Check for uncommitted changes (abort unless --force)
/// 2. Check PR exists and is open
/// 3. Fetch PR info (CI status, review status)
/// 4. Check CI is passing (abort unless --force or --skip-ci)
/// 5. Merge the PR using the configured strategy
/// 6. Delete the remote branch
/// 7. Destroy the worktree and session
///
/// With `--no-merge` (legacy behavior):
/// - If PR is already merged: delete remote branch, destroy session
/// - If PR is not merged: just destroy session, preserve remote
///
/// With `--dry-run`:
/// - Walk through all checks and report what would happen, without mutating.
pub fn complete_session(request: &CompleteRequest) -> Result<CompleteResult, SessionError> {
    let name = &request.name;
    info!(
        event = "core.session.complete_started",
        name = name,
        merge_strategy = %request.merge_strategy,
        no_merge = request.no_merge,
        force = request.force,
        dry_run = request.dry_run,
        skip_ci = request.skip_ci,
    );

    let config = Config::new();
    let forge_override = load_forge_override();

    // 1. Find session
    let session =
        persistence::find_session_by_name(&config.sessions_dir(), name)?.ok_or_else(|| {
            SessionError::NotFound {
                name: name.to_string(),
            }
        })?;

    let kild_branch = git::kild_branch_name(name);

    // 2. Check uncommitted changes
    let safety_info = super::destroy::get_destroy_safety_info(name)?;
    if safety_info.should_block() && !request.force {
        error!(
            event = "core.session.complete_blocked",
            name = name,
            reason = "uncommitted_changes"
        );
        return Err(SessionError::UncommittedChanges {
            name: name.to_string(),
        });
    }

    // 3. Check remote + forge availability
    let has_remote = super::destroy::has_remote_configured(&session.worktree_path);
    if !has_remote {
        error!(
            event = "core.session.complete_no_pr",
            name = name,
            reason = "no_remote"
        );
        return Err(SessionError::NoPrFound {
            name: name.to_string(),
        });
    }

    let forge_backend = crate::forge::get_forge_backend(&session.worktree_path, forge_override)
        .ok_or_else(|| {
            error!(
                event = "core.session.complete_no_pr",
                name = name,
                reason = "no_forge_backend"
            );
            SessionError::NoPrFound {
                name: name.to_string(),
            }
        })?;

    // 4. --no-merge path uses check_pr_exists + is_pr_merged (lightweight checks)
    if request.no_merge {
        match forge_backend.check_pr_exists(&session.worktree_path, &kild_branch) {
            PrCheckResult::Exists => {
                debug!(event = "core.session.complete_pr_exists", branch = name);
            }
            PrCheckResult::NotFound => {
                error!(
                    event = "core.session.complete_no_pr",
                    name = name,
                    reason = "not_found"
                );
                return Err(SessionError::NoPrFound {
                    name: name.to_string(),
                });
            }
            PrCheckResult::Unavailable => {
                warn!(
                    event = "core.session.complete_pr_check_unavailable",
                    branch = name,
                    "Cannot verify PR status — proceeding anyway"
                );
            }
        }
        return complete_no_merge(
            name,
            &session.worktree_path,
            &kild_branch,
            forge_backend,
            request.force,
            request.dry_run,
        );
    }

    // 5. Default merge path: fetch_pr_info subsumes check_pr_exists (returns state, CI, reviews)
    let pr_info = match forge_backend.fetch_pr_info(&session.worktree_path, &kild_branch) {
        Ok(Some(info)) => info,
        Ok(None) => {
            error!(
                event = "core.session.complete_no_pr",
                name = name,
                reason = "fetch_returned_none"
            );
            return Err(SessionError::NoPrFound {
                name: name.to_string(),
            });
        }
        Err(e) => {
            error!(
                event = "core.session.complete_pr_fetch_failed",
                name = name,
                error = %e
            );
            return Err(SessionError::NoPrFound {
                name: name.to_string(),
            });
        }
    };

    info!(
        event = "core.session.complete_pr_info",
        branch = name,
        pr_number = pr_info.number,
        pr_state = %pr_info.state,
        ci_status = %pr_info.ci_status,
        review_status = %pr_info.review_status,
    );

    // 6. If PR is already merged, skip to cleanup
    if pr_info.state == PrState::Merged {
        info!(
            event = "core.session.complete_already_merged",
            branch = name
        );
        if request.dry_run {
            let mut steps = vec![
                format!("PR #{} is already merged", pr_info.number),
                "Delete remote branch".to_string(),
                "Destroy worktree and session".to_string(),
            ];
            if safety_info.should_block() {
                steps.insert(1, "Force discard uncommitted changes".to_string());
            }
            return Ok(CompleteResult::DryRun { steps });
        }
        let remote_deleted = try_delete_remote(&session.worktree_path, &kild_branch);
        super::destroy::destroy_session(name, request.force)?;
        info!(
            event = "core.session.complete_completed",
            name = name,
            outcome = "already_merged"
        );
        return Ok(CompleteResult::AlreadyMerged { remote_deleted });
    }

    // 7. PR must be open to merge
    if pr_info.state != PrState::Open && pr_info.state != PrState::Draft {
        return Err(SessionError::PrNotOpen {
            name: name.to_string(),
            state: pr_info.state.to_string(),
        });
    }

    // 8. CI check (unless --force or --skip-ci)
    if !request.force && !request.skip_ci && pr_info.ci_status == CiStatus::Failing {
        let summary = pr_info
            .ci_summary
            .as_deref()
            .unwrap_or("checks failing")
            .to_string();
        error!(
            event = "core.session.complete_ci_failing",
            name = name,
            ci_summary = %summary
        );
        return Err(SessionError::CiFailing {
            name: name.to_string(),
            summary,
        });
    }

    // 9. Dry run — report what would happen
    if request.dry_run {
        let mut steps = Vec::new();
        if safety_info.should_block() {
            steps.push("Force discard uncommitted changes".to_string());
        }
        steps.push(format!(
            "Merge PR #{} via {} strategy",
            pr_info.number, request.merge_strategy
        ));
        steps.push("Delete remote branch".to_string());
        steps.push("Destroy worktree and session".to_string());
        return Ok(CompleteResult::DryRun { steps });
    }

    // 10. Merge the PR
    info!(
        event = "core.session.merge_started",
        name = name,
        pr_number = pr_info.number,
        strategy = %request.merge_strategy
    );

    if let Err(e) =
        forge_backend.merge_pr(&session.worktree_path, &kild_branch, request.merge_strategy)
    {
        error!(
            event = "core.session.merge_failed",
            name = name,
            error = %e
        );
        return Err(SessionError::MergeFailed {
            name: name.to_string(),
            message: e.to_string(),
        });
    }

    info!(
        event = "core.session.merge_completed",
        name = name,
        pr_number = pr_info.number,
        strategy = %request.merge_strategy
    );

    // 11. Delete remote branch
    let remote_deleted = try_delete_remote(&session.worktree_path, &kild_branch);

    // 12. Destroy session
    super::destroy::destroy_session(name, request.force)?;

    info!(
        event = "core.session.complete_completed",
        name = name,
        outcome = "merged",
        strategy = %request.merge_strategy
    );

    Ok(CompleteResult::Merged {
        strategy: request.merge_strategy,
        remote_deleted,
    })
}

/// Legacy --no-merge path: check if PR was already merged, then cleanup.
fn complete_no_merge(
    name: &str,
    worktree_path: &Path,
    kild_branch: &str,
    forge_backend: &dyn crate::forge::ForgeBackend,
    force: bool,
    dry_run: bool,
) -> Result<CompleteResult, SessionError> {
    let pr_merged = match forge_backend.is_pr_merged(worktree_path, kild_branch) {
        Ok(merged) => Some(merged),
        Err(e) => {
            warn!(
                event = "core.session.complete_pr_check_failed",
                branch = name,
                error = %e,
            );
            None
        }
    };

    info!(
        event = "core.session.complete_pr_status",
        branch = name,
        pr_merged = ?pr_merged,
        mode = "no_merge"
    );

    if dry_run {
        let mut steps = Vec::new();
        match pr_merged {
            Some(true) => {
                steps.push("PR is already merged".to_string());
                steps.push("Delete remote branch".to_string());
                steps.push("Destroy worktree and session".to_string());
            }
            Some(false) => {
                steps.push("PR is not merged — remote branch preserved".to_string());
                steps.push("Destroy worktree and session".to_string());
            }
            None => {
                steps.push(
                    "PR merge status unknown — would abort (session NOT destroyed)".to_string(),
                );
            }
        }
        return Ok(CompleteResult::DryRun { steps });
    }

    match pr_merged {
        Some(true) => {
            let remote_deleted = try_delete_remote(worktree_path, kild_branch);
            super::destroy::destroy_session(name, force)?;
            info!(
                event = "core.session.complete_completed",
                name = name,
                outcome = "already_merged_no_merge"
            );
            Ok(CompleteResult::AlreadyMerged { remote_deleted })
        }
        Some(false) => {
            super::destroy::destroy_session(name, force)?;
            info!(
                event = "core.session.complete_completed",
                name = name,
                outcome = "cleanup_only"
            );
            Ok(CompleteResult::CleanupOnly)
        }
        None => {
            // Forge check failed (network error, no auth, etc.) — don't destroy
            // on ambiguous state since it's irreversible.
            error!(
                event = "core.session.complete_pr_check_unavailable",
                name = name,
                "Cannot determine PR merge status — aborting to avoid destroying session with unknown state"
            );
            Err(SessionError::NoPrFound {
                name: name.to_string(),
            })
        }
    }
}

/// Attempt to delete a remote branch. Returns true on success, false on failure.
fn try_delete_remote(worktree_path: &Path, kild_branch: &str) -> bool {
    match crate::git::cli::delete_remote_branch(worktree_path, "origin", kild_branch) {
        Ok(()) => {
            info!(
                event = "core.session.complete_remote_deleted",
                branch = kild_branch
            );
            true
        }
        Err(e) => {
            warn!(
                event = "core.session.complete_remote_delete_failed",
                branch = kild_branch,
                worktree_path = %worktree_path.display(),
                error = %e
            );
            false
        }
    }
}

/// Load forge override from config hierarchy (best-effort).
fn load_forge_override() -> Option<crate::forge::ForgeType> {
    kild_config::KildConfig::load_hierarchy()
        .inspect_err(|e| {
            warn!(
                event = "core.session.config_load_failed",
                error = %e,
                "Could not load config for forge override — falling back to auto-detection"
            );
        })
        .ok()
        .and_then(|c| c.git.forge())
}

/// Fetch rich PR info via the forge backend.
///
/// Delegates to `forge::get_forge_backend()` to determine the correct forge
/// and calls its `fetch_pr_info()` method.
///
/// Returns `None` if no forge detected, CLI unavailable, no PR, or fetch error.
pub fn fetch_pr_info(
    worktree_path: &Path,
    branch: &str,
) -> Option<crate::forge::types::PullRequest> {
    let forge_override = load_forge_override();
    let backend = crate::forge::get_forge_backend(worktree_path, forge_override)?;

    backend
        .fetch_pr_info(worktree_path, branch)
        .inspect_err(|e| {
            warn!(
                event = "core.session.pr_info_fetch_failed",
                branch = branch,
                error = %e,
            );
        })
        .ok()
        .flatten()
}

/// Read PR info for a session from the sidecar file.
///
/// Returns `None` if no PR info has been cached yet.
pub fn read_pr_info(session_id: &str) -> Option<crate::forge::types::PullRequest> {
    let config = Config::new();
    persistence::read_pr_info(&config.sessions_dir(), session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_session_not_found() {
        let request = CompleteRequest::new("non-existent");
        let result = complete_session(&request);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionError::NotFound { .. }));
    }

    #[test]
    fn test_complete_request_defaults() {
        let request = CompleteRequest::new("test-branch");
        assert_eq!(request.name, "test-branch");
        assert_eq!(
            request.merge_strategy,
            crate::forge::types::MergeStrategy::Squash
        );
        assert!(!request.no_merge);
        assert!(!request.force);
        assert!(!request.dry_run);
        assert!(!request.skip_ci);
    }
}
