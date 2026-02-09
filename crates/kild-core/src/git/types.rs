use serde::Serialize;
use std::fmt;
use std::path::PathBuf;

/// Git diff statistics for a worktree.
///
/// Represents the number of lines added, removed, and files changed
/// between the index (staging area) and the working directory.
/// This captures **unstaged changes only**, not staged changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct DiffStats {
    /// Number of lines added
    pub insertions: usize,
    /// Number of lines removed
    pub deletions: usize,
    /// Number of files changed
    pub files_changed: usize,
}

impl DiffStats {
    /// Returns true if there are any line changes.
    pub fn has_changes(&self) -> bool {
        self.insertions > 0 || self.deletions > 0
    }
}

/// Result of counting commits ahead/behind remote tracking branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommitCounts {
    /// Number of commits ahead of remote (unpushed)
    pub ahead: usize,
    /// Number of commits behind remote
    pub behind: usize,
    /// Whether a remote tracking branch exists
    pub has_remote: bool,
    /// Whether the behind count check failed (behind value is unreliable)
    pub behind_count_failed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub project_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectInfo {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub remote_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub exists: bool,
    pub is_current: bool,
}

impl WorktreeInfo {
    pub fn new(path: PathBuf, branch: String, project_id: String) -> Self {
        Self {
            path,
            branch,
            project_id,
        }
    }
}

/// Comprehensive worktree status for destroy safety checks.
///
/// Contains information about uncommitted changes, unpushed commits,
/// and remote branch existence to help users make informed decisions
/// before destroying a kild.
///
/// # Degraded State
///
/// If `status_check_failed` is true, the status information may be incomplete
/// or inaccurate. In this case, the fallback behavior is conservative:
/// - `has_uncommitted_changes` is set to true (assume dirty)
/// - Users should be warned that the check failed
#[derive(Debug, Clone, Default, Serialize)]
pub struct WorktreeStatus {
    /// Whether there are uncommitted changes (staged, modified, or untracked).
    ///
    /// When `status_check_failed` is true, this defaults to true (conservative).
    pub has_uncommitted_changes: bool,
    /// Number of commits ahead of the remote tracking branch.
    ///
    /// Zero when:
    /// - Branch tracks a remote and is up-to-date
    /// - Branch has no remote tracking branch (check `has_remote_branch`)
    /// - Status check failed (check `status_check_failed`)
    ///
    /// Note: When `has_remote_branch` is false, this is always 0 because
    /// there's no baseline to compare against. Use the "never pushed" warning
    /// via `has_remote_branch` instead.
    pub unpushed_commit_count: usize,
    /// Number of commits behind the remote tracking branch.
    ///
    /// Zero when:
    /// - Branch tracks a remote and is up-to-date
    /// - Branch has no remote tracking branch (check `has_remote_branch`)
    /// - Status check failed (check `status_check_failed`)
    pub behind_commit_count: usize,
    /// Whether a remote tracking branch exists for this branch.
    /// False means the branch has never been pushed.
    pub has_remote_branch: bool,
    /// Details about uncommitted changes (file counts by category).
    /// None when no uncommitted changes exist or when status check failed.
    pub uncommitted_details: Option<UncommittedDetails>,
    /// Whether the behind-count check failed and `behind_commit_count` is unreliable.
    ///
    /// When true, `behind_commit_count` is 0 but this does NOT mean the branch is
    /// up-to-date — we simply couldn't determine the actual count. The CLI should
    /// surface this uncertainty to the user.
    pub behind_count_failed: bool,
    /// Whether the status check encountered errors and returned degraded results.
    ///
    /// When true, the status information may be incomplete. The fallback behavior
    /// is conservative (assumes uncommitted changes exist) to prevent data loss.
    pub status_check_failed: bool,
}

/// Detailed breakdown of uncommitted changes.
#[derive(Debug, Clone, Default, Serialize)]
pub struct UncommittedDetails {
    /// Number of files staged for commit.
    pub staged_files: usize,
    /// Number of tracked files with unstaged modifications.
    pub modified_files: usize,
    /// Number of untracked files.
    pub untracked_files: usize,
}

impl UncommittedDetails {
    /// Returns true if there are no uncommitted changes.
    pub fn is_empty(&self) -> bool {
        self.staged_files == 0 && self.modified_files == 0 && self.untracked_files == 0
    }

    /// Returns the total number of files with uncommitted changes.
    pub fn total(&self) -> usize {
        self.staged_files + self.modified_files + self.untracked_files
    }
}

/// Aggregated git statistics for a worktree.
///
/// Combines diff stats and worktree status into a single response.
/// Both fields are optional to support graceful degradation when
/// individual git operations fail.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GitStats {
    pub diff_stats: Option<DiffStats>,
    pub worktree_status: Option<WorktreeStatus>,
}

impl GitStats {
    /// Returns true if any git data was successfully collected.
    pub fn has_data(&self) -> bool {
        self.diff_stats.is_some() || self.worktree_status.is_some()
    }

    /// Returns true if all git operations failed.
    pub fn is_empty(&self) -> bool {
        self.diff_stats.is_none() && self.worktree_status.is_none()
    }
}

/// Computed merge readiness status for a branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeReadiness {
    /// Clean, pushed, PR open, CI passing
    Ready,
    /// Has unpushed commits
    NeedsPush,
    /// Behind base branch significantly
    NeedsRebase,
    /// Cannot merge cleanly into base
    HasConflicts,
    /// Pushed but no PR exists
    NeedsPr,
    /// PR exists but CI is failing
    CiFailing,
    /// Ready to merge locally (no remote configured)
    ReadyLocal,
}

impl fmt::Display for MergeReadiness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MergeReadiness::Ready => write!(f, "Ready"),
            MergeReadiness::NeedsPush => write!(f, "Needs push"),
            MergeReadiness::NeedsRebase => write!(f, "Needs rebase"),
            MergeReadiness::HasConflicts => write!(f, "Has conflicts"),
            MergeReadiness::NeedsPr => write!(f, "Needs PR"),
            MergeReadiness::CiFailing => write!(f, "CI failing"),
            MergeReadiness::ReadyLocal => write!(f, "Ready (local)"),
        }
    }
}

/// Commit activity metrics for a branch.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct CommitActivity {
    /// Total commits on branch since diverging from base.
    pub commits_since_base: usize,
    /// Timestamp of the last commit (RFC3339). None if no commits.
    pub last_commit_time: Option<String>,
}

/// Base branch drift metrics.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct BaseBranchDrift {
    /// Commits ahead of base branch (on kild branch, not on base).
    pub ahead: usize,
    /// Commits base branch has gained since kild branched off.
    pub behind: usize,
    /// Name of the base branch used for comparison.
    pub base_branch: String,
}

/// Comprehensive branch health for a kild.
#[derive(Debug, Clone, Serialize)]
pub struct BranchHealth {
    pub branch: String,
    pub created_at: String,
    pub commit_activity: CommitActivity,
    pub drift: BaseBranchDrift,
    /// Total diff from merge base to branch tip (how big the PR will be).
    pub diff_vs_base: Option<DiffStats>,
    /// Whether the branch can merge cleanly into base.
    pub has_conflicts: bool,
    /// Whether conflict detection failed (result unreliable).
    pub conflict_check_failed: bool,
    /// Computed readiness status.
    pub merge_readiness: MergeReadiness,
    /// Whether a remote is configured for this repository.
    pub has_remote: bool,
}

impl ProjectInfo {
    pub fn new(id: String, name: String, path: PathBuf, remote_url: Option<String>) -> Self {
        Self {
            id,
            name,
            path,
            remote_url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worktree_info() {
        let worktree = WorktreeInfo::new(
            PathBuf::from("/tmp/test"),
            "feature-branch".to_string(),
            "test-project".to_string(),
        );

        assert_eq!(worktree.branch, "feature-branch");
        assert_eq!(worktree.project_id, "test-project");
        assert_eq!(worktree.path, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn test_worktree_info_preserves_original_branch_name() {
        // WorktreeInfo stores the original branch name (with slashes),
        // not the sanitized version used for the worktree path/directory.
        // This ensures git operations use the correct branch name.
        let original_branch = "feature/auth";
        let sanitized_path = PathBuf::from("/tmp/worktrees/project/feature-auth");

        let info = WorktreeInfo::new(
            sanitized_path,
            original_branch.to_string(),
            "test-project".to_string(),
        );

        // Original branch name with slash is preserved
        assert_eq!(info.branch, "feature/auth");
        assert_ne!(info.branch, "feature-auth");
    }

    #[test]
    fn test_project_info() {
        let project = ProjectInfo::new(
            "test-id".to_string(),
            "test-project".to_string(),
            PathBuf::from("/path/to/project"),
            Some("https://github.com/user/repo.git".to_string()),
        );

        assert_eq!(project.id, "test-id");
        assert_eq!(project.name, "test-project");
        assert_eq!(
            project.remote_url,
            Some("https://github.com/user/repo.git".to_string())
        );
    }

    #[test]
    fn test_branch_info() {
        let branch = BranchInfo {
            name: "main".to_string(),
            exists: true,
            is_current: true,
        };

        assert_eq!(branch.name, "main");
        assert!(branch.exists);
        assert!(branch.is_current);
    }

    // --- UncommittedDetails tests ---

    #[test]
    fn test_uncommitted_details_is_empty() {
        let empty = UncommittedDetails::default();
        assert!(empty.is_empty());

        let with_staged = UncommittedDetails {
            staged_files: 1,
            ..Default::default()
        };
        assert!(!with_staged.is_empty());

        let with_modified = UncommittedDetails {
            modified_files: 1,
            ..Default::default()
        };
        assert!(!with_modified.is_empty());

        let with_untracked = UncommittedDetails {
            untracked_files: 1,
            ..Default::default()
        };
        assert!(!with_untracked.is_empty());
    }

    #[test]
    fn test_uncommitted_details_total() {
        let empty = UncommittedDetails::default();
        assert_eq!(empty.total(), 0);

        let details = UncommittedDetails {
            staged_files: 2,
            modified_files: 3,
            untracked_files: 5,
        };
        assert_eq!(details.total(), 10);
    }

    // --- WorktreeStatus tests ---

    #[test]
    fn test_worktree_status_default() {
        let status = WorktreeStatus::default();
        assert!(!status.has_uncommitted_changes);
        assert_eq!(status.unpushed_commit_count, 0);
        assert_eq!(status.behind_commit_count, 0);
        assert!(!status.has_remote_branch);
        assert!(status.uncommitted_details.is_none());
        assert!(!status.behind_count_failed);
        assert!(!status.status_check_failed);
    }

    #[test]
    fn test_worktree_status_with_degraded_state() {
        let status = WorktreeStatus {
            has_uncommitted_changes: true,
            status_check_failed: true,
            ..Default::default()
        };
        assert!(status.has_uncommitted_changes);
        assert!(status.status_check_failed);
    }

    // --- Serialization tests ---

    #[test]
    fn test_diff_stats_serializes_to_json() {
        let stats = DiffStats {
            insertions: 42,
            deletions: 10,
            files_changed: 5,
        };
        let value = serde_json::to_value(&stats).expect("DiffStats should serialize");
        assert_eq!(value["insertions"], 42);
        assert_eq!(value["deletions"], 10);
        assert_eq!(value["files_changed"], 5);
    }

    #[test]
    fn test_worktree_status_serializes_to_json() {
        let status = WorktreeStatus {
            has_uncommitted_changes: true,
            unpushed_commit_count: 4,
            behind_commit_count: 2,
            has_remote_branch: true,
            uncommitted_details: Some(UncommittedDetails {
                staged_files: 3,
                modified_files: 2,
                untracked_files: 1,
            }),
            behind_count_failed: false,
            status_check_failed: false,
        };
        let value = serde_json::to_value(&status).expect("WorktreeStatus should serialize");
        assert_eq!(value["has_uncommitted_changes"], true);
        assert_eq!(value["unpushed_commit_count"], 4);
        assert_eq!(value["behind_commit_count"], 2);
        assert_eq!(value["has_remote_branch"], true);
        assert_eq!(value["behind_count_failed"], false);
        assert_eq!(value["status_check_failed"], false);

        let details = &value["uncommitted_details"];
        assert_eq!(details["staged_files"], 3);
        assert_eq!(details["modified_files"], 2);
        assert_eq!(details["untracked_files"], 1);
    }

    // --- MergeReadiness tests ---

    #[test]
    fn test_merge_readiness_display() {
        assert_eq!(MergeReadiness::Ready.to_string(), "Ready");
        assert_eq!(MergeReadiness::NeedsPush.to_string(), "Needs push");
        assert_eq!(MergeReadiness::NeedsRebase.to_string(), "Needs rebase");
        assert_eq!(MergeReadiness::HasConflicts.to_string(), "Has conflicts");
        assert_eq!(MergeReadiness::NeedsPr.to_string(), "Needs PR");
        assert_eq!(MergeReadiness::CiFailing.to_string(), "CI failing");
        assert_eq!(MergeReadiness::ReadyLocal.to_string(), "Ready (local)");
    }

    #[test]
    fn test_merge_readiness_serde() {
        let json = serde_json::to_string(&MergeReadiness::NeedsRebase).unwrap();
        assert_eq!(json, "\"needs_rebase\"");

        let json = serde_json::to_string(&MergeReadiness::HasConflicts).unwrap();
        assert_eq!(json, "\"has_conflicts\"");

        let json = serde_json::to_string(&MergeReadiness::ReadyLocal).unwrap();
        assert_eq!(json, "\"ready_local\"");
    }

    #[test]
    fn test_commit_activity_default() {
        let activity = CommitActivity::default();
        assert_eq!(activity.commits_since_base, 0);
        assert!(activity.last_commit_time.is_none());
    }

    #[test]
    fn test_base_branch_drift_default() {
        let drift = BaseBranchDrift::default();
        assert_eq!(drift.ahead, 0);
        assert_eq!(drift.behind, 0);
        assert_eq!(drift.base_branch, "");
    }

    #[test]
    fn test_branch_health_serializes_to_json() {
        let health = BranchHealth {
            branch: "feature-auth".to_string(),
            created_at: "2026-02-09T10:00:00Z".to_string(),
            commit_activity: CommitActivity {
                commits_since_base: 4,
                last_commit_time: Some("2026-02-09T11:48:00Z".to_string()),
            },
            drift: BaseBranchDrift {
                ahead: 4,
                behind: 12,
                base_branch: "main".to_string(),
            },
            diff_vs_base: Some(DiffStats {
                insertions: 450,
                deletions: 30,
                files_changed: 12,
            }),
            has_conflicts: false,
            conflict_check_failed: false,
            merge_readiness: MergeReadiness::NeedsRebase,
            has_remote: true,
        };
        let value = serde_json::to_value(&health).expect("BranchHealth should serialize");
        assert_eq!(value["branch"], "feature-auth");
        assert_eq!(value["commit_activity"]["commits_since_base"], 4);
        assert_eq!(value["drift"]["behind"], 12);
        assert_eq!(value["diff_vs_base"]["insertions"], 450);
        assert_eq!(value["has_conflicts"], false);
        assert_eq!(value["merge_readiness"], "needs_rebase");
        assert_eq!(value["has_remote"], true);
    }

    #[test]
    fn test_uncommitted_details_serializes_to_json() {
        let details = UncommittedDetails {
            staged_files: 1,
            modified_files: 2,
            untracked_files: 3,
        };
        let value = serde_json::to_value(&details).expect("UncommittedDetails should serialize");
        assert_eq!(value["staged_files"], 1);
        assert_eq!(value["modified_files"], 2);
        assert_eq!(value["untracked_files"], 3);
    }
}
