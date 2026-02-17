use std::path::Path;

use git2::Repository;
use tracing::warn;

use crate::git::types::{BaseBranchDrift, DiffStats, GitStats};

use super::{get_diff_stats, get_worktree_status};

/// Collect aggregated git stats for a worktree.
///
/// Returns `None` if the worktree path doesn't exist.
/// Individual stat failures are logged as warnings and degraded to `None`
/// fields rather than failing the entire operation.
///
/// The `base_branch` parameter is used to compute drift (ahead/behind) and
/// diff_vs_base (total committed changes) relative to the base branch.
pub fn collect_git_stats(
    worktree_path: &Path,
    branch: &str,
    base_branch: &str,
) -> Option<GitStats> {
    if !worktree_path.exists() {
        return None;
    }

    let diff = match get_diff_stats(worktree_path) {
        Ok(d) => Some(d),
        Err(e) => {
            warn!(
                event = "core.git.stats.diff_failed",
                branch = branch,
                error = %e
            );
            None
        }
    };

    let status = match get_worktree_status(worktree_path) {
        Ok(s) => Some(s),
        Err(e) => {
            warn!(
                event = "core.git.stats.worktree_status_failed",
                branch = branch,
                error = %e
            );
            None
        }
    };

    // Compute base-branch metrics (drift + diff_vs_base)
    let (drift, diff_vs_base) = compute_base_metrics(worktree_path, branch, base_branch);

    Some(GitStats {
        diff_vs_base,
        drift,
        uncommitted_diff: diff,
        worktree_status: status,
    })
}

/// Compute base-branch-relative metrics for a worktree.
///
/// Returns `(drift, diff_vs_base)` — both `None` if branches cannot be resolved.
pub(super) fn compute_base_metrics(
    worktree_path: &Path,
    branch: &str,
    base_branch: &str,
) -> (Option<BaseBranchDrift>, Option<DiffStats>) {
    let repo = match Repository::open(worktree_path) {
        Ok(r) => r,
        Err(e) => {
            warn!(
                event = "core.git.stats.base_metrics_repo_open_failed",
                branch = branch,
                error = %e
            );
            return (None, None);
        }
    };

    let kild_branch = crate::git::naming::kild_branch_name(branch);
    let branch_oid = match super::super::health::resolve_branch_oid(&repo, &kild_branch) {
        Some(oid) => oid,
        None => {
            warn!(
                event = "core.git.stats.kild_branch_not_found",
                branch = %kild_branch
            );
            return (None, None);
        }
    };
    let base_oid = match super::super::health::resolve_branch_oid(&repo, base_branch) {
        Some(oid) => oid,
        None => {
            warn!(
                event = "core.git.stats.base_branch_not_found",
                base = base_branch,
                "Base branch not found — check git.base_branch config"
            );
            return (None, None);
        }
    };

    let drift = Some(super::super::health::count_base_drift(
        &repo,
        branch_oid,
        base_oid,
        base_branch,
    ));

    let merge_base = super::super::health::find_merge_base(&repo, branch_oid, base_oid);
    let diff_vs_base = match merge_base {
        Some(mb) => super::super::health::diff_against_base(&repo, branch_oid, mb),
        None => None,
    };

    (drift, diff_vs_base)
}
