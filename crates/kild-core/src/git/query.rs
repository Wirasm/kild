//! High-level git query functions that encapsulate git2 usage.
//!
//! These functions provide the public API for git queries outside the `git/` module.
//! All git2 types stay contained here — callers only deal with standard Rust types.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use git2::{BranchType, Repository};
use tracing::{debug, warn};

use crate::git::errors::GitError;

/// Check if a path is inside a git repository.
///
/// Uses `Repository::discover` which traverses parent directories.
/// Returns `Ok(true)` if found, `Ok(false)` if not a git repo,
/// and `Err` for unexpected errors (e.g. permission denied).
pub fn is_git_repo(path: &Path) -> Result<bool, GitError> {
    match Repository::discover(path) {
        Ok(_) => Ok(true),
        Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(false),
        Err(e) => Err(GitError::Git2Error { source: e }),
    }
}

/// Verify that a path is inside a git repository.
///
/// Returns `Ok(())` if in a repo, `Err(GitError::NotInRepository)` otherwise.
pub fn ensure_in_repo(path: &Path) -> Result<(), GitError> {
    Repository::discover(path).map_err(|_| GitError::NotInRepository)?;
    Ok(())
}

/// Get the URL of the "origin" remote, if configured.
///
/// Returns `None` if the repo can't be opened, has no "origin" remote,
/// or the URL is not valid UTF-8.
pub fn get_origin_url(path: &Path) -> Option<String> {
    let repo = match Repository::open(path) {
        Ok(r) => r,
        Err(e) => {
            debug!(
                event = "core.git.query.repo_open_failed",
                path = %path.display(),
                error = %e
            );
            return None;
        }
    };

    let remote = match repo.find_remote("origin") {
        Ok(r) => r,
        Err(e) => {
            debug!(
                event = "core.git.query.no_origin_remote",
                path = %path.display(),
                error = %e
            );
            return None;
        }
    };

    match remote.url() {
        Some(url) => Some(url.to_string()),
        None => {
            debug!(
                event = "core.git.query.invalid_url",
                path = %path.display(),
                "Remote URL is not valid UTF-8"
            );
            None
        }
    }
}

/// Check if the repository at the given path has any remote configured.
///
/// Returns `false` on any error (graceful degradation).
pub fn has_any_remote(path: &Path) -> bool {
    let repo = match Repository::open(path) {
        Ok(r) => r,
        Err(e) => {
            debug!(
                event = "core.git.query.remote_check_repo_open_failed",
                path = %path.display(),
                error = %e
            );
            return false;
        }
    };
    match repo.remotes() {
        Ok(remotes) => !remotes.is_empty(),
        Err(e) => {
            debug!(
                event = "core.git.query.remote_check_failed",
                path = %path.display(),
                error = %e
            );
            false
        }
    }
}

/// Check if a worktree has uncommitted changes (simple dirty/clean check).
///
/// Returns `Some(true)` if dirty, `Some(false)` if clean,
/// `None` if the check failed (repo can't be opened, status error).
pub fn has_uncommitted_changes(path: &Path) -> Option<bool> {
    let repo = Repository::open(path).ok()?;
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true);
    opts.include_ignored(false);
    let statuses = repo.statuses(Some(&mut opts)).ok()?;
    Some(!statuses.is_empty())
}

/// List all local branch names in the repository discovered from `path`.
pub fn list_local_branch_names(path: &Path) -> Result<Vec<String>, GitError> {
    let repo = Repository::discover(path).map_err(|e| GitError::Git2Error { source: e })?;
    let branches = repo
        .branches(Some(BranchType::Local))
        .map_err(|e| GitError::Git2Error { source: e })?;

    let mut names = Vec::new();
    for item in branches {
        let (branch, _) = item.map_err(|e| GitError::Git2Error { source: e })?;
        if let Ok(Some(name)) = branch.name() {
            names.push(name.to_string());
        }
    }
    Ok(names)
}

/// Get the HEAD branch name of the repository discovered from `path`.
///
/// Returns `None` for detached HEAD or unborn branches.
pub fn head_branch_name(path: &Path) -> Result<Option<String>, GitError> {
    let repo = Repository::discover(path).map_err(|e| GitError::Git2Error { source: e })?;
    match repo.head() {
        Ok(head) => Ok(head.shorthand().map(|s| s.to_string())),
        Err(e) => {
            warn!(
                event = "core.git.query.head_read_failed",
                path = %path.display(),
                error = %e,
            );
            Ok(None)
        }
    }
}

/// Get the set of branch names currently checked out in worktrees (plus the main repo HEAD).
///
/// This discovers which branches are "active" — checked out in any worktree
/// or the main repository HEAD. Useful for orphan detection.
pub fn worktree_active_branches(path: &Path) -> Result<HashSet<String>, GitError> {
    let repo = Repository::discover(path).map_err(|e| GitError::Git2Error { source: e })?;
    let mut active = HashSet::new();

    // Collect branches checked out in worktrees
    let worktrees = repo
        .worktrees()
        .map_err(|e| GitError::Git2Error { source: e })?;

    for worktree_name in worktrees.iter().flatten() {
        match repo.find_worktree(worktree_name) {
            Ok(worktree) => match Repository::open(worktree.path()) {
                Ok(wt_repo) => match wt_repo.head() {
                    Ok(head) => {
                        if let Some(branch_name) = head.shorthand() {
                            active.insert(branch_name.to_string());
                        }
                    }
                    Err(e) => {
                        warn!(
                            event = "core.git.query.worktree_head_read_failed",
                            worktree_name = %worktree_name,
                            error = %e,
                        );
                    }
                },
                Err(e) => {
                    warn!(
                        event = "core.git.query.worktree_open_failed",
                        worktree_name = %worktree_name,
                        error = %e,
                    );
                }
            },
            Err(e) => {
                warn!(
                    event = "core.git.query.worktree_find_failed",
                    worktree_name = %worktree_name,
                    error = %e,
                );
            }
        }
    }

    // Add the main repository HEAD branch
    match repo.head() {
        Ok(head) => {
            if let Some(branch_name) = head.shorthand() {
                active.insert(branch_name.to_string());
            }
        }
        Err(e) => {
            warn!(
                event = "core.git.query.repo_head_read_failed",
                error = %e,
            );
        }
    }

    Ok(active)
}

/// A registered git worktree entry.
pub struct WorktreeEntry {
    pub name: String,
    pub path: PathBuf,
}

/// List all registered worktrees in the repository discovered from `path`.
pub fn list_worktree_entries(path: &Path) -> Result<Vec<WorktreeEntry>, GitError> {
    let repo = Repository::discover(path).map_err(|e| GitError::Git2Error { source: e })?;
    let worktrees = repo
        .worktrees()
        .map_err(|e| GitError::Git2Error { source: e })?;

    let mut entries = Vec::new();
    for worktree_name in worktrees.iter().flatten() {
        match repo.find_worktree(worktree_name) {
            Ok(worktree) => {
                entries.push(WorktreeEntry {
                    name: worktree_name.to_string(),
                    path: worktree.path().to_path_buf(),
                });
            }
            Err(e) => {
                warn!(
                    event = "core.git.query.worktree_find_failed",
                    worktree_name = %worktree_name,
                    error = %e,
                );
            }
        }
    }
    Ok(entries)
}

/// Check if a worktree path is a valid git repository with a resolvable HEAD.
///
/// Returns `true` if the worktree can be opened as a repo and HEAD has a target.
/// Returns `false` if the path doesn't exist, can't be opened, or HEAD is broken.
pub fn is_worktree_valid(worktree_path: &Path) -> bool {
    if !worktree_path.exists() {
        return false;
    }
    let Ok(repo) = Repository::open(worktree_path) else {
        return false;
    };
    match repo.head() {
        Ok(head) => head.target().is_some(),
        Err(_) => false,
    }
}

/// Delete a local branch by name.
///
/// Returns `Ok(true)` if the branch was deleted, `Ok(false)` if it was
/// already gone (not found or race condition). Errors on permission/lock failures.
pub fn delete_local_branch(path: &Path, name: &str) -> Result<bool, GitError> {
    let repo = Repository::discover(path).map_err(|e| GitError::Git2Error { source: e })?;

    let mut branch = match repo.find_branch(name, BranchType::Local) {
        Ok(b) => b,
        Err(e) if e.code() == git2::ErrorCode::NotFound => return Ok(false),
        Err(e) => return Err(GitError::Git2Error { source: e }),
    };

    match branch.delete() {
        Ok(()) => Ok(true),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("does not exist") {
                // Race condition: branch deleted between find and delete
                Ok(false)
            } else {
                Err(GitError::Git2Error { source: e })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_git_repo(path: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init failed");
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(path)
            .output()
            .unwrap();
    }

    fn create_initial_commit(path: &Path) {
        std::fs::write(path.join("test.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(path)
            .output()
            .unwrap();
    }

    #[test]
    fn test_is_git_repo_valid() {
        let temp = TempDir::new().unwrap();
        init_git_repo(temp.path());
        assert!(is_git_repo(temp.path()).unwrap());
    }

    #[test]
    fn test_is_git_repo_not_repo() {
        let temp = TempDir::new().unwrap();
        assert!(!is_git_repo(temp.path()).unwrap());
    }

    #[test]
    fn test_ensure_in_repo_valid() {
        let temp = TempDir::new().unwrap();
        init_git_repo(temp.path());
        assert!(ensure_in_repo(temp.path()).is_ok());
    }

    #[test]
    fn test_ensure_in_repo_not_repo() {
        let temp = TempDir::new().unwrap();
        assert!(ensure_in_repo(temp.path()).is_err());
    }

    #[test]
    fn test_get_origin_url_no_remote() {
        let temp = TempDir::new().unwrap();
        init_git_repo(temp.path());
        assert!(get_origin_url(temp.path()).is_none());
    }

    #[test]
    fn test_get_origin_url_nonexistent_path() {
        assert!(get_origin_url(Path::new("/nonexistent/path")).is_none());
    }

    #[test]
    fn test_has_any_remote_no_remotes() {
        let temp = TempDir::new().unwrap();
        init_git_repo(temp.path());
        assert!(!has_any_remote(temp.path()));
    }

    #[test]
    fn test_has_any_remote_nonexistent_path() {
        assert!(!has_any_remote(Path::new("/nonexistent/path")));
    }

    #[test]
    fn test_has_uncommitted_changes_clean() {
        let temp = TempDir::new().unwrap();
        init_git_repo(temp.path());
        create_initial_commit(temp.path());
        assert_eq!(has_uncommitted_changes(temp.path()), Some(false));
    }

    #[test]
    fn test_has_uncommitted_changes_dirty() {
        let temp = TempDir::new().unwrap();
        init_git_repo(temp.path());
        std::fs::write(temp.path().join("untracked.txt"), "dirty").unwrap();
        assert_eq!(has_uncommitted_changes(temp.path()), Some(true));
    }

    #[test]
    fn test_has_uncommitted_changes_not_repo() {
        let temp = TempDir::new().unwrap();
        assert_eq!(has_uncommitted_changes(temp.path()), None);
    }

    #[test]
    fn test_list_local_branch_names() {
        let temp = TempDir::new().unwrap();
        init_git_repo(temp.path());
        create_initial_commit(temp.path());

        Command::new("git")
            .args(["branch", "feature-a"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let branches = list_local_branch_names(temp.path()).unwrap();
        assert!(branches.len() >= 2); // default branch + feature-a
        assert!(branches.contains(&"feature-a".to_string()));
    }

    #[test]
    fn test_head_branch_name() {
        let temp = TempDir::new().unwrap();
        init_git_repo(temp.path());
        create_initial_commit(temp.path());

        let name = head_branch_name(temp.path()).unwrap();
        assert!(name.is_some());
    }

    #[test]
    fn test_worktree_active_branches() {
        let temp = TempDir::new().unwrap();
        init_git_repo(temp.path());
        create_initial_commit(temp.path());

        let active = worktree_active_branches(temp.path()).unwrap();
        assert!(!active.is_empty()); // At least the main branch
    }

    #[test]
    fn test_list_worktree_entries_empty() {
        let temp = TempDir::new().unwrap();
        init_git_repo(temp.path());
        create_initial_commit(temp.path());

        let entries = list_worktree_entries(temp.path()).unwrap();
        assert!(entries.is_empty()); // No worktrees in a fresh repo
    }

    #[test]
    fn test_is_worktree_valid_nonexistent() {
        assert!(!is_worktree_valid(Path::new("/nonexistent/path")));
    }

    #[test]
    fn test_is_worktree_valid_valid_repo() {
        let temp = TempDir::new().unwrap();
        init_git_repo(temp.path());
        create_initial_commit(temp.path());
        assert!(is_worktree_valid(temp.path()));
    }

    #[test]
    fn test_delete_local_branch_not_found() {
        let temp = TempDir::new().unwrap();
        init_git_repo(temp.path());
        create_initial_commit(temp.path());

        let result = delete_local_branch(temp.path(), "nonexistent-branch").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_delete_local_branch_success() {
        let temp = TempDir::new().unwrap();
        init_git_repo(temp.path());
        create_initial_commit(temp.path());

        Command::new("git")
            .args(["branch", "to-delete"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let result = delete_local_branch(temp.path(), "to-delete").unwrap();
        assert!(result);

        // Verify it's gone
        let branches = list_local_branch_names(temp.path()).unwrap();
        assert!(!branches.contains(&"to-delete".to_string()));
    }
}
