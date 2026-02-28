//! GitHub forge backend implementation.

use std::borrow::Cow;
use std::path::Path;

use tracing::{debug, error, info, warn};

use crate::forge::errors::ForgeError;
use crate::forge::traits::ForgeBackend;
use crate::forge::types::{
    CiStatus, MergeStrategy, PrCheckResult, PrState, PullRequest, ReviewStatus,
};
use crate::git::naming::{KILD_BRANCH_PREFIX, kild_branch_name};

/// GitHub forge backend using the `gh` CLI.
pub struct GitHubBackend;

/// Ensure the branch name has the `kild/` prefix for GitHub API queries.
///
/// KILD pushes branches as `kild/<branch>`, so `gh pr view` needs the full ref.
/// Callers should always pass the full ref, but this normalizes defensively to
/// prevent "no pull requests found" errors from short branch names.
fn normalize_branch(branch: &str) -> Cow<'_, str> {
    if branch.starts_with(KILD_BRANCH_PREFIX) {
        Cow::Borrowed(branch)
    } else {
        Cow::Owned(kild_branch_name(branch))
    }
}

impl ForgeBackend for GitHubBackend {
    fn name(&self) -> &'static str {
        "github"
    }

    fn display_name(&self) -> &'static str {
        "GitHub"
    }

    fn is_available(&self) -> bool {
        which::which("gh").is_ok()
    }

    fn is_pr_merged(&self, worktree_path: &Path, branch: &str) -> Result<bool, ForgeError> {
        let branch = normalize_branch(branch);
        debug!(
            event = "core.forge.pr_merge_check_started",
            branch = %branch,
            worktree_path = %worktree_path.display()
        );

        let output = std::process::Command::new("gh")
            .current_dir(worktree_path)
            .args(["pr", "view", &branch, "--json", "state", "-q", ".state"])
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let state = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .to_uppercase();
                let merged = state == "MERGED";
                debug!(
                    event = "core.forge.pr_merge_check_completed",
                    branch = %branch,
                    state = %state,
                    merged = merged
                );
                Ok(merged)
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                // "no pull requests found" is not an error — it means the PR doesn't exist
                if stderr.contains("no pull requests found")
                    || stderr.contains("Could not resolve")
                    || stderr.contains("no open pull requests")
                {
                    debug!(event = "core.forge.pr_merge_check_no_pr", branch = %branch);
                    Ok(false)
                } else {
                    Err(ForgeError::CliError {
                        message: format!(
                            "gh pr view failed (exit {}): {}",
                            output.status.code().unwrap_or(-1),
                            stderr.trim()
                        ),
                    })
                }
            }
            Err(e) => Err(ForgeError::from(e)),
        }
    }

    fn check_pr_exists(&self, worktree_path: &Path, branch: &str) -> PrCheckResult {
        let branch = normalize_branch(branch);
        debug!(
            event = "core.forge.pr_exists_check_started",
            branch = %branch
        );

        if !worktree_path.exists() {
            debug!(
                event = "core.forge.pr_exists_check_skipped",
                reason = "worktree_missing"
            );
            return PrCheckResult::Unavailable;
        }

        let output = std::process::Command::new("gh")
            .current_dir(worktree_path)
            .args(["pr", "view", &branch, "--json", "state"])
            .output();

        match output {
            Ok(output) if output.status.success() => PrCheckResult::Exists,
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("no pull requests found")
                    || stderr.contains("Could not resolve")
                    || stderr.contains("no open pull requests")
                {
                    PrCheckResult::NotFound
                } else {
                    warn!(
                        event = "core.forge.pr_exists_check_error",
                        branch = %branch,
                        exit_code = output.status.code(),
                        stderr = %stderr.trim(),
                        "gh CLI error - PR status unavailable"
                    );
                    PrCheckResult::Unavailable
                }
            }
            Err(e) => {
                debug!(
                    event = "core.forge.pr_exists_check_unavailable",
                    error = %e,
                    "gh CLI not available"
                );
                PrCheckResult::Unavailable
            }
        }
    }

    fn fetch_pr_info(
        &self,
        worktree_path: &Path,
        branch: &str,
    ) -> Result<Option<PullRequest>, ForgeError> {
        let branch = normalize_branch(branch);
        debug!(
            event = "core.forge.pr_info_fetch_started",
            branch = %branch,
            worktree_path = %worktree_path.display()
        );

        let output = std::process::Command::new("gh")
            .current_dir(worktree_path)
            .args([
                "pr",
                "view",
                &branch,
                "--json",
                "number,url,state,statusCheckRollup,reviews,isDraft",
            ])
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let json_str = String::from_utf8_lossy(&output.stdout);
                Ok(parse_gh_pr_json(&json_str, &branch))
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("no pull requests found")
                    || stderr.contains("Could not resolve")
                    || stderr.contains("no open pull requests")
                {
                    debug!(event = "core.forge.pr_info_fetch_no_pr", branch = %branch);
                    Ok(None)
                } else {
                    Err(ForgeError::CliError {
                        message: format!(
                            "gh pr view failed (exit {}): {}",
                            output.status.code().unwrap_or(-1),
                            stderr.trim()
                        ),
                    })
                }
            }
            Err(e) => Err(ForgeError::from(e)),
        }
    }

    fn merge_pr(
        &self,
        worktree_path: &Path,
        branch: &str,
        strategy: MergeStrategy,
    ) -> Result<(), ForgeError> {
        let branch = normalize_branch(branch);
        info!(
            event = "core.forge.merge_started",
            branch = %branch,
            strategy = %strategy,
            worktree_path = %worktree_path.display()
        );

        let output = std::process::Command::new("gh")
            .current_dir(worktree_path)
            .args(["pr", "merge", &branch, strategy.gh_flag()])
            .output();

        match output {
            Ok(output) if output.status.success() => {
                info!(
                    event = "core.forge.merge_completed",
                    branch = %branch,
                    strategy = %strategy
                );
                Ok(())
            }
            Ok(output) => {
                let exit_code = output.status.code().unwrap_or(-1);
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                error!(
                    event = "core.forge.merge_failed",
                    branch = %branch,
                    exit_code = exit_code,
                    stderr = %stderr
                );
                Err(ForgeError::CliError {
                    message: format!("gh pr merge failed (exit {}): {}", exit_code, stderr),
                })
            }
            Err(e) => Err(ForgeError::from(e)),
        }
    }
}

/// Parse the JSON output from `gh pr view` into a `PullRequest`.
///
/// Expects JSON with fields: number, url, state, isDraft, statusCheckRollup, reviews.
/// Returns `None` if JSON is malformed or required fields are missing (logged as warnings).
fn parse_gh_pr_json(json_str: &str, branch: &str) -> Option<PullRequest> {
    let value: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => {
            warn!(
                event = "core.forge.pr_info_parse_failed",
                branch = branch,
                error = %e
            );
            return None;
        }
    };

    let number = match value.get("number").and_then(|v| v.as_u64()) {
        Some(n) => n as u32,
        None => {
            warn!(
                event = "core.forge.pr_info_missing_field",
                branch = branch,
                field = "number",
            );
            return None;
        }
    };
    let url = match value.get("url").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            warn!(
                event = "core.forge.pr_info_missing_field",
                branch = branch,
                field = "url",
            );
            return None;
        }
    };
    let is_draft = value
        .get("isDraft")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let gh_state = match value.get("state").and_then(|v| v.as_str()) {
        Some(s) => s.to_uppercase(),
        None => {
            warn!(
                event = "core.forge.pr_info_missing_field",
                branch = branch,
                field = "state",
            );
            return None;
        }
    };

    let state = match gh_state.as_str() {
        "MERGED" => PrState::Merged,
        "CLOSED" => PrState::Closed,
        "OPEN" if is_draft => PrState::Draft,
        "OPEN" => PrState::Open,
        unknown => {
            warn!(
                event = "core.forge.pr_state_unknown",
                branch = branch,
                state = unknown,
                "Unknown PR state from gh CLI — treating as Open"
            );
            PrState::Open
        }
    };

    let (ci_status, ci_summary) = parse_ci_status(&value);
    let (review_status, review_summary) = parse_review_status(&value);

    let now = chrono::Utc::now().to_rfc3339();

    info!(
        event = "core.forge.pr_info_fetch_completed",
        branch = branch,
        pr_number = number,
        pr_state = %state,
        ci_status = %ci_status,
        review_status = %review_status
    );

    Some(PullRequest {
        number,
        url,
        state,
        ci_status,
        ci_summary,
        review_status,
        review_summary,
        updated_at: now,
    })
}

/// Parse `statusCheckRollup` array from gh output into CI status.
///
/// Priority: Failing > Pending > Passing. If any check fails, the overall
/// status is Failing. If none fail but some are pending, it's Pending.
fn parse_ci_status(value: &serde_json::Value) -> (CiStatus, Option<String>) {
    let checks = match value.get("statusCheckRollup").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return (CiStatus::Unknown, None),
    };

    if checks.is_empty() {
        return (CiStatus::Unknown, None);
    }

    let mut passing = 0u32;
    let mut failing = 0u32;
    let mut pending = 0u32;

    for check in checks {
        let conclusion = check
            .get("conclusion")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let status = check.get("status").and_then(|v| v.as_str()).unwrap_or("");

        let conclusion_upper = conclusion.to_uppercase();
        match conclusion_upper.as_str() {
            "SUCCESS" | "NEUTRAL" | "SKIPPED" => passing += 1,
            "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE" => {
                failing += 1
            }
            "" => {
                // No conclusion yet - check status field
                let status_upper = status.to_uppercase();
                match status_upper.as_str() {
                    "COMPLETED" => passing += 1,
                    "IN_PROGRESS" | "QUEUED" | "REQUESTED" | "WAITING" | "PENDING" => pending += 1,
                    _ => pending += 1,
                }
            }
            _ => pending += 1,
        }
    }

    let total = passing + failing + pending;
    let summary = format!("{}/{} passing", passing, total);

    let ci_status = match (failing > 0, pending > 0) {
        (true, _) => CiStatus::Failing,
        (false, true) => CiStatus::Pending,
        (false, false) => CiStatus::Passing,
    };

    (ci_status, Some(summary))
}

/// Parse `reviews` array from gh output into review status.
///
/// Deduplicates reviews by author (last review wins per GitHub API array order),
/// then applies priority: ChangesRequested > Approved > Pending.
fn parse_review_status(value: &serde_json::Value) -> (ReviewStatus, Option<String>) {
    let reviews = match value.get("reviews").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return (ReviewStatus::Unknown, None),
    };

    if reviews.is_empty() {
        return (ReviewStatus::Pending, None);
    }

    // Deduplicate reviews by author — last entry per author wins (GitHub API array order)
    let mut latest_by_author: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for review in reviews {
        let author = review
            .get("author")
            .and_then(|a| a.get("login"))
            .and_then(|l| l.as_str())
            .unwrap_or("unknown")
            .to_string();

        let state = review
            .get("state")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_uppercase();

        // Skip COMMENTED and DISMISSED — they don't represent a review decision
        if matches!(state.as_str(), "APPROVED" | "CHANGES_REQUESTED" | "PENDING") {
            latest_by_author.insert(author, state);
        }
    }

    let mut approved = 0u32;
    let mut changes_requested = 0u32;
    let mut pending_reviews = 0u32;

    for state in latest_by_author.values() {
        match state.as_str() {
            "APPROVED" => approved += 1,
            "CHANGES_REQUESTED" => changes_requested += 1,
            _ => pending_reviews += 1,
        }
    }

    let mut parts = Vec::new();
    if approved > 0 {
        parts.push(format!("{} approved", approved));
    }
    if changes_requested > 0 {
        parts.push(format!("{} changes requested", changes_requested));
    }
    if pending_reviews > 0 {
        parts.push(format!("{} pending", pending_reviews));
    }

    let summary = if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    };

    let review_status = match (changes_requested > 0, approved > 0) {
        (true, _) => ReviewStatus::ChangesRequested,
        (false, true) => ReviewStatus::Approved,
        (false, false) => ReviewStatus::Pending,
    };

    (review_status, summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_backend_name() {
        let backend = GitHubBackend;
        assert_eq!(backend.name(), "github");
        assert_eq!(backend.display_name(), "GitHub");
    }

    #[test]
    fn test_normalize_branch_adds_prefix() {
        let result = normalize_branch("my-feature");
        assert_eq!(result.as_ref(), "kild/my-feature");
    }

    #[test]
    fn test_normalize_branch_preserves_existing_prefix() {
        let result = normalize_branch("kild/my-feature");
        assert_eq!(result.as_ref(), "kild/my-feature");
    }

    #[test]
    fn test_normalize_branch_nested_slashes() {
        let result = normalize_branch("feature/auth");
        assert_eq!(result.as_ref(), "kild/feature/auth");
    }

    #[test]
    fn test_parse_gh_pr_json_valid() {
        let json = r#"{
            "number": 42,
            "url": "https://github.com/org/repo/pull/42",
            "state": "OPEN",
            "isDraft": false,
            "statusCheckRollup": [],
            "reviews": []
        }"#;

        let result = parse_gh_pr_json(json, "test-branch");
        assert!(result.is_some());
        let pr = result.unwrap();
        assert_eq!(pr.number, 42);
        assert_eq!(pr.url, "https://github.com/org/repo/pull/42");
        assert_eq!(pr.state, PrState::Open);
    }

    #[test]
    fn test_parse_gh_pr_json_draft() {
        let json = r#"{
            "number": 10,
            "url": "https://github.com/org/repo/pull/10",
            "state": "OPEN",
            "isDraft": true,
            "statusCheckRollup": [],
            "reviews": []
        }"#;

        let result = parse_gh_pr_json(json, "draft-branch");
        assert!(result.is_some());
        assert_eq!(result.unwrap().state, PrState::Draft);
    }

    #[test]
    fn test_parse_gh_pr_json_merged() {
        let json = r#"{
            "number": 5,
            "url": "https://github.com/org/repo/pull/5",
            "state": "MERGED",
            "isDraft": false,
            "statusCheckRollup": [],
            "reviews": []
        }"#;

        let result = parse_gh_pr_json(json, "merged-branch");
        assert!(result.is_some());
        assert_eq!(result.unwrap().state, PrState::Merged);
    }

    #[test]
    fn test_parse_gh_pr_json_invalid() {
        let result = parse_gh_pr_json("not json", "test");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_gh_pr_json_missing_fields() {
        let json = r#"{"number": 1}"#;
        let result = parse_gh_pr_json(json, "test");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_ci_status_all_passing() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
            "statusCheckRollup": [
                {"conclusion": "SUCCESS", "status": "COMPLETED"},
                {"conclusion": "SUCCESS", "status": "COMPLETED"},
                {"conclusion": "NEUTRAL", "status": "COMPLETED"}
            ]
        }"#,
        )
        .unwrap();

        let (status, summary) = parse_ci_status(&json);
        assert_eq!(status, CiStatus::Passing);
        assert_eq!(summary, Some("3/3 passing".to_string()));
    }

    #[test]
    fn test_parse_ci_status_with_failure() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
            "statusCheckRollup": [
                {"conclusion": "SUCCESS", "status": "COMPLETED"},
                {"conclusion": "FAILURE", "status": "COMPLETED"}
            ]
        }"#,
        )
        .unwrap();

        let (status, summary) = parse_ci_status(&json);
        assert_eq!(status, CiStatus::Failing);
        assert_eq!(summary, Some("1/2 passing".to_string()));
    }

    #[test]
    fn test_parse_ci_status_with_pending() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
            "statusCheckRollup": [
                {"conclusion": "SUCCESS", "status": "COMPLETED"},
                {"conclusion": "", "status": "IN_PROGRESS"}
            ]
        }"#,
        )
        .unwrap();

        let (status, summary) = parse_ci_status(&json);
        assert_eq!(status, CiStatus::Pending);
        assert_eq!(summary, Some("1/2 passing".to_string()));
    }

    #[test]
    fn test_parse_ci_status_empty_array() {
        let json: serde_json::Value = serde_json::from_str(r#"{"statusCheckRollup": []}"#).unwrap();
        let (status, summary) = parse_ci_status(&json);
        assert_eq!(status, CiStatus::Unknown);
        assert!(summary.is_none());
    }

    #[test]
    fn test_parse_ci_status_no_field() {
        let json: serde_json::Value = serde_json::from_str(r#"{}"#).unwrap();
        let (status, summary) = parse_ci_status(&json);
        assert_eq!(status, CiStatus::Unknown);
        assert!(summary.is_none());
    }

    #[test]
    fn test_parse_review_status_approved() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
            "reviews": [
                {"author": {"login": "alice"}, "state": "APPROVED"}
            ]
        }"#,
        )
        .unwrap();

        let (status, summary) = parse_review_status(&json);
        assert_eq!(status, ReviewStatus::Approved);
        assert_eq!(summary, Some("1 approved".to_string()));
    }

    #[test]
    fn test_parse_review_status_changes_requested() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
            "reviews": [
                {"author": {"login": "bob"}, "state": "CHANGES_REQUESTED"}
            ]
        }"#,
        )
        .unwrap();

        let (status, summary) = parse_review_status(&json);
        assert_eq!(status, ReviewStatus::ChangesRequested);
        assert_eq!(summary, Some("1 changes requested".to_string()));
    }

    #[test]
    fn test_parse_review_status_deduplicates_by_author() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
            "reviews": [
                {"author": {"login": "alice"}, "state": "CHANGES_REQUESTED"},
                {"author": {"login": "alice"}, "state": "APPROVED"}
            ]
        }"#,
        )
        .unwrap();

        let (status, _summary) = parse_review_status(&json);
        // Latest review from alice is APPROVED (overwrites CHANGES_REQUESTED)
        assert_eq!(status, ReviewStatus::Approved);
    }

    #[test]
    fn test_parse_review_status_empty_reviews() {
        let json: serde_json::Value = serde_json::from_str(r#"{"reviews": []}"#).unwrap();
        let (status, summary) = parse_review_status(&json);
        assert_eq!(status, ReviewStatus::Pending);
        assert!(summary.is_none());
    }

    #[test]
    fn test_parse_review_status_no_field() {
        let json: serde_json::Value = serde_json::from_str(r#"{}"#).unwrap();
        let (status, summary) = parse_review_status(&json);
        assert_eq!(status, ReviewStatus::Unknown);
        assert!(summary.is_none());
    }

    #[test]
    fn test_parse_review_status_skips_commented() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
            "reviews": [
                {"author": {"login": "alice"}, "state": "COMMENTED"}
            ]
        }"#,
        )
        .unwrap();

        let (status, summary) = parse_review_status(&json);
        // COMMENTED is skipped, so no review decisions exist → Pending
        assert_eq!(status, ReviewStatus::Pending);
        assert!(summary.is_none());
    }
}
