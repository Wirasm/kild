pub use crate::forge::types::PrCheckResult;
use crate::git::types::WorktreeStatus;

/// Safety information for a destroy operation.
///
/// Contains git status information and PR check results to help users
/// make informed decisions before destroying a kild.
///
/// # Degraded State
///
/// Check `git_status.status_check_failed` to determine if the safety info
/// is degraded. When degraded, the fallback is conservative (assumes dirty)
/// and a warning message is included.
#[derive(Debug, Clone, Default)]
pub struct DestroySafetyInfo {
    /// Git worktree status (uncommitted changes, unpushed commits, etc.)
    pub git_status: WorktreeStatus,
    /// PR check result for the kild's branch.
    pub pr_status: PrCheckResult,
}

impl DestroySafetyInfo {
    /// Returns true if the destroy should be blocked (requires --force).
    ///
    /// Blocks on:
    /// - Uncommitted changes (cannot be recovered)
    /// - Status check failure with conservative fallback (user should verify manually)
    pub fn should_block(&self) -> bool {
        self.git_status.has_uncommitted_changes
    }

    /// Returns true if there are any warnings to show the user.
    pub fn has_warnings(&self) -> bool {
        self.git_status.has_uncommitted_changes
            || self.git_status.unpushed_commit_count > 0
            || !self.git_status.has_remote_branch
            || self.pr_status.not_found()
            || self.git_status.status_check_failed
    }

    /// Generate warning messages for display.
    ///
    /// Returns a list of human-readable warning messages in severity order:
    /// 1. Status check failures (critical - user should verify manually)
    /// 2. Uncommitted changes (blocking)
    /// 3. Unpushed commits (warning)
    /// 4. Never pushed (warning)
    /// 5. No PR found (advisory)
    pub fn warning_messages(&self) -> Vec<String> {
        let mut messages = Vec::new();

        // Status check failure (critical - shown first)
        if self.git_status.status_check_failed {
            messages
                .push("Git status check failed - could not verify uncommitted changes".to_string());
        }

        // Uncommitted changes (blocking)
        // Skip if status check failed (already showed critical message)
        if self.git_status.has_uncommitted_changes && !self.git_status.status_check_failed {
            let message = if let Some(details) = &self.git_status.uncommitted_details {
                let parts: Vec<String> = [
                    (details.staged_files > 0).then(|| format!("{} staged", details.staged_files)),
                    (details.modified_files > 0)
                        .then(|| format!("{} modified", details.modified_files)),
                    (details.untracked_files > 0)
                        .then(|| format!("{} untracked", details.untracked_files)),
                ]
                .into_iter()
                .flatten()
                .collect();
                format!("Uncommitted changes: {}", parts.join(", "))
            } else {
                "Uncommitted changes detected".to_string()
            };
            messages.push(message);
        }

        // Unpushed commits (warning only)
        if self.git_status.unpushed_commit_count > 0 {
            let count = self.git_status.unpushed_commit_count;
            let commit_word = if count == 1 { "commit" } else { "commits" };
            messages.push(format!("{} unpushed {} will be lost", count, commit_word));
        }

        // Never pushed (warning only) - skip if status check failed or has unpushed commits
        if !self.git_status.has_remote_branch
            && self.git_status.unpushed_commit_count == 0
            && !self.git_status.status_check_failed
        {
            messages.push("Branch has never been pushed".to_string());
        }

        // No PR found (advisory)
        if self.pr_status.not_found() {
            messages.push("No PR found for this branch".to_string());
        }

        messages
    }
}

/// Result of the `complete_session` operation, distinguishing between different outcomes.
#[derive(Debug, Clone, PartialEq)]
pub enum CompleteResult {
    /// PR was merged and remote branch was successfully deleted
    RemoteDeleted,
    /// PR was merged but remote branch deletion failed (logged as warning, non-fatal)
    RemoteDeleteFailed,
    /// PR was not merged, remote branch preserved for future merge
    PrNotMerged,
    /// Could not verify PR merge status (no forge, CLI error, no remote)
    PrCheckUnavailable,
}
