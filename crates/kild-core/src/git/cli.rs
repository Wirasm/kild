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
use super::validation::validate_git_arg;

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

        let abort_output = match abort_result {
            Ok(output) => output,
            Err(e) => {
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
        };
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

/// Execute `git diff` in a worktree, inheriting stdio for terminal output.
///
/// Uses `.status()` (not `.output()`) so diff output appears directly in the
/// user's terminal with proper paging.
///
/// # Exit Code Semantics
/// - 0: no differences
/// - 1: differences found (NOT an error)
/// - 128+: git error
pub fn show_diff(worktree_path: &Path, staged: bool) -> Result<(), GitError> {
    info!(
        event = "core.git.diff_started",
        path = %worktree_path.display(),
        staged = staged
    );

    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(worktree_path);
    cmd.arg("diff");
    if staged {
        cmd.arg("--staged");
    }

    let status = cmd.status().map_err(|e| GitError::DiffFailed {
        message: format!("Failed to execute git: {}", e),
    })?;

    // git diff: 0 = no diff, 1 = diff found (both OK), 128+ = error
    if let Some(code) = status.code()
        && code >= 128
    {
        warn!(
            event = "core.git.diff_failed",
            exit_code = code,
            path = %worktree_path.display()
        );
        return Err(GitError::DiffFailed {
            message: format!("git diff failed with exit code {}", code),
        });
    }

    info!(
        event = "core.git.diff_completed",
        path = %worktree_path.display(),
        staged = staged,
        exit_code = status.code()
    );
    Ok(())
}

/// Get recent commits from a worktree as a formatted string.
///
/// Executes `git log --oneline -n <count>` and returns the output.
pub fn get_commits(worktree_path: &Path, count: usize) -> Result<String, GitError> {
    info!(
        event = "core.git.commits_started",
        path = %worktree_path.display(),
        count = count
    );

    let output = std::process::Command::new("git")
        .current_dir(worktree_path)
        .args(["log", "--oneline", "-n", &count.to_string()])
        .output()
        .map_err(|e| GitError::LogFailed {
            message: format!("Failed to execute git: {}", e),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(
            event = "core.git.commits_failed",
            path = %worktree_path.display(),
            stderr = %stderr.trim()
        );
        return Err(GitError::LogFailed {
            message: format!("git log failed: {}", stderr.trim()),
        });
    }

    let commits = String::from_utf8_lossy(&output.stdout).to_string();
    info!(
        event = "core.git.commits_completed",
        path = %worktree_path.display(),
        count = count
    );
    Ok(commits)
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

    // --- show_diff tests ---

    use std::fs;
    use std::process::Command as ProcessCommand;
    use tempfile::TempDir;

    fn init_git_repo(dir: &Path) {
        ProcessCommand::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .expect("Failed to init git repo");
        ProcessCommand::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .expect("Failed to configure git email");
        ProcessCommand::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir)
            .output()
            .expect("Failed to configure git name");
    }

    #[test]
    fn test_show_diff_clean_repo() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        fs::write(dir.path().join("file.txt"), "hello").unwrap();
        ProcessCommand::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        ProcessCommand::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(show_diff(dir.path(), false).is_ok());
    }

    #[test]
    fn test_show_diff_staged() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        fs::write(dir.path().join("file.txt"), "hello").unwrap();
        ProcessCommand::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        ProcessCommand::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("file.txt"), "changed").unwrap();
        ProcessCommand::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(show_diff(dir.path(), true).is_ok());
    }

    #[test]
    fn test_show_diff_invalid_path() {
        let result = show_diff(Path::new("/nonexistent/path"), false);
        assert!(result.is_err());
    }

    // --- get_commits tests ---

    #[test]
    fn test_get_commits_with_history() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        fs::write(dir.path().join("file.txt"), "hello").unwrap();
        ProcessCommand::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        ProcessCommand::new("git")
            .args(["commit", "-m", "first commit"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let result = get_commits(dir.path(), 10).unwrap();
        assert!(result.contains("first commit"));
    }

    #[test]
    fn test_get_commits_empty_repo() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        // No commits yet — git log should fail
        let result = get_commits(dir.path(), 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_commits_invalid_path() {
        let result = get_commits(Path::new("/nonexistent/path"), 10);
        assert!(result.is_err());
    }
}
