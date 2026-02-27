use git2::Repository;
use tracing::{debug, warn};

use crate::types::CommitCounts;

/// Count unpushed and behind commits and check if remote tracking branch exists.
pub(super) fn count_unpushed_commits(repo: &Repository) -> CommitCounts {
    // Get current branch reference
    let head = match repo.head() {
        Ok(h) => h,
        Err(e) => {
            warn!(
                event = "core.git.head_read_failed",
                error = %e,
                "Failed to read HEAD - cannot count unpushed commits"
            );
            return CommitCounts::default();
        }
    };

    // Get the branch name
    let branch_name = match head.shorthand() {
        Some(name) => name,
        None => {
            // Detached HEAD is a normal state, not an error
            debug!(
                event = "core.git.detached_head",
                "Repository is in detached HEAD state"
            );
            return CommitCounts::default();
        }
    };

    // Find the local branch
    let local_branch = match repo.find_branch(branch_name, git2::BranchType::Local) {
        Ok(b) => b,
        Err(e) => {
            warn!(
                event = "core.git.local_branch_not_found",
                branch = branch_name,
                error = %e,
                "Could not find local branch"
            );
            return CommitCounts::default();
        }
    };

    // Check if there's an upstream (remote tracking) branch
    let upstream = match local_branch.upstream() {
        Ok(u) => u,
        Err(_) => {
            // No upstream configured - branch has never been pushed
            // This is expected for new branches, not an error
            debug!(
                event = "core.git.no_upstream",
                branch = branch_name,
                "Branch has no upstream - never pushed"
            );
            return CommitCounts::default();
        }
    };

    // Get the OIDs for local and remote
    let local_oid = match head.target() {
        Some(oid) => oid,
        None => {
            warn!(
                event = "core.git.head_target_missing",
                branch = branch_name,
                "HEAD has no target OID"
            );
            return CommitCounts {
                has_remote: true,
                ..Default::default()
            };
        }
    };

    let upstream_oid = match upstream.get().target() {
        Some(oid) => oid,
        None => {
            warn!(
                event = "core.git.upstream_target_missing",
                branch = branch_name,
                "Upstream branch has no target OID"
            );
            return CommitCounts {
                has_remote: true,
                ..Default::default()
            };
        }
    };

    // Count commits ahead (local has, upstream doesn't)
    let mut ahead_walk = match repo.revwalk() {
        Ok(rw) => rw,
        Err(e) => {
            warn!(
                event = "core.git.revwalk_init_failed",
                error = %e,
                "Failed to create revwalk - cannot count unpushed commits"
            );
            return CommitCounts {
                has_remote: true,
                ..Default::default()
            };
        }
    };

    if let Err(e) = ahead_walk.push(local_oid) {
        warn!(
            event = "core.git.revwalk_push_failed",
            error = %e,
            "Failed to push local commit to revwalk"
        );
        return CommitCounts {
            has_remote: true,
            ..Default::default()
        };
    }
    if let Err(e) = ahead_walk.hide(upstream_oid) {
        warn!(
            event = "core.git.revwalk_hide_failed",
            error = %e,
            "Failed to hide upstream commit - history may have diverged"
        );
        return CommitCounts {
            has_remote: true,
            ..Default::default()
        };
    }

    let unpushed_count = ahead_walk.count();

    // Count commits behind (upstream has, local doesn't)
    let mut behind_walk = match repo.revwalk() {
        Ok(rw) => rw,
        Err(e) => {
            warn!(
                event = "core.git.behind_revwalk_init_failed",
                error = %e,
                "Failed to create revwalk for behind count"
            );
            return CommitCounts {
                ahead: unpushed_count,
                has_remote: true,
                behind_count_failed: true,
                ..Default::default()
            };
        }
    };

    if let Err(e) = behind_walk.push(upstream_oid) {
        warn!(
            event = "core.git.behind_revwalk_push_failed",
            error = %e,
            "Failed to push upstream commit to behind revwalk"
        );
        return CommitCounts {
            ahead: unpushed_count,
            has_remote: true,
            behind_count_failed: true,
            ..Default::default()
        };
    }
    if let Err(e) = behind_walk.hide(local_oid) {
        warn!(
            event = "core.git.behind_revwalk_hide_failed",
            error = %e,
            "Failed to hide local commit in behind revwalk"
        );
        return CommitCounts {
            ahead: unpushed_count,
            has_remote: true,
            behind_count_failed: true,
            ..Default::default()
        };
    }

    let behind_count = behind_walk.count();

    CommitCounts {
        ahead: unpushed_count,
        behind: behind_count,
        has_remote: true,
        behind_count_failed: false,
    }
}
