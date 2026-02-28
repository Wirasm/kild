//! Forge type definitions and PR-related data structures.

use serde::{Deserialize, Serialize};

pub use kild_protocol::ForgeType;

/// Result of checking if a PR exists for a branch.
///
/// This is a proper enum instead of `Option<bool>` to make the semantics
/// explicit and self-documenting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrCheckResult {
    /// PR exists for this branch (open, merged, or closed).
    Exists,
    /// No PR found for this branch.
    NotFound,
    /// Could not check PR status.
    ///
    /// This happens when:
    /// - The forge CLI (gh, glab, etc.) is not installed or not authenticated
    /// - Network errors occurred
    /// - The worktree path doesn't exist
    /// - API rate limiting or other CLI errors
    #[default]
    Unavailable,
}

impl PrCheckResult {
    /// Returns true if a PR definitely exists.
    pub fn exists(&self) -> bool {
        matches!(self, PrCheckResult::Exists)
    }

    /// Returns true if we confirmed no PR exists.
    pub fn not_found(&self) -> bool {
        matches!(self, PrCheckResult::NotFound)
    }

    /// Returns true if we couldn't check PR status.
    pub fn is_unavailable(&self) -> bool {
        matches!(self, PrCheckResult::Unavailable)
    }
}

/// PR state from a forge platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrState {
    Open,
    Draft,
    Merged,
    Closed,
}

impl std::fmt::Display for PrState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::Draft => write!(f, "draft"),
            Self::Merged => write!(f, "merged"),
            Self::Closed => write!(f, "closed"),
        }
    }
}

/// CI/check suite status summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CiStatus {
    Passing,
    Failing,
    Pending,
    Unknown,
}

impl std::fmt::Display for CiStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Passing => write!(f, "passing"),
            Self::Failing => write!(f, "failing"),
            Self::Pending => write!(f, "pending"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Review status summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStatus {
    Approved,
    ChangesRequested,
    Pending,
    Unknown,
}

impl std::fmt::Display for ReviewStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Approved => write!(f, "approved"),
            Self::ChangesRequested => write!(f, "changes_requested"),
            Self::Pending => write!(f, "pending"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Merge strategy for landing a PR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MergeStrategy {
    /// Squash all commits into one (default).
    #[default]
    Squash,
    /// Create a merge commit.
    Merge,
    /// Rebase commits onto the base branch.
    Rebase,
}

impl MergeStrategy {
    /// Returns the `gh pr merge` flag for this strategy.
    pub fn gh_flag(&self) -> &'static str {
        match self {
            MergeStrategy::Squash => "--squash",
            MergeStrategy::Merge => "--merge",
            MergeStrategy::Rebase => "--rebase",
        }
    }
}

impl std::fmt::Display for MergeStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Squash => write!(f, "squash"),
            Self::Merge => write!(f, "merge"),
            Self::Rebase => write!(f, "rebase"),
        }
    }
}

impl std::str::FromStr for MergeStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "squash" => Ok(MergeStrategy::Squash),
            "merge" => Ok(MergeStrategy::Merge),
            "rebase" => Ok(MergeStrategy::Rebase),
            other => Err(format!(
                "Unknown merge strategy '{}'. Valid: squash, merge, rebase",
                other
            )),
        }
    }
}

/// PR metadata stored as a sidecar file (`{session_id}.pr`).
///
/// Fetched from a forge platform and cached locally.
/// Refreshed on demand via `kild pr --refresh`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: u32,
    pub url: String,
    pub state: PrState,
    pub ci_status: CiStatus,
    pub ci_summary: Option<String>,
    pub review_status: ReviewStatus,
    pub review_summary: Option<String>,
    pub updated_at: String,
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
    /// Compute merge readiness from extracted git and forge signals.
    ///
    /// Parameters are primitive values extracted by the caller from git/forge
    /// data, keeping the forge module decoupled from git types.
    ///
    /// Priority order (highest severity first):
    /// 1. HasConflicts / ConflictCheckFailed — blocks merge entirely
    /// 2. NeedsRebase — behind base, conflicts likely if not rebased
    /// 3. NeedsPush — local-only commits, PR can't be created/updated
    /// 4. NeedsPr — pushed but no tracking PR exists
    /// 5. CiFailing — PR exists but not passing checks
    /// 6. Ready / ReadyLocal — all checks passed
    ///
    /// # Parameters
    ///
    /// - `merge_clean`: `Some(true)` = clean, `Some(false)` = conflicts, `None` = check failed.
    ///   Obtained via `ConflictStatus::is_clean()`.
    /// - `behind_base`: commits behind the base branch (from `BaseBranchDrift::behind`).
    /// - `has_remote`: whether any remote is configured.
    /// - `has_unpushed`: whether there are unpushed commits or the branch was never pushed.
    ///   Obtained via `WorktreeStatus::has_unpushed()`.
    /// - `pr_info`: optional PR metadata from the forge.
    pub fn compute(
        merge_clean: Option<bool>,
        behind_base: usize,
        has_remote: bool,
        has_unpushed: bool,
        pr_info: Option<&PullRequest>,
    ) -> Self {
        match merge_clean {
            Some(false) => return Self::HasConflicts,
            None => return Self::ConflictCheckFailed,
            Some(true) => {}
        }

        if behind_base > 0 {
            return Self::NeedsRebase;
        }

        if !has_remote {
            return Self::ReadyLocal;
        }

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
            Self::Ready => write!(f, "Ready"),
            Self::NeedsPush => write!(f, "Needs push"),
            Self::NeedsRebase => write!(f, "Needs rebase"),
            Self::HasConflicts => write!(f, "Has conflicts"),
            Self::ConflictCheckFailed => write!(f, "Conflict check failed"),
            Self::NeedsPr => write!(f, "Needs PR"),
            Self::CiFailing => write!(f, "CI failing"),
            Self::ReadyLocal => write!(f, "Ready (local)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_forge_type_as_str() {
        assert_eq!(ForgeType::GitHub.as_str(), "github");
    }

    #[test]
    fn test_forge_type_from_str_case_insensitive() {
        use std::str::FromStr;
        assert_eq!(ForgeType::from_str("github"), Ok(ForgeType::GitHub));
        assert_eq!(ForgeType::from_str("GITHUB"), Ok(ForgeType::GitHub));
        assert_eq!(ForgeType::from_str("GitHub"), Ok(ForgeType::GitHub));
        assert!(ForgeType::from_str("unknown").is_err());
        assert!(ForgeType::from_str("").is_err());
    }

    #[test]
    fn test_forge_type_display() {
        assert_eq!(format!("{}", ForgeType::GitHub), "github");
    }

    #[test]
    fn test_forge_type_from_str() {
        use std::str::FromStr;
        assert_eq!(ForgeType::from_str("github").unwrap(), ForgeType::GitHub);
        assert_eq!(ForgeType::from_str("GITHUB").unwrap(), ForgeType::GitHub);

        let err = ForgeType::from_str("unknown").unwrap_err();
        assert!(err.contains("Unknown forge 'unknown'"));
        assert!(err.contains("github"));
    }

    #[test]
    fn test_forge_type_serde() {
        let github = ForgeType::GitHub;
        let json = serde_json::to_string(&github).unwrap();
        assert_eq!(json, "\"github\"");

        let parsed: ForgeType = serde_json::from_str("\"github\"").unwrap();
        assert_eq!(parsed, ForgeType::GitHub);
    }

    #[test]
    fn test_forge_type_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ForgeType::GitHub);
        set.insert(ForgeType::GitHub); // Duplicate
        assert_eq!(set.len(), 1);
    }

    // --- PrCheckResult tests ---

    #[test]
    fn test_pr_check_result_exists() {
        let result = PrCheckResult::Exists;
        assert!(result.exists());
        assert!(!result.not_found());
        assert!(!result.is_unavailable());
    }

    #[test]
    fn test_pr_check_result_not_found() {
        let result = PrCheckResult::NotFound;
        assert!(!result.exists());
        assert!(result.not_found());
        assert!(!result.is_unavailable());
    }

    #[test]
    fn test_pr_check_result_unavailable() {
        let result = PrCheckResult::Unavailable;
        assert!(!result.exists());
        assert!(!result.not_found());
        assert!(result.is_unavailable());
    }

    #[test]
    fn test_pr_check_result_default() {
        let result = PrCheckResult::default();
        assert!(result.is_unavailable());
    }

    // --- PR type Display tests ---

    #[test]
    fn test_pr_state_display() {
        assert_eq!(PrState::Open.to_string(), "open");
        assert_eq!(PrState::Draft.to_string(), "draft");
        assert_eq!(PrState::Merged.to_string(), "merged");
        assert_eq!(PrState::Closed.to_string(), "closed");
    }

    #[test]
    fn test_ci_status_display() {
        assert_eq!(CiStatus::Passing.to_string(), "passing");
        assert_eq!(CiStatus::Failing.to_string(), "failing");
        assert_eq!(CiStatus::Pending.to_string(), "pending");
        assert_eq!(CiStatus::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_review_status_display() {
        assert_eq!(ReviewStatus::Approved.to_string(), "approved");
        assert_eq!(
            ReviewStatus::ChangesRequested.to_string(),
            "changes_requested"
        );
        assert_eq!(ReviewStatus::Pending.to_string(), "pending");
        assert_eq!(ReviewStatus::Unknown.to_string(), "unknown");
    }

    // --- Serde roundtrip tests ---

    #[test]
    fn test_pr_state_serde_roundtrip() {
        for state in [
            PrState::Open,
            PrState::Draft,
            PrState::Merged,
            PrState::Closed,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            let parsed: PrState = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, state);
        }
    }

    #[test]
    fn test_ci_status_serde_roundtrip() {
        for status in [
            CiStatus::Passing,
            CiStatus::Failing,
            CiStatus::Pending,
            CiStatus::Unknown,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: CiStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn test_review_status_serde_roundtrip() {
        for status in [
            ReviewStatus::Approved,
            ReviewStatus::ChangesRequested,
            ReviewStatus::Pending,
            ReviewStatus::Unknown,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: ReviewStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn test_pull_request_serde_roundtrip() {
        let info = PullRequest {
            number: 45,
            url: "https://github.com/org/repo/pull/45".to_string(),
            state: PrState::Open,
            ci_status: CiStatus::Passing,
            ci_summary: Some("3/3 passing".to_string()),
            review_status: ReviewStatus::Approved,
            review_summary: Some("1 approved".to_string()),
            updated_at: "2026-02-05T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: PullRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, info);
    }

    #[test]
    fn test_pull_request_with_none_summaries() {
        let info = PullRequest {
            number: 1,
            url: "https://github.com/org/repo/pull/1".to_string(),
            state: PrState::Draft,
            ci_status: CiStatus::Unknown,
            ci_summary: None,
            review_status: ReviewStatus::Unknown,
            review_summary: None,
            updated_at: "2026-02-05T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: PullRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, info);
    }

    // --- MergeReadiness tests ---
    // Tests use scalar params directly — no git type dependencies.

    fn make_pr(ci_status: CiStatus) -> PullRequest {
        PullRequest {
            number: 1,
            url: "https://example.com/pull/1".to_string(),
            state: PrState::Open,
            ci_status,
            ci_summary: None,
            review_status: ReviewStatus::Unknown,
            review_summary: None,
            updated_at: "2026-02-09T12:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_readiness_has_conflicts() {
        assert_eq!(
            MergeReadiness::compute(Some(false), 0, true, false, None),
            MergeReadiness::HasConflicts
        );
    }

    #[test]
    fn test_readiness_conflict_check_failed() {
        assert_eq!(
            MergeReadiness::compute(None, 0, true, false, None),
            MergeReadiness::ConflictCheckFailed
        );
    }

    #[test]
    fn test_readiness_needs_rebase() {
        assert_eq!(
            MergeReadiness::compute(Some(true), 5, true, false, None),
            MergeReadiness::NeedsRebase
        );
    }

    #[test]
    fn test_readiness_ready_local() {
        assert_eq!(
            MergeReadiness::compute(Some(true), 0, false, false, None),
            MergeReadiness::ReadyLocal
        );
    }

    #[test]
    fn test_readiness_needs_push() {
        assert_eq!(
            MergeReadiness::compute(Some(true), 0, true, true, None),
            MergeReadiness::NeedsPush
        );
    }

    #[test]
    fn test_readiness_needs_pr() {
        assert_eq!(
            MergeReadiness::compute(Some(true), 0, true, false, None),
            MergeReadiness::NeedsPr
        );
    }

    #[test]
    fn test_readiness_ci_failing() {
        let pr = make_pr(CiStatus::Failing);
        assert_eq!(
            MergeReadiness::compute(Some(true), 0, true, false, Some(&pr)),
            MergeReadiness::CiFailing
        );
    }

    #[test]
    fn test_readiness_ready() {
        let pr = make_pr(CiStatus::Passing);
        assert_eq!(
            MergeReadiness::compute(Some(true), 0, true, false, Some(&pr)),
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
        let pr = make_pr(CiStatus::Pending);
        assert_eq!(
            MergeReadiness::compute(Some(true), 0, true, false, Some(&pr)),
            MergeReadiness::Ready
        );
    }

    #[test]
    fn test_readiness_ready_with_unknown_ci() {
        let pr = make_pr(CiStatus::Unknown);
        assert_eq!(
            MergeReadiness::compute(Some(true), 0, true, false, Some(&pr)),
            MergeReadiness::Ready
        );
    }

    #[test]
    fn test_readiness_ready_with_draft_pr() {
        let pr = PullRequest {
            state: PrState::Draft,
            ..make_pr(CiStatus::Passing)
        };
        assert_eq!(
            MergeReadiness::compute(Some(true), 0, true, false, Some(&pr)),
            MergeReadiness::Ready
        );
    }

    // --- MergeStrategy tests ---

    #[test]
    fn test_merge_strategy_default_is_squash() {
        assert_eq!(MergeStrategy::default(), MergeStrategy::Squash);
    }

    #[test]
    fn test_merge_strategy_display() {
        assert_eq!(MergeStrategy::Squash.to_string(), "squash");
        assert_eq!(MergeStrategy::Merge.to_string(), "merge");
        assert_eq!(MergeStrategy::Rebase.to_string(), "rebase");
    }

    #[test]
    fn test_merge_strategy_gh_flag() {
        assert_eq!(MergeStrategy::Squash.gh_flag(), "--squash");
        assert_eq!(MergeStrategy::Merge.gh_flag(), "--merge");
        assert_eq!(MergeStrategy::Rebase.gh_flag(), "--rebase");
    }

    #[test]
    fn test_merge_strategy_from_str() {
        use std::str::FromStr;
        assert_eq!(MergeStrategy::from_str("squash"), Ok(MergeStrategy::Squash));
        assert_eq!(MergeStrategy::from_str("merge"), Ok(MergeStrategy::Merge));
        assert_eq!(MergeStrategy::from_str("rebase"), Ok(MergeStrategy::Rebase));
        assert_eq!(MergeStrategy::from_str("SQUASH"), Ok(MergeStrategy::Squash));
        assert!(MergeStrategy::from_str("invalid").is_err());
    }
}
