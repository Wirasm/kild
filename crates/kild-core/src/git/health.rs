use std::path::Path;

use git2::{Oid, Repository};
use tracing::{debug, warn};

use crate::git::naming::kild_branch_name;
use crate::git::types::{BaseBranchDrift, BranchHealth, CommitActivity, ConflictStatus, DiffStats};

/// Find the merge base between two commits.
///
/// Returns `None` if no common ancestor exists (e.g., unrelated histories).
pub(super) fn find_merge_base(repo: &Repository, oid_a: Oid, oid_b: Oid) -> Option<Oid> {
    match repo.merge_base(oid_a, oid_b) {
        Ok(oid) => Some(oid),
        Err(e) => {
            debug!(
                event = "core.git.stats.merge_base_not_found",
                error = %e
            );
            None
        }
    }
}

/// Count commits reachable from `branch_oid` but not from `base_oid`.
///
/// Returns 0 on revwalk failure (errors are logged as warnings).
/// Callers cannot distinguish between "no commits" and "check failed".
fn count_commits_since(repo: &Repository, branch_oid: Oid, base_oid: Oid) -> usize {
    let mut walk = match repo.revwalk() {
        Ok(rw) => rw,
        Err(e) => {
            warn!(event = "core.git.stats.revwalk_init_failed", error = %e);
            return 0;
        }
    };
    if let Err(e) = walk.push(branch_oid) {
        warn!(event = "core.git.stats.revwalk_push_failed", error = %e);
        return 0;
    }
    if let Err(e) = walk.hide(base_oid) {
        warn!(event = "core.git.stats.revwalk_hide_failed", error = %e);
        return 0;
    }
    walk.count()
}

/// Get the last commit time on HEAD as RFC3339.
///
/// Returns the commit time converted to UTC. Returns `None` if HEAD
/// cannot be resolved or the timestamp is invalid.
fn get_last_commit_time(repo: &Repository) -> Option<String> {
    let head = match repo.head() {
        Ok(h) => h,
        Err(e) => {
            debug!(event = "core.git.stats.head_read_failed", error = %e);
            return None;
        }
    };
    let commit = match head.peel_to_commit() {
        Ok(c) => c,
        Err(e) => {
            warn!(event = "core.git.stats.commit_peel_failed", error = %e);
            return None;
        }
    };
    let time = commit.time();
    let secs = time.seconds();
    let offset_mins = time.offset_minutes();
    let offset_secs = (offset_mins as i64) * 60;
    let utc_secs = secs + offset_secs;
    match chrono::DateTime::from_timestamp(utc_secs, 0) {
        Some(dt) => Some(dt.to_rfc3339()),
        None => {
            warn!(event = "core.git.stats.timestamp_invalid", secs = utc_secs);
            None
        }
    }
}

/// Compute diff stats between merge base tree and branch tip tree.
///
/// Shows the total changes introduced by the branch (how big the PR will be).
/// Returns `None` if commits cannot be resolved or diff computation fails (logged as warnings).
fn diff_against_base(repo: &Repository, branch_oid: Oid, merge_base_oid: Oid) -> Option<DiffStats> {
    let base_commit = match repo.find_commit(merge_base_oid) {
        Ok(c) => c,
        Err(e) => {
            warn!(event = "core.git.stats.base_commit_not_found", error = %e);
            return None;
        }
    };
    let branch_commit = match repo.find_commit(branch_oid) {
        Ok(c) => c,
        Err(e) => {
            warn!(event = "core.git.stats.branch_commit_not_found", error = %e);
            return None;
        }
    };
    let base_tree = match base_commit.tree() {
        Ok(t) => t,
        Err(e) => {
            warn!(event = "core.git.stats.base_tree_failed", error = %e);
            return None;
        }
    };
    let branch_tree = match branch_commit.tree() {
        Ok(t) => t,
        Err(e) => {
            warn!(event = "core.git.stats.branch_tree_failed", error = %e);
            return None;
        }
    };
    let diff = match repo.diff_tree_to_tree(Some(&base_tree), Some(&branch_tree), None) {
        Ok(d) => d,
        Err(e) => {
            warn!(event = "core.git.stats.diff_computation_failed", error = %e);
            return None;
        }
    };
    let stats = match diff.stats() {
        Ok(s) => s,
        Err(e) => {
            warn!(event = "core.git.stats.diff_stats_failed", error = %e);
            return None;
        }
    };
    Some(DiffStats {
        insertions: stats.insertions(),
        deletions: stats.deletions(),
        files_changed: stats.files_changed(),
    })
}

/// Check for merge conflicts between branch tip and base tip (in-memory).
///
/// Performs an in-memory merge without modifying the working tree.
/// Returns `Unknown` if the merge cannot be performed (logged as warnings).
fn check_conflicts(repo: &Repository, branch_oid: Oid, base_oid: Oid) -> ConflictStatus {
    let branch_commit = match repo.find_commit(branch_oid) {
        Ok(c) => c,
        Err(e) => {
            warn!(event = "core.git.stats.conflict_check_branch_not_found", error = %e);
            return ConflictStatus::Unknown;
        }
    };
    let base_commit = match repo.find_commit(base_oid) {
        Ok(c) => c,
        Err(e) => {
            warn!(event = "core.git.stats.conflict_check_base_not_found", error = %e);
            return ConflictStatus::Unknown;
        }
    };

    let index = match repo.merge_commits(&branch_commit, &base_commit, None) {
        Ok(idx) => idx,
        Err(e) => {
            warn!(event = "core.git.stats.merge_check_failed", error = %e);
            return ConflictStatus::Unknown;
        }
    };

    if index.has_conflicts() {
        ConflictStatus::Conflicts
    } else {
        ConflictStatus::Clean
    }
}

/// Count commits ahead and behind between branch tip and base branch tip.
fn count_base_drift(
    repo: &Repository,
    branch_oid: Oid,
    base_oid: Oid,
    base_branch: &str,
) -> BaseBranchDrift {
    let ahead = count_commits_since(repo, branch_oid, base_oid);
    let behind = count_commits_since(repo, base_oid, branch_oid);
    BaseBranchDrift {
        ahead,
        behind,
        base_branch: base_branch.to_string(),
    }
}

/// Resolve a branch name to its OID, trying local first then remote.
pub(super) fn resolve_branch_oid(repo: &Repository, branch_name: &str) -> Option<Oid> {
    // Try local branch first
    match repo.find_branch(branch_name, git2::BranchType::Local) {
        Ok(branch) => {
            if let Some(oid) = branch.get().target() {
                return Some(oid);
            }
            warn!(
                event = "core.git.stats.branch_no_target",
                branch = branch_name
            );
            return None;
        }
        Err(_) => {
            debug!(
                event = "core.git.stats.branch_not_found_local",
                branch = branch_name
            );
        }
    }
    // Try remote tracking branch
    let remote_ref = format!("origin/{}", branch_name);
    match repo.find_branch(&remote_ref, git2::BranchType::Remote) {
        Ok(branch) => {
            if let Some(oid) = branch.get().target() {
                return Some(oid);
            }
            warn!(
                event = "core.git.stats.remote_branch_no_target",
                branch = branch_name
            );
            return None;
        }
        Err(_) => {
            debug!(
                event = "core.git.stats.branch_not_found_remote",
                branch = branch_name
            );
        }
    }
    None
}

/// Check if repository has any remote configured.
fn repo_has_remote(repo: &Repository) -> bool {
    repo.remotes().is_ok_and(|remotes| !remotes.is_empty())
}

/// Collect comprehensive branch health metrics for a kild.
///
/// Returns pure git metrics only. Merge readiness (which depends on
/// forge/PR data) is computed separately by the caller.
///
/// - `branch`: User branch name (without `kild/` prefix).
/// - `base_branch`: Base branch for drift comparison (e.g., "main").
/// - `created_at`: Session creation timestamp (RFC3339), passed through to result.
///
/// Returns `Err` if the worktree cannot be opened or branch refs cannot be resolved.
pub fn collect_branch_health(
    worktree_path: &Path,
    branch: &str,
    base_branch: &str,
    created_at: &str,
) -> Result<BranchHealth, String> {
    let repo = match Repository::open(worktree_path) {
        Ok(r) => r,
        Err(e) => {
            warn!(event = "core.git.stats.repo_open_failed", branch = branch, error = %e);
            return Err(format!("Failed to open repository: {}", e));
        }
    };

    let has_remote = repo_has_remote(&repo);

    // Resolve kild branch OID
    let kild_branch = kild_branch_name(branch);
    let branch_oid = match resolve_branch_oid(&repo, &kild_branch) {
        Some(oid) => oid,
        None => {
            warn!(
                event = "core.git.stats.branch_not_found",
                branch = kild_branch
            );
            return Err(format!("Branch '{}' not found in repository", kild_branch));
        }
    };

    // Resolve base branch OID
    let base_oid = match resolve_branch_oid(&repo, base_branch) {
        Some(oid) => oid,
        None => {
            warn!(
                event = "core.git.stats.base_branch_not_found",
                base = base_branch
            );
            return Err(format!(
                "Base branch '{}' not found. Check your git.base_branch config.",
                base_branch
            ));
        }
    };

    // Find merge base
    let merge_base = find_merge_base(&repo, branch_oid, base_oid);

    // Commit activity
    let commits_since_base = merge_base.map_or(0, |mb| count_commits_since(&repo, branch_oid, mb));
    let last_commit_time = get_last_commit_time(&repo);

    // Diff vs base
    let diff_vs_base = merge_base.and_then(|mb| diff_against_base(&repo, branch_oid, mb));

    // Conflict detection (against base tip, not merge base)
    let conflict_status = check_conflicts(&repo, branch_oid, base_oid);

    // Base branch drift
    let drift = count_base_drift(&repo, branch_oid, base_oid, base_branch);

    Ok(BranchHealth {
        branch: branch.to_string(),
        created_at: created_at.to_string(),
        commit_activity: CommitActivity {
            commits_since_base,
            last_commit_time,
        },
        drift,
        diff_vs_base,
        conflict_status,
        has_remote,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::types::{CiStatus, PrInfo};
    use crate::git::types::{MergeReadiness, WorktreeStatus};
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_git_repo(dir: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .expect("Failed to init git repo");
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .expect("Failed to set git email");
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .expect("Failed to set git name");
    }

    /// Helper: git add all + commit with message
    fn git_add_commit(dir: &Path, msg: &str) {
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", msg])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    #[test]
    fn test_collect_branch_health_nonexistent_path() {
        let result = collect_branch_health(
            Path::new("/nonexistent/path"),
            "test",
            "main",
            "2026-02-09T10:00:00Z",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_collect_branch_health_with_kild_branch() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Create initial commit on main
        fs::write(dir.path().join("file.txt"), "initial").unwrap();
        git_add_commit(dir.path(), "initial on main");

        // Rename default branch to main (in case it's 'master')
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Create kild branch with a commit
        Command::new("git")
            .args(["checkout", "-b", "kild/test-feature"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("feature.txt"), "feature code").unwrap();
        git_add_commit(dir.path(), "feature commit");

        let health =
            collect_branch_health(dir.path(), "test-feature", "main", "2026-02-09T10:00:00Z");
        assert!(health.is_ok());
        let health = health.unwrap();
        assert_eq!(health.branch, "test-feature");
        assert_eq!(health.commit_activity.commits_since_base, 1);
        assert!(!health.has_remote);
        assert_eq!(health.drift.ahead, 1);
        assert_eq!(health.drift.behind, 0);
        assert_eq!(health.conflict_status, ConflictStatus::Clean);
        assert!(health.diff_vs_base.is_some());
    }

    #[test]
    fn test_collect_branch_health_behind_base() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Create initial commit on main
        fs::write(dir.path().join("file.txt"), "initial").unwrap();
        git_add_commit(dir.path(), "initial on main");

        // Rename default branch to main
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Create kild branch
        Command::new("git")
            .args(["checkout", "-b", "kild/test-behind"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("feature.txt"), "feature").unwrap();
        git_add_commit(dir.path(), "feature commit");

        // Go back to main and add commits
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("main-update.txt"), "update 1").unwrap();
        git_add_commit(dir.path(), "main update 1");
        fs::write(dir.path().join("main-update2.txt"), "update 2").unwrap();
        git_add_commit(dir.path(), "main update 2");

        // Go back to kild branch to set HEAD
        Command::new("git")
            .args(["checkout", "kild/test-behind"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let health =
            collect_branch_health(dir.path(), "test-behind", "main", "2026-02-09T10:00:00Z");
        assert!(health.is_ok());
        let health = health.unwrap();
        assert_eq!(health.drift.ahead, 1);
        assert_eq!(health.drift.behind, 2);
    }

    #[test]
    fn test_collect_branch_health_with_conflicts() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Create initial commit on main
        fs::write(dir.path().join("shared.txt"), "original content").unwrap();
        git_add_commit(dir.path(), "initial on main");

        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Create kild branch that modifies the same file
        Command::new("git")
            .args(["checkout", "-b", "kild/test-conflicts"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("shared.txt"), "branch version of content").unwrap();
        git_add_commit(dir.path(), "branch change to shared file");

        // Go back to main and modify the same file differently
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("shared.txt"), "main version of content").unwrap();
        git_add_commit(dir.path(), "main change to shared file");

        // Switch back to kild branch for HEAD
        Command::new("git")
            .args(["checkout", "kild/test-conflicts"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let health =
            collect_branch_health(dir.path(), "test-conflicts", "main", "2026-02-09T10:00:00Z");
        assert!(health.is_ok());
        let health = health.unwrap();
        assert_eq!(
            health.conflict_status,
            ConflictStatus::Conflicts,
            "Branches modifying the same file should detect conflicts"
        );
    }

    #[test]
    fn test_collect_branch_health_invalid_base_branch() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        fs::write(dir.path().join("file.txt"), "initial").unwrap();
        git_add_commit(dir.path(), "initial on main");

        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["checkout", "-b", "kild/test"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("feature.txt"), "feature").unwrap();
        git_add_commit(dir.path(), "feature commit");

        let result = collect_branch_health(
            dir.path(),
            "test",
            "nonexistent-base",
            "2026-02-09T10:00:00Z",
        );
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("nonexistent-base"),
            "Error should mention the missing base branch: {}",
            msg
        );
    }

    // -- MergeReadiness tests --

    fn make_health(
        conflict_status: ConflictStatus,
        behind: usize,
        has_remote: bool,
    ) -> BranchHealth {
        BranchHealth {
            branch: "test".to_string(),
            created_at: "2026-02-09T10:00:00Z".to_string(),
            commit_activity: CommitActivity {
                commits_since_base: 3,
                last_commit_time: None,
            },
            drift: BaseBranchDrift {
                ahead: 3,
                behind,
                base_branch: "main".to_string(),
            },
            diff_vs_base: Some(DiffStats {
                insertions: 10,
                deletions: 2,
                files_changed: 1,
            }),
            conflict_status,
            has_remote,
        }
    }

    #[test]
    fn test_readiness_has_conflicts() {
        let h = make_health(ConflictStatus::Conflicts, 0, true);
        assert_eq!(
            MergeReadiness::compute(&h, &None, None),
            MergeReadiness::HasConflicts
        );
    }

    #[test]
    fn test_readiness_conflict_check_failed() {
        let h = make_health(ConflictStatus::Unknown, 0, true);
        assert_eq!(
            MergeReadiness::compute(&h, &None, None),
            MergeReadiness::ConflictCheckFailed
        );
    }

    #[test]
    fn test_readiness_needs_rebase() {
        let h = make_health(ConflictStatus::Clean, 5, true);
        assert_eq!(
            MergeReadiness::compute(&h, &None, None),
            MergeReadiness::NeedsRebase
        );
    }

    #[test]
    fn test_readiness_ready_local() {
        let h = make_health(ConflictStatus::Clean, 0, false);
        assert_eq!(
            MergeReadiness::compute(&h, &None, None),
            MergeReadiness::ReadyLocal
        );
    }

    #[test]
    fn test_readiness_needs_push() {
        let h = make_health(ConflictStatus::Clean, 0, true);
        let ws = WorktreeStatus {
            unpushed_commit_count: 3,
            has_remote_branch: true,
            ..Default::default()
        };
        assert_eq!(
            MergeReadiness::compute(&h, &Some(ws), None),
            MergeReadiness::NeedsPush
        );
    }

    #[test]
    fn test_readiness_needs_push_never_pushed() {
        let h = make_health(ConflictStatus::Clean, 0, true);
        let ws = WorktreeStatus {
            unpushed_commit_count: 0,
            has_remote_branch: false,
            ..Default::default()
        };
        assert_eq!(
            MergeReadiness::compute(&h, &Some(ws), None),
            MergeReadiness::NeedsPush
        );
    }

    #[test]
    fn test_readiness_needs_pr() {
        let h = make_health(ConflictStatus::Clean, 0, true);
        let ws = WorktreeStatus {
            unpushed_commit_count: 0,
            has_remote_branch: true,
            ..Default::default()
        };
        assert_eq!(
            MergeReadiness::compute(&h, &Some(ws), None),
            MergeReadiness::NeedsPr
        );
    }

    #[test]
    fn test_readiness_ci_failing() {
        use crate::forge::types::{PrState, ReviewStatus};
        let h = make_health(ConflictStatus::Clean, 0, true);
        let ws = WorktreeStatus {
            unpushed_commit_count: 0,
            has_remote_branch: true,
            ..Default::default()
        };
        let pr = PrInfo {
            number: 1,
            url: "https://example.com/pull/1".to_string(),
            state: PrState::Open,
            ci_status: CiStatus::Failing,
            ci_summary: None,
            review_status: ReviewStatus::Unknown,
            review_summary: None,
            updated_at: "2026-02-09T12:00:00Z".to_string(),
        };
        assert_eq!(
            MergeReadiness::compute(&h, &Some(ws), Some(&pr)),
            MergeReadiness::CiFailing
        );
    }

    #[test]
    fn test_readiness_ready() {
        use crate::forge::types::{PrState, ReviewStatus};
        let h = make_health(ConflictStatus::Clean, 0, true);
        let ws = WorktreeStatus {
            unpushed_commit_count: 0,
            has_remote_branch: true,
            ..Default::default()
        };
        let pr = PrInfo {
            number: 1,
            url: "https://example.com/pull/1".to_string(),
            state: PrState::Open,
            ci_status: CiStatus::Passing,
            ci_summary: None,
            review_status: ReviewStatus::Approved,
            review_summary: None,
            updated_at: "2026-02-09T12:00:00Z".to_string(),
        };
        assert_eq!(
            MergeReadiness::compute(&h, &Some(ws), Some(&pr)),
            MergeReadiness::Ready
        );
    }

    #[test]
    fn test_readiness_display() {
        assert_eq!(MergeReadiness::Ready.to_string(), "Ready");
        assert_eq!(MergeReadiness::NeedsPush.to_string(), "Needs push");
        assert_eq!(MergeReadiness::NeedsRebase.to_string(), "Needs rebase");
        assert_eq!(MergeReadiness::HasConflicts.to_string(), "Has conflicts");
        assert_eq!(
            MergeReadiness::ConflictCheckFailed.to_string(),
            "Conflict check failed"
        );
        assert_eq!(MergeReadiness::NeedsPr.to_string(), "Needs PR");
        assert_eq!(MergeReadiness::CiFailing.to_string(), "CI failing");
        assert_eq!(MergeReadiness::ReadyLocal.to_string(), "Ready (local)");
    }

    #[test]
    fn test_readiness_serde() {
        let json = serde_json::to_string(&MergeReadiness::NeedsRebase).unwrap();
        assert_eq!(json, "\"needs_rebase\"");

        let json = serde_json::to_string(&MergeReadiness::HasConflicts).unwrap();
        assert_eq!(json, "\"has_conflicts\"");

        let json = serde_json::to_string(&MergeReadiness::ConflictCheckFailed).unwrap();
        assert_eq!(json, "\"conflict_check_failed\"");

        let json = serde_json::to_string(&MergeReadiness::ReadyLocal).unwrap();
        assert_eq!(json, "\"ready_local\"");
    }

    #[test]
    fn test_readiness_ready_with_pending_ci() {
        use crate::forge::types::{PrState, ReviewStatus};
        let h = make_health(ConflictStatus::Clean, 0, true);
        let ws = WorktreeStatus {
            unpushed_commit_count: 0,
            has_remote_branch: true,
            ..Default::default()
        };
        let pr = PrInfo {
            number: 1,
            url: "https://example.com/pull/1".to_string(),
            state: PrState::Open,
            ci_status: CiStatus::Pending,
            ci_summary: None,
            review_status: ReviewStatus::Unknown,
            review_summary: None,
            updated_at: "2026-02-09T12:00:00Z".to_string(),
        };
        assert_eq!(
            MergeReadiness::compute(&h, &Some(ws), Some(&pr)),
            MergeReadiness::Ready
        );
    }

    #[test]
    fn test_readiness_ready_with_unknown_ci() {
        use crate::forge::types::{PrState, ReviewStatus};
        let h = make_health(ConflictStatus::Clean, 0, true);
        let ws = WorktreeStatus {
            unpushed_commit_count: 0,
            has_remote_branch: true,
            ..Default::default()
        };
        let pr = PrInfo {
            number: 1,
            url: "https://example.com/pull/1".to_string(),
            state: PrState::Open,
            ci_status: CiStatus::Unknown,
            ci_summary: None,
            review_status: ReviewStatus::Unknown,
            review_summary: None,
            updated_at: "2026-02-09T12:00:00Z".to_string(),
        };
        assert_eq!(
            MergeReadiness::compute(&h, &Some(ws), Some(&pr)),
            MergeReadiness::Ready
        );
    }

    #[test]
    fn test_readiness_ready_with_draft_pr() {
        use crate::forge::types::{PrState, ReviewStatus};
        let h = make_health(ConflictStatus::Clean, 0, true);
        let ws = WorktreeStatus {
            unpushed_commit_count: 0,
            has_remote_branch: true,
            ..Default::default()
        };
        let pr = PrInfo {
            number: 1,
            url: "https://example.com/pull/1".to_string(),
            state: PrState::Draft,
            ci_status: CiStatus::Passing,
            ci_summary: None,
            review_status: ReviewStatus::Unknown,
            review_summary: None,
            updated_at: "2026-02-09T12:00:00Z".to_string(),
        };
        assert_eq!(
            MergeReadiness::compute(&h, &Some(ws), Some(&pr)),
            MergeReadiness::Ready
        );
    }
}
