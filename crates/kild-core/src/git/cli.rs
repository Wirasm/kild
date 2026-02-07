//! Centralized git CLI wrappers for auth-requiring operations.
//!
//! Operations like `fetch`, `rebase`, and `push --delete` require authentication.
//! The git CLI inherits the user's SSH agent and credential helpers automatically,
//! while git2 requires explicit credential callback setup.
//!
//! Each function validates arguments, logs structured events, and maps errors consistently.

use std::path::Path;

use tracing::{debug, error, info, warn};

use super::errors::GitError;
use super::operations::validate_git_arg;

/// Fetch a specific branch from a remote.
///
/// Uses `git fetch` CLI to inherit the user's SSH agent and credential helpers
/// with zero auth code in kild.
pub fn fetch(dir: &Path, remote: &str, branch: &str) -> Result<(), GitError> {
    validate_git_arg(remote, "remote name")?;
    validate_git_arg(branch, "branch name")?;

    info!(
        event = "core.git.fetch_started",
        remote = remote,
        branch = branch,
        path = %dir.display()
    );

    let output = std::process::Command::new("git")
        .current_dir(dir)
        .args(["fetch", remote, branch])
        .output()
        .map_err(|e| GitError::FetchFailed {
            remote: remote.to_string(),
            message: format!("Failed to execute git: {}", e),
        })?;

    if output.status.success() {
        info!(
            event = "core.git.fetch_completed",
            remote = remote,
            branch = branch
        );
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(
            event = "core.git.fetch_failed",
            remote = remote,
            branch = branch,
            stderr = %stderr.trim()
        );
        Err(GitError::FetchFailed {
            remote: remote.to_string(),
            message: stderr.trim().to_string(),
        })
    }
}

/// Delete a branch from a remote.
///
/// Uses `git push --delete` CLI because push operations require authentication
/// that the CLI inherits from the user's credential helpers.
///
/// Treats "branch already deleted" as success (idempotent).
pub fn delete_remote_branch(dir: &Path, remote: &str, branch: &str) -> Result<(), GitError> {
    validate_git_arg(remote, "remote name")?;
    validate_git_arg(branch, "branch name")?;

    info!(
        event = "core.git.delete_remote_branch_started",
        remote = remote,
        branch = branch,
        path = %dir.display()
    );

    let output = std::process::Command::new("git")
        .current_dir(dir)
        .args(["push", remote, "--delete", branch])
        .output()
        .map_err(|e| GitError::RemoteBranchDeleteFailed {
            branch: branch.to_string(),
            message: format!("Failed to execute git in {}: {}", dir.display(), e),
        })?;

    if output.status.success() {
        info!(
            event = "core.git.delete_remote_branch_completed",
            remote = remote,
            branch = branch
        );
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);

    if is_already_deleted_error(&stderr) {
        info!(
            event = "core.git.delete_remote_branch_already_deleted",
            remote = remote,
            branch = branch
        );
        Ok(())
    } else {
        debug!(
            event = "core.git.delete_remote_branch_failed",
            remote = remote,
            branch = branch,
            stderr = %stderr.trim()
        );
        Err(GitError::RemoteBranchDeleteFailed {
            branch: branch.to_string(),
            message: stderr.trim().to_string(),
        })
    }
}

/// Check if a `git push --delete` stderr indicates the branch was already deleted.
///
/// Matches common "branch doesn't exist" patterns across git versions.
fn is_already_deleted_error(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    [
        "remote ref does not exist",
        "unable to delete",
        "does not exist",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern))
}

/// Rebase the current branch onto a base branch.
///
/// Uses `git rebase` CLI because rebase may trigger fetches or other operations
/// that require the user's credential helpers.
///
/// On conflict, auto-aborts the rebase to leave the worktree clean,
/// then returns `GitError::RebaseConflict` so the user can resolve manually.
pub fn rebase(dir: &Path, base_branch: &str) -> Result<(), GitError> {
    validate_git_arg(base_branch, "base branch")?;

    info!(
        event = "core.git.rebase_started",
        base = base_branch,
        path = %dir.display()
    );

    let output = std::process::Command::new("git")
        .current_dir(dir)
        .args(["rebase", base_branch])
        .output()
        .map_err(|e| GitError::OperationFailed {
            message: format!("Failed to execute git rebase: {}", e),
        })?;

    if output.status.success() {
        info!(
            event = "core.git.rebase_completed",
            base = base_branch,
            path = %dir.display()
        );
        return Ok(());
    }

    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Detect conflicts: exit code 1 with conflict markers in stderr
    let has_conflict_marker = stderr.contains("CONFLICT")
        || stderr.contains("failed to merge")
        || stderr.contains("could not apply");
    let is_conflict = code == 1 && has_conflict_marker;

    if is_conflict {
        // Auto-abort to leave worktree clean
        let abort_result = std::process::Command::new("git")
            .current_dir(dir)
            .args(["rebase", "--abort"])
            .output();

        // Handle abort result: log failure but continue to return RebaseConflict
        if let Err(e) = abort_result {
            error!(
                event = "core.git.rebase_abort_failed",
                base = base_branch,
                path = %dir.display(),
                error = %e
            );
            return Err(GitError::RebaseAbortFailed {
                base_branch: base_branch.to_string(),
                worktree_path: dir.to_path_buf(),
                message: e.to_string(),
            });
        }

        let abort_output = abort_result.unwrap();
        if !abort_output.status.success() {
            let abort_stderr = String::from_utf8_lossy(&abort_output.stderr);
            error!(
                event = "core.git.rebase_abort_failed",
                base = base_branch,
                path = %dir.display(),
                stderr = %abort_stderr.trim()
            );
            return Err(GitError::RebaseAbortFailed {
                base_branch: base_branch.to_string(),
                worktree_path: dir.to_path_buf(),
                message: abort_stderr.trim().to_string(),
            });
        }

        info!(
            event = "core.git.rebase_abort_completed",
            base = base_branch,
            path = %dir.display()
        );

        warn!(
            event = "core.git.rebase_conflicts",
            base = base_branch,
            path = %dir.display()
        );
        return Err(GitError::RebaseConflict {
            base_branch: base_branch.to_string(),
            worktree_path: dir.to_path_buf(),
        });
    }

    // Non-conflict failure
    error!(
        event = "core.git.rebase_failed",
        base = base_branch,
        path = %dir.display(),
        code = code,
        stderr = %stderr.trim()
    );
    Err(GitError::OperationFailed {
        message: format!("git rebase failed (exit {}): {}", code, stderr.trim()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_git_arg_rejects_dash_prefix() {
        let result = validate_git_arg("--evil", "test");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("must not start with '-'"));
    }

    #[test]
    fn test_validate_git_arg_rejects_control_chars() {
        let result = validate_git_arg("hello\x00world", "test");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("control characters"));
    }

    #[test]
    fn test_validate_git_arg_rejects_double_colon() {
        let result = validate_git_arg("refs::heads", "test");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("'::'"));
    }

    #[test]
    fn test_validate_git_arg_accepts_valid_values() {
        assert!(validate_git_arg("origin", "remote").is_ok());
        assert!(validate_git_arg("main", "branch").is_ok());
        assert!(validate_git_arg("kild/feature-auth", "branch").is_ok());
        assert!(validate_git_arg("refs/heads/main", "refspec").is_ok());
    }

    #[test]
    fn test_is_already_deleted_error_matches_benign_patterns() {
        // Standard "remote ref does not exist" message
        assert!(is_already_deleted_error(
            "error: unable to delete 'origin/kild/test': remote ref does not exist"
        ));
        // Lowercase variant
        assert!(is_already_deleted_error(
            "fatal: branch 'kild/test' does not exist"
        ));
        // "unable to delete" variant
        assert!(is_already_deleted_error("error: unable to delete 'foo'"));
    }

    #[test]
    fn test_is_already_deleted_error_rejects_real_failures() {
        assert!(!is_already_deleted_error("fatal: Authentication failed"));
        assert!(!is_already_deleted_error(
            "fatal: Could not read from remote repository"
        ));
        assert!(!is_already_deleted_error(""));
    }
}
