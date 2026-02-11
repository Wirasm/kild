use crate::forge::types::{CiStatus, PrInfo};
use serde::Serialize;
use std::path::PathBuf;

/// Git diff statistics.
///
/// Generic container for insertions, deletions, and files changed.
/// Context-dependent meaning:
/// - In `GitStats.uncommitted_diff`: unstaged changes (index vs working directory).
/// - In `GitStats.diff_vs_base` / `BranchHealth.diff_vs_base`: total branch changes (merge base vs branch tip).
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
/// Combines diff stats, worktree status, and base-branch metrics into a single response.
/// All fields are optional to support graceful degradation when individual git operations fail.
///
/// # Field Dependencies
/// - `drift` and `diff_vs_base` are computed together but may fail independently.
///   Both are `None` when branch OIDs cannot be resolved.
///   `drift` may be `Some` while `diff_vs_base` is `None` (merge base not found).
/// - `uncommitted_diff` and `worktree_status` are computed independently from base-branch metrics.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GitStats {
    /// Total committed changes from merge base to branch tip (how big the PR will be).
    /// `None` if merge base cannot be found or diff computation fails.
    /// May be `None` even when `drift` is `Some` (merge base not found).
    pub diff_vs_base: Option<DiffStats>,
    /// Branch position relative to base branch.
    /// `None` if branch or base branch OIDs cannot be resolved.
    /// May be `Some` even when `diff_vs_base` is `None` (drift doesn't require merge base).
    pub drift: Option<BaseBranchDrift>,
    /// Uncommitted changes (index vs working directory). Renamed from `diff_stats`.
    pub uncommitted_diff: Option<DiffStats>,
    pub worktree_status: Option<WorktreeStatus>,
}

impl GitStats {
    /// Returns true if any git data was successfully collected.
    pub fn has_data(&self) -> bool {
        self.diff_vs_base.is_some()
            || self.drift.is_some()
            || self.uncommitted_diff.is_some()
            || self.worktree_status.is_some()
    }

    /// Returns true if all git operations failed.
    pub fn is_empty(&self) -> bool {
        self.diff_vs_base.is_none()
            && self.drift.is_none()
            && self.uncommitted_diff.is_none()
            && self.worktree_status.is_none()
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BaseBranchDrift {
    /// Commits ahead of base branch (on kild branch, not on base).
    pub ahead: usize,
    /// Commits base branch has gained since kild branched off.
    pub behind: usize,
    /// Name of the base branch used for comparison.
    pub base_branch: String,
}

/// Result of in-memory merge conflict detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStatus {
    /// No conflicts detected — branch can merge cleanly.
    Clean,
    /// Conflicts detected — branch cannot merge without resolution.
    Conflicts,
    /// Check failed — conflict status is unreliable.
    Unknown,
}

impl std::fmt::Display for ConflictStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConflictStatus::Clean => write!(f, "Clean"),
            ConflictStatus::Conflicts => write!(f, "Conflicts"),
            ConflictStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Comprehensive branch health for a kild.
///
/// # Field Relationships
/// - `conflict_status`: Result of merging this branch into `drift.base_branch` (in-memory).
/// - `drift.ahead/behind`: Commit counts relative to base. Meaningful even without a remote.
/// - `diff_vs_base`: `None` if merge base calculation failed (logged as warning).
/// - `has_remote`: When false, push/PR-related readiness checks are skipped (local-only repo).
/// - `created_at`: Passthrough from session metadata, not validated here.
#[derive(Debug, Clone, Serialize)]
pub struct BranchHealth {
    pub branch: String,
    pub created_at: String,
    pub commit_activity: CommitActivity,
    pub drift: BaseBranchDrift,
    /// Total diff from merge base to branch tip (how big the PR will be).
    /// `None` if merge base could not be found or diff computation failed.
    pub diff_vs_base: Option<DiffStats>,
    /// Result of in-memory merge conflict detection.
    pub conflict_status: ConflictStatus,
    /// Whether any remote is configured. When false, PR-based readiness is skipped.
    pub has_remote: bool,
}

/// Computed merge readiness status for a branch.
///
/// Combines git health metrics with forge/PR data to determine
/// whether a branch is ready to merge.
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
    /// Conflict detection failed — status unknown, treat as blocked
    ConflictCheckFailed,
    /// Pushed but no PR exists
    NeedsPr,
    /// PR exists but CI is failing
    CiFailing,
    /// Ready to merge locally (no remote configured)
    ReadyLocal,
}

impl MergeReadiness {
    /// Compute merge readiness from git health metrics, worktree status, and optional PR info.
    ///
    /// Priority order (highest severity first):
    /// 1. HasConflicts / ConflictCheckFailed — blocks merge entirely
    /// 2. NeedsRebase — behind base, conflicts likely if not rebased
    /// 3. NeedsPush — local-only commits, PR can't be created/updated
    /// 4. NeedsPr — pushed but no tracking PR exists
    /// 5. CiFailing — PR exists but not passing checks
    /// 6. Ready / ReadyLocal — all checks passed
    pub fn compute(
        health: &BranchHealth,
        worktree_status: &Option<WorktreeStatus>,
        pr_info: Option<&PrInfo>,
    ) -> Self {
        match health.conflict_status {
            ConflictStatus::Conflicts => return Self::HasConflicts,
            ConflictStatus::Unknown => return Self::ConflictCheckFailed,
            ConflictStatus::Clean => {}
        }

        if health.drift.behind > 0 {
            return Self::NeedsRebase;
        }

        if !health.has_remote {
            return Self::ReadyLocal;
        }

        // Check if there are unpushed commits
        let has_unpushed = worktree_status
            .as_ref()
            .is_some_and(|ws| ws.unpushed_commit_count > 0 || !ws.has_remote_branch);

        if has_unpushed {
            return Self::NeedsPush;
        }

        let Some(pr) = pr_info else {
            return Self::NeedsPr;
        };

        if pr.ci_status == CiStatus::Failing {
            return Self::CiFailing;
        }

        Self::Ready
    }
}

impl std::fmt::Display for MergeReadiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MergeReadiness::Ready => write!(f, "Ready"),
            MergeReadiness::NeedsPush => write!(f, "Needs push"),
            MergeReadiness::NeedsRebase => write!(f, "Needs rebase"),
            MergeReadiness::HasConflicts => write!(f, "Has conflicts"),
            MergeReadiness::ConflictCheckFailed => write!(f, "Conflict check failed"),
            MergeReadiness::NeedsPr => write!(f, "Needs PR"),
            MergeReadiness::CiFailing => write!(f, "CI failing"),
            MergeReadiness::ReadyLocal => write!(f, "Ready (local)"),
        }
    }
}

/// A single file that is modified by multiple kilds.
///
/// Invariants (enforced at construction in `collect_file_overlaps`):
/// - `branches.len() >= 2` (overlap requires multiple branches)
/// - `branches` is sorted alphabetically and deduplicated
/// - Branch names are user-facing (not `kild/` prefixed)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileOverlap {
    /// The file path relative to the repository root.
    pub file: PathBuf,
    /// Branch names (user names, not kild/ prefixed) that modify this file.
    /// Guaranteed to contain at least 2 entries, sorted and deduplicated.
    pub branches: Vec<String>,
}

/// A kild with no file overlaps with other kilds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CleanKild {
    /// Branch name (user-facing, not kild/ prefixed).
    pub branch: String,
    /// Number of files changed in this kild relative to the base branch.
    pub changed_files: usize,
}

/// Report of file overlaps across kilds in a project.
///
/// Invariants (enforced at construction in `collect_file_overlaps`):
/// - `overlapping_files` sorted by branch count (desc), then file path (asc)
/// - `clean_kilds` sorted alphabetically by branch name
/// - Branches in `clean_kilds` do not appear in any `overlapping_files`
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OverlapReport {
    /// Files modified by more than one kild.
    /// Sorted by number of overlapping branches (descending), then by file path.
    pub overlapping_files: Vec<FileOverlap>,
    /// Kilds with no file overlaps with other kilds.
    /// Sorted alphabetically by branch name.
    pub clean_kilds: Vec<CleanKild>,
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

    #[test]
    fn test_commit_activity_default() {
        let activity = CommitActivity::default();
        assert_eq!(activity.commits_since_base, 0);
        assert!(activity.last_commit_time.is_none());
    }

    #[test]
    fn test_base_branch_drift_construction() {
        let drift = BaseBranchDrift {
            ahead: 3,
            behind: 5,
            base_branch: "main".to_string(),
        };
        assert_eq!(drift.ahead, 3);
        assert_eq!(drift.behind, 5);
        assert_eq!(drift.base_branch, "main");
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
            conflict_status: ConflictStatus::Clean,
            has_remote: true,
        };
        let value = serde_json::to_value(&health).expect("BranchHealth should serialize");
        assert_eq!(value["branch"], "feature-auth");
        assert_eq!(value["commit_activity"]["commits_since_base"], 4);
        assert_eq!(value["drift"]["behind"], 12);
        assert_eq!(value["diff_vs_base"]["insertions"], 450);
        assert_eq!(value["conflict_status"], "clean");
        assert_eq!(value["has_remote"], true);
    }

    #[test]
    fn test_conflict_status_display() {
        assert_eq!(ConflictStatus::Clean.to_string(), "Clean");
        assert_eq!(ConflictStatus::Conflicts.to_string(), "Conflicts");
        assert_eq!(ConflictStatus::Unknown.to_string(), "Unknown");
    }

    #[test]
    fn test_conflict_status_serde() {
        assert_eq!(
            serde_json::to_string(&ConflictStatus::Clean).unwrap(),
            "\"clean\""
        );
        assert_eq!(
            serde_json::to_string(&ConflictStatus::Conflicts).unwrap(),
            "\"conflicts\""
        );
        assert_eq!(
            serde_json::to_string(&ConflictStatus::Unknown).unwrap(),
            "\"unknown\""
        );
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

    #[test]
    fn test_file_overlap_serializes_to_json() {
        let overlap = FileOverlap {
            file: PathBuf::from("src/config.rs"),
            branches: vec!["feature-auth".to_string(), "feature-api".to_string()],
        };
        let value = serde_json::to_value(&overlap).expect("FileOverlap should serialize");
        assert_eq!(value["file"], "src/config.rs");
        assert_eq!(value["branches"][0], "feature-auth");
        assert_eq!(value["branches"][1], "feature-api");
    }

    #[test]
    fn test_overlap_report_serializes_to_json() {
        let report = OverlapReport {
            overlapping_files: vec![FileOverlap {
                file: PathBuf::from("src/lib.rs"),
                branches: vec!["branch-a".to_string(), "branch-b".to_string()],
            }],
            clean_kilds: vec![CleanKild {
                branch: "branch-c".to_string(),
                changed_files: 5,
            }],
        };
        let value = serde_json::to_value(&report).expect("OverlapReport should serialize");
        assert_eq!(value["overlapping_files"].as_array().unwrap().len(), 1);
        assert_eq!(value["overlapping_files"][0]["file"], "src/lib.rs");
        assert_eq!(value["clean_kilds"][0]["branch"], "branch-c");
        assert_eq!(value["clean_kilds"][0]["changed_files"], 5);
    }

    #[test]
    fn test_overlap_report_empty_serializes_to_json() {
        let report = OverlapReport {
            overlapping_files: vec![],
            clean_kilds: vec![],
        };
        let value = serde_json::to_value(&report).expect("Empty OverlapReport should serialize");
        assert!(value["overlapping_files"].as_array().unwrap().is_empty());
        assert!(value["clean_kilds"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_clean_kild_serializes_to_json() {
        let clean = CleanKild {
            branch: "feature-auth".to_string(),
            changed_files: 3,
        };
        let value = serde_json::to_value(&clean).expect("CleanKild should serialize");
        assert_eq!(value["branch"], "feature-auth");
        assert_eq!(value["changed_files"], 3);
    }
}
