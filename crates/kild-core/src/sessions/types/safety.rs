use crate::forge::types::{MergeStrategy, PrCheckResult};
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
pub struct DestroySafety {
    /// Git worktree status (uncommitted changes, unpushed commits, etc.)
    pub git_status: WorktreeStatus,
    /// PR check result for the kild's branch.
    pub pr_status: PrCheckResult,
}

impl DestroySafety {
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
                let mut parts = Vec::new();
                if details.staged_files > 0 {
                    parts.push(format!("{} staged", details.staged_files));
                }
                if details.modified_files > 0 {
                    parts.push(format!("{} modified", details.modified_files));
                }
                if details.untracked_files > 0 {
                    parts.push(format!("{} untracked", details.untracked_files));
                }
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

/// Request options for `complete_session`.
#[derive(Debug, Clone)]
pub struct CompleteRequest {
    /// Branch name of the kild to complete.
    pub name: String,
    /// Merge strategy (squash, merge, rebase).
    pub merge_strategy: MergeStrategy,
    /// Skip merging — just clean up (old behavior, requires PR already merged).
    pub no_merge: bool,
    /// Force through safety checks (uncommitted changes, CI failures, pending reviews).
    pub force: bool,
    /// Show what would happen without doing it.
    pub dry_run: bool,
    /// Skip CI status check before merging.
    pub skip_ci: bool,
}

impl CompleteRequest {
    /// Create a new request with defaults (squash, merge enabled, no force/dry-run).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            merge_strategy: MergeStrategy::default(),
            no_merge: false,
            force: false,
            dry_run: false,
            skip_ci: false,
        }
    }
}

/// Result of the `complete_session` operation, distinguishing between different outcomes.
#[derive(Debug, Clone, PartialEq)]
pub enum CompleteResult {
    /// PR was merged by this command, remote branch deleted, session destroyed.
    Merged {
        /// The merge strategy used.
        strategy: MergeStrategy,
        /// Whether remote branch was deleted (false if deletion failed, non-fatal).
        remote_deleted: bool,
    },
    /// PR was already merged (--no-merge or detected as merged). Cleaned up.
    AlreadyMerged {
        /// Whether remote branch was deleted.
        remote_deleted: bool,
    },
    /// --no-merge mode: PR not merged, session destroyed, remote branch preserved.
    CleanupOnly,
    /// --dry-run: shows what would happen.
    DryRun {
        /// Steps that would be performed.
        steps: Vec<String>,
    },
}
