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

/// PR metadata stored as a sidecar file (`{session_id}.pr`).
///
/// Fetched from a forge platform and cached locally.
/// Refreshed on demand via `kild pr --refresh`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrInfo {
    pub number: u32,
    pub url: String,
    pub state: PrState,
    pub ci_status: CiStatus,
    pub ci_summary: Option<String>,
    pub review_status: ReviewStatus,
    pub review_summary: Option<String>,
    pub updated_at: String,
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
    fn test_pr_info_serde_roundtrip() {
        let info = PrInfo {
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
        let parsed: PrInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, info);
    }

    #[test]
    fn test_pr_info_with_none_summaries() {
        let info = PrInfo {
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
        let parsed: PrInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, info);
    }
}
