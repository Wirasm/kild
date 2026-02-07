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
        check_pr_merged(&session.worktree_path, &kild_branch)
    } else {
        debug!(
            event = "core.session.complete_no_remote",
            branch = name,
            "No remote configured — skipping PR check"
        );
        false
    };

    info!(
        event = "core.session.complete_pr_status",
        branch = name,
        pr_merged = pr_merged
    );

    // 3. Determine the result based on PR status and remote deletion outcome
    let result = if !pr_merged {
        CompleteResult::PrNotMerged
    } else {
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
                // Non-fatal: remote might already be deleted, not exist, or deletion failed
                warn!(
                    event = "core.session.complete_remote_delete_failed",
                    branch = kild_branch,
                    worktree_path = %session.worktree_path.display(),
                    error = %e
                );
                CompleteResult::RemoteDeleteFailed
            }
        }
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

/// Fetch rich PR info from GitHub via `gh pr view`.
///
/// Queries `gh pr view <branch> --json number,url,state,statusCheckRollup,reviews,isDraft`
/// and parses the JSON output into a `PrInfo` struct.
///
/// Returns `None` on any error (gh unavailable, no PR, parse error).
pub fn fetch_pr_info(
    worktree_path: &std::path::Path,
    branch: &str,
) -> Option<super::types::PrInfo> {
    debug!(
        event = "core.session.pr_info_fetch_started",
        branch = branch,
        worktree_path = %worktree_path.display()
    );

    let output = std::process::Command::new("gh")
        .current_dir(worktree_path)
        .args([
            "pr",
            "view",
            branch,
            "--json",
            "number,url,state,statusCheckRollup,reviews,isDraft",
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let json_str = String::from_utf8_lossy(&output.stdout);
            parse_gh_pr_json(&json_str, branch)
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            debug!(
                event = "core.session.pr_info_fetch_no_pr",
                branch = branch,
                stderr = %stderr.trim()
            );
            None
        }
        Err(e) => {
            warn!(
                event = "core.session.pr_info_fetch_failed",
                branch = branch,
                error = %e,
                hint = "gh CLI may not be installed or accessible"
            );
            None
        }
    }
}

/// Parse the JSON output from `gh pr view` into a `PrInfo`.
fn parse_gh_pr_json(json_str: &str, branch: &str) -> Option<super::types::PrInfo> {
    let value: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => {
            warn!(
                event = "core.session.pr_info_parse_failed",
                branch = branch,
                error = %e
            );
            return None;
        }
    };

    let number = value.get("number")?.as_u64()? as u32;
    let url = value.get("url")?.as_str()?.to_string();
    let is_draft = value
        .get("isDraft")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let gh_state = value.get("state")?.as_str()?.to_uppercase();

    let state = match gh_state.as_str() {
        "MERGED" => super::types::PrState::Merged,
        "CLOSED" => super::types::PrState::Closed,
        "OPEN" if is_draft => super::types::PrState::Draft,
        _ => super::types::PrState::Open,
    };

    // Parse statusCheckRollup for CI status
    let (ci_status, ci_summary) = parse_ci_status(&value);
    // Parse reviews for review status
    let (review_status, review_summary) = parse_review_status(&value);

    let now = chrono::Utc::now().to_rfc3339();

    info!(
        event = "core.session.pr_info_fetch_completed",
        branch = branch,
        pr_number = number,
        pr_state = %state,
        ci_status = %ci_status,
        review_status = %review_status
    );

    Some(super::types::PrInfo {
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
fn parse_ci_status(value: &serde_json::Value) -> (super::types::CiStatus, Option<String>) {
    let checks = match value.get("statusCheckRollup").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return (super::types::CiStatus::Unknown, None),
    };

    if checks.is_empty() {
        return (super::types::CiStatus::Unknown, None);
    }

    let mut passing = 0u32;
    let mut failing = 0u32;
    let mut pending = 0u32;

    for check in checks {
        // gh returns either "conclusion" (for completed checks) or "status" (for in-progress)
        let conclusion = check
            .get("conclusion")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let status = check.get("status").and_then(|v| v.as_str()).unwrap_or("");

        match conclusion.to_uppercase().as_str() {
            "SUCCESS" | "NEUTRAL" | "SKIPPED" => passing += 1,
            "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE" => {
                failing += 1
            }
            _ => {
                // No conclusion yet - check status
                match status.to_uppercase().as_str() {
                    "COMPLETED" => passing += 1,
                    "IN_PROGRESS" | "QUEUED" | "REQUESTED" | "WAITING" | "PENDING" => pending += 1,
                    _ => pending += 1,
                }
            }
        }
    }

    let total = passing + failing + pending;
    let summary = format!("{}/{} passing", passing, total);

    let ci_status = if failing > 0 {
        super::types::CiStatus::Failing
    } else if pending > 0 {
        super::types::CiStatus::Pending
    } else {
        super::types::CiStatus::Passing
    };

    (ci_status, Some(summary))
}

/// Parse `reviews` array from gh output into review status.
fn parse_review_status(value: &serde_json::Value) -> (super::types::ReviewStatus, Option<String>) {
    let reviews = match value.get("reviews").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return (super::types::ReviewStatus::Unknown, None),
    };

    if reviews.is_empty() {
        return (super::types::ReviewStatus::Pending, None);
    }

    // Deduplicate reviews by author - only keep the latest review per author
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
        // Skip COMMENTED and DISMISSED - they don't represent a review decision
        if state == "APPROVED" || state == "CHANGES_REQUESTED" || state == "PENDING" {
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

    let review_status = if changes_requested > 0 {
        super::types::ReviewStatus::ChangesRequested
    } else if approved > 0 {
        super::types::ReviewStatus::Approved
    } else {
        super::types::ReviewStatus::Pending
    };

    (review_status, summary)
}

/// Read PR info for a session from the sidecar file.
///
/// Returns `None` if no PR info has been cached yet.
pub fn read_pr_info(session_id: &str) -> Option<super::types::PrInfo> {
    let config = Config::new();
    persistence::read_pr_info(&config.sessions_dir(), session_id)
}

/// Check if there's a merged PR for the given branch using gh CLI.
///
/// # Arguments
/// * `worktree_path` - Path to the git worktree (sets working directory for gh command)
/// * `branch` - Branch name to check (passed to gh pr view)
///
/// # Returns
/// * `true` - PR exists and is in MERGED state
/// * `false` - gh not available, PR doesn't exist, PR not merged, or any error occurred
///
/// # Note
/// This function treats all error cases as "not merged" for safety. Errors are logged
/// at debug/warn level for debugging purposes.
fn check_pr_merged(worktree_path: &std::path::Path, branch: &str) -> bool {
    debug!(
        event = "core.session.pr_check_started",
        branch = branch,
        worktree_path = %worktree_path.display()
    );

    let output = std::process::Command::new("gh")
        .current_dir(worktree_path)
        .args(["pr", "view", branch, "--json", "state", "-q", ".state"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let state = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_uppercase();
            let merged = state == "MERGED";
            debug!(
                event = "core.session.pr_check_completed",
                branch = branch,
                state = %state,
                merged = merged
            );
            merged
        }
        Ok(output) => {
            // gh CLI executed but returned error (PR not found, auth error, etc.)
            let stderr = String::from_utf8_lossy(&output.stderr);
            debug!(
                event = "core.session.pr_check_gh_error",
                branch = branch,
                exit_code = output.status.code(),
                stderr = %stderr.trim()
            );
            false
        }
        Err(e) => {
            // gh not found, permission denied, or other I/O error
            warn!(
                event = "core.session.pr_check_failed",
                branch = branch,
                worktree_path = %worktree_path.display(),
                error = %e,
                hint = "gh CLI may not be installed or accessible"
            );
            false
        }
    }
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
