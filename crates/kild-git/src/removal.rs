use git2::{BranchType, Repository};
use std::path::Path;
use tracing::{debug, error, info, warn};

use crate::{errors::GitError, naming};

/// Safety check: refuse to delete a path that is a main git repository checkout.
///
/// A main checkout has a `.git` **directory** at its root, while worktrees have
/// a `.git` **file** (pointing back to the main repo's `.git/worktrees/<name>/`).
/// This prevents catastrophic deletion of the project root via `remove_dir_all`.
fn assert_not_main_repo(worktree_path: &Path) -> Result<(), GitError> {
    let dot_git = worktree_path.join(".git");
    if dot_git.is_dir() {
        error!(
            event = "core.git.worktree.remove_blocked_main_repo",
            path = %worktree_path.display(),
            "Refusing to remove path that is a main git repository checkout"
        );
        return Err(GitError::WorktreeRemovalFailed {
            path: worktree_path.display().to_string(),
            message: "Path is a main git repository, not a worktree. \
                      This is a safety guard to prevent deleting project roots."
                .to_string(),
        });
    }
    Ok(())
}

pub fn remove_worktree(
    project: &crate::types::GitProjectState,
    worktree_path: &Path,
) -> Result<(), GitError> {
    info!(
        event = "core.git.worktree.remove_started",
        project_id = project.id,
        worktree_path = %worktree_path.display()
    );

    let repo = Repository::open(&project.path).map_err(|e| GitError::Git2Error { source: e })?;

    if let Some(worktree) = find_worktree_by_path(&repo, worktree_path) {
        // Remove worktree
        worktree
            .prune(None)
            .map_err(|e| GitError::Git2Error { source: e })?;

        // Remove directory if it still exists
        if worktree_path.exists() {
            std::fs::remove_dir_all(worktree_path).map_err(|e| GitError::IoError { source: e })?;
        }

        info!(
            event = "core.git.worktree.remove_completed",
            project_id = project.id,
            worktree_path = %worktree_path.display()
        );
    } else {
        error!(
            event = "core.git.worktree.remove_failed",
            project_id = project.id,
            worktree_path = %worktree_path.display(),
            error = "worktree not found"
        );
        return Err(GitError::WorktreeNotFound {
            path: worktree_path.display().to_string(),
        });
    }

    Ok(())
}

/// Find the main repository for a given worktree path.
///
/// Attempts two strategies:
/// 1. Open worktree as repo and navigate to main repo via parent directories
/// 2. Search up directory tree for .git directory
///
/// Returns error if repository cannot be found (required for normal removal).
fn find_main_repository(worktree_path: &Path) -> Result<Repository, GitError> {
    // Strategy 1: Open worktree and find main repo
    if let Ok(repo) = Repository::open(worktree_path) {
        if let Some(main_repo_path) = repo.path().parent().and_then(|p| p.parent()) {
            match Repository::open(main_repo_path) {
                Ok(main_repo) => return Ok(main_repo),
                Err(e) => {
                    debug!(
                        event = "core.git.worktree.main_repo_open_via_worktree_failed",
                        path = %main_repo_path.display(),
                        error = %e,
                    );
                }
            }
        }
        // If we can't find main repo, use the worktree repo itself
        return Ok(repo);
    }

    // Strategy 2: Search up directory tree for .git
    let mut current_path = worktree_path;
    while let Some(parent) = current_path.parent() {
        let git_dir = parent.join(".git");
        if git_dir.exists() && git_dir.is_dir() {
            return Repository::open(parent).map_err(|e| GitError::OperationFailed {
                message: format!(
                    "Found repository at {} but failed to open: {}",
                    parent.display(),
                    e
                ),
            });
        }
        current_path = parent;
    }

    Err(GitError::OperationFailed {
        message: format!(
            "Could not find main repository for worktree at {}",
            worktree_path.display()
        ),
    })
}

/// Try to discover the main repository for a worktree that needs force removal.
///
/// Similar to `find_main_repository` but returns `None` on failure instead of
/// erroring, allowing force removal to proceed with just directory deletion.
fn find_repository_for_force_removal(worktree_path: &Path) -> Option<Repository> {
    match find_main_repository(worktree_path) {
        Ok(repo) => Some(repo),
        Err(e) => {
            warn!(
                event = "core.git.worktree.remove_force_repo_discovery_failed",
                path = %worktree_path.display(),
                error = %e,
                message = "Will proceed with directory deletion only"
            );
            None
        }
    }
}

/// Check if a branch name matches kild's naming pattern.
///
/// Accepts both current (kild/) and legacy (kild_) prefixes.
fn is_kild_managed_branch(branch_name: &str) -> bool {
    branch_name.starts_with(naming::KILD_BRANCH_PREFIX) || branch_name.starts_with("kild_")
}

/// Find a worktree by its path in the repository.
///
/// Returns None if worktree is not found.
fn find_worktree_by_path(repo: &Repository, worktree_path: &Path) -> Option<git2::Worktree> {
    let worktrees = repo.worktrees().ok()?;

    for worktree_name in worktrees.iter().flatten() {
        if let Ok(worktree) = repo.find_worktree(worktree_name)
            && worktree.path() == worktree_path
        {
            return Some(worktree);
        }
    }

    None
}

/// Delete a branch if it's managed by kild.
///
/// Handles race conditions and missing branches gracefully with appropriate logging.
fn delete_kild_branch_if_managed(repo: &Repository, branch_name: &str, worktree_path: &Path) {
    if !is_kild_managed_branch(branch_name) {
        return;
    }

    let mut branch = match repo.find_branch(branch_name, BranchType::Local) {
        Ok(branch) => branch,
        Err(e) => {
            debug!(
                event = "core.git.branch.not_found_for_cleanup",
                branch = branch_name,
                worktree_path = %worktree_path.display(),
                error = %e,
                message = "Branch already deleted or never existed"
            );
            return;
        }
    };

    match branch.delete() {
        Ok(()) => {
            info!(
                event = "core.git.branch.delete_completed",
                branch = branch_name,
                worktree_path = %worktree_path.display()
            );
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") || error_msg.contains("does not exist") {
                debug!(
                    event = "core.git.branch.delete_race_condition",
                    branch = branch_name,
                    worktree_path = %worktree_path.display(),
                    message = "Branch was deleted by another process"
                );
            } else {
                warn!(
                    event = "core.git.branch.delete_failed",
                    branch = branch_name,
                    worktree_path = %worktree_path.display(),
                    error = %e,
                    error_type = "concurrent_operation_or_permission"
                );
            }
        }
    }
}

/// Find the root path of the main repository for a given worktree.
///
/// Returns the working directory of the main repository, suitable for
/// passing to `delete_branch_if_exists`. Must be called while the
/// worktree directory still exists on disk.
pub fn find_main_repo_root(worktree_path: &Path) -> Option<std::path::PathBuf> {
    find_main_repository(worktree_path)
        .ok()
        .and_then(|repo| repo.workdir().map(|p| p.to_path_buf()))
}

/// Delete a local git branch if it exists.
///
/// `repo_root` is the path to the main repository (not the worktree).
/// Best-effort: logs failures but never returns an error, matching the
/// non-fatal pattern used throughout destroy operations.
pub fn delete_branch_if_exists(repo_root: &Path, branch_name: &str) {
    let repo = match Repository::open(repo_root) {
        Ok(repo) => repo,
        Err(e) => {
            warn!(
                event = "core.git.branch.delete_repo_not_found",
                branch = branch_name,
                repo_root = %repo_root.display(),
                error = %e,
            );
            return;
        }
    };

    delete_kild_branch_if_managed(&repo, branch_name, repo_root);
}

pub fn remove_worktree_by_path(worktree_path: &Path) -> Result<(), GitError> {
    assert_not_main_repo(worktree_path)?;

    info!(
        event = "core.git.worktree.remove_by_path_started",
        worktree_path = %worktree_path.display()
    );

    // Find the main repository for this worktree
    let repo = find_main_repository(worktree_path)?;

    if let Some(worktree) = find_worktree_by_path(&repo, worktree_path) {
        // Get the branch name before removing the worktree
        let branch_name = if let Ok(worktree_repo) = Repository::open(worktree.path()) {
            if let Ok(head) = worktree_repo.head() {
                head.shorthand().map(|s| s.to_string())
            } else {
                None
            }
        } else {
            None
        };

        // Remove worktree with force flag
        let mut prune_options = git2::WorktreePruneOptions::new();
        prune_options.valid(true); // Allow pruning valid worktrees

        worktree
            .prune(Some(&mut prune_options))
            .map_err(|e| GitError::Git2Error { source: e })?;

        // Remove directory if it still exists
        if worktree_path.exists() {
            std::fs::remove_dir_all(worktree_path).map_err(|e| GitError::IoError { source: e })?;
        }

        // Delete associated branch if it's a kild-managed branch
        if let Some(ref branch_name) = branch_name {
            delete_kild_branch_if_managed(&repo, branch_name, worktree_path);
        }

        info!(
            event = "core.git.worktree.remove_by_path_completed",
            worktree_path = %worktree_path.display()
        );
    } else {
        // Worktree not found in git registry - state inconsistency detected
        warn!(
            event = "core.git.worktree.state_inconsistency",
            worktree_path = %worktree_path.display(),
            message = "Worktree directory exists but not registered in git - cleaning up orphaned directory"
        );

        // If worktree not found in git, just remove the directory
        if worktree_path.exists() {
            std::fs::remove_dir_all(worktree_path).map_err(|e| GitError::IoError { source: e })?;
            info!(
                event = "core.git.worktree.remove_by_path_directory_only",
                worktree_path = %worktree_path.display()
            );
        }
    }

    Ok(())
}

/// Force removes a git worktree, bypassing uncommitted changes check.
///
/// Use with caution - uncommitted work will be lost.
/// This first tries to prune from git, then force-deletes the directory.
pub fn remove_worktree_force(worktree_path: &Path) -> Result<(), GitError> {
    assert_not_main_repo(worktree_path)?;

    info!(
        event = "core.git.worktree.remove_force_started",
        path = %worktree_path.display()
    );

    let repo = find_repository_for_force_removal(worktree_path);

    // Try to prune from git if we found the repo
    if let Some(repo) = repo
        && let Some(worktree) = find_worktree_by_path(&repo, worktree_path)
    {
        let mut prune_options = git2::WorktreePruneOptions::new();
        prune_options.valid(true);
        prune_options.working_tree(true);

        if let Err(e) = worktree.prune(Some(&mut prune_options)) {
            warn!(
                event = "core.git.worktree.prune_failed_force_continue",
                path = %worktree_path.display(),
                error = %e
            );
        }
    }

    // Force delete the directory regardless of git status
    if worktree_path.exists() {
        std::fs::remove_dir_all(worktree_path).map_err(|e| GitError::WorktreeRemovalFailed {
            path: worktree_path.display().to_string(),
            message: e.to_string(),
        })?;
    }

    info!(
        event = "core.git.worktree.remove_force_completed",
        path = %worktree_path.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::WorktreeAddOptions;
    use std::path::PathBuf;

    /// Test helper: Create a temporary directory with unique name.
    fn create_temp_test_dir(prefix: &str) -> PathBuf {
        let temp_dir = std::env::temp_dir().join(format!("{}_{}", prefix, std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
        temp_dir
    }

    /// Test helper: Initialize a git repository with an initial commit.
    fn init_test_repo(path: &Path) {
        let repo = Repository::init(path).expect("Failed to init git repo");
        let sig = repo
            .signature()
            .unwrap_or_else(|_| git2::Signature::now("Test", "test@test.com").unwrap());
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .expect("Failed to create initial commit");
    }

    #[test]
    fn test_remove_worktree_force_nonexistent_is_ok() {
        // Force removal should not error if directory doesn't exist (idempotent)
        let nonexistent = std::path::Path::new("/tmp/shards-test-nonexistent-kild_12345");
        // Make sure it doesn't exist
        let _ = std::fs::remove_dir_all(nonexistent);

        let result = remove_worktree_force(nonexistent);
        assert!(result.is_ok());
    }

    #[test]
    fn test_delete_branch_if_exists_cleans_up_kild_branch() {
        let repo_dir = create_temp_test_dir("kild_test_delete_branch_repo");
        let worktree_base = create_temp_test_dir("kild_test_delete_branch_wt");
        init_test_repo(&repo_dir);

        let repo = Repository::open(&repo_dir).unwrap();
        let head = repo.head().unwrap();
        let head_commit = head.peel_to_commit().unwrap();

        // Create a kild/feature branch
        repo.branch("kild/feature", &head_commit, false).unwrap();

        // Create a worktree using the branch
        let worktree_path = worktree_base.join("kild-feature");
        let branch_ref = repo
            .find_branch("kild/feature", git2::BranchType::Local)
            .unwrap()
            .into_reference();
        let mut opts = WorktreeAddOptions::new();
        opts.reference(Some(&branch_ref));
        repo.worktree("kild-feature", &worktree_path, Some(&opts))
            .unwrap();

        // Verify the branch exists
        assert!(
            repo.find_branch("kild/feature", git2::BranchType::Local)
                .is_ok()
        );

        // Canonicalize the path (macOS /tmp -> /private/tmp)
        let canonical_worktree_path = worktree_path.canonicalize().unwrap();

        // Force remove the worktree (simulating kild destroy --force)
        remove_worktree_force(&canonical_worktree_path).unwrap();

        // Delete branch using the main repo root (not the worktree path)
        let canonical_repo_dir = repo_dir.canonicalize().unwrap();
        delete_branch_if_exists(&canonical_repo_dir, "kild/feature");

        // Reopen repo to see branch changes
        let repo = Repository::open(&repo_dir).unwrap();
        assert!(
            repo.find_branch("kild/feature", git2::BranchType::Local)
                .is_err(),
            "kild/feature branch should be deleted after delete_branch_if_exists"
        );

        let _ = std::fs::remove_dir_all(&repo_dir);
        let _ = std::fs::remove_dir_all(&worktree_base);
    }

    #[test]
    fn test_delete_branch_if_exists_noop_for_nonexistent_branch() {
        let repo_dir = create_temp_test_dir("kild_test_delete_noop_repo");
        init_test_repo(&repo_dir);

        // Calling with a branch that doesn't exist should not panic
        delete_branch_if_exists(&repo_dir, "kild/no-such-branch");

        let _ = std::fs::remove_dir_all(&repo_dir);
    }

    #[test]
    fn test_remove_worktree_cleans_up_legacy_kild_prefix() {
        let repo_dir = create_temp_test_dir("kild_test_legacy_repo");
        let worktree_base = create_temp_test_dir("kild_test_legacy_wt");
        init_test_repo(&repo_dir);

        let repo = Repository::open(&repo_dir).unwrap();
        let head = repo.head().unwrap();
        let head_commit = head.peel_to_commit().unwrap();

        // Create a legacy kild_feature branch
        repo.branch("kild_feature", &head_commit, false).unwrap();

        // Create a worktree using the legacy branch, outside the main repo
        let worktree_path = worktree_base.join("kild_feature");
        let branch_ref = repo
            .find_branch("kild_feature", git2::BranchType::Local)
            .unwrap()
            .into_reference();
        let mut opts = WorktreeAddOptions::new();
        opts.reference(Some(&branch_ref));
        repo.worktree("kild_feature", &worktree_path, Some(&opts))
            .unwrap();

        // Verify the worktree and branch exist
        assert!(worktree_path.exists());
        assert!(
            repo.find_branch("kild_feature", git2::BranchType::Local)
                .is_ok()
        );

        // Canonicalize the path so it matches git2's internal path storage.
        // On macOS, /tmp symlinks to /private/tmp; git2 stores canonicalized paths.
        let canonical_worktree_path = worktree_path.canonicalize().unwrap();

        // Remove via remove_worktree_by_path
        let result = remove_worktree_by_path(&canonical_worktree_path);
        assert!(result.is_ok(), "remove_worktree_by_path should succeed");

        // Reopen repo to see branch changes
        let repo = Repository::open(&repo_dir).unwrap();

        // Legacy kild_feature branch should be cleaned up
        assert!(
            repo.find_branch("kild_feature", git2::BranchType::Local)
                .is_err(),
            "legacy kild_feature branch should be deleted during cleanup"
        );

        let _ = std::fs::remove_dir_all(&repo_dir);
        let _ = std::fs::remove_dir_all(&worktree_base);
    }

    #[test]
    fn test_assert_not_main_repo_blocks_main_checkout() {
        let dir = create_temp_test_dir("kild_test_assert_main_repo");
        init_test_repo(&dir);

        // dir has .git/ directory → should be blocked
        let result = assert_not_main_repo(&dir);
        assert!(result.is_err(), "Should refuse to remove main repo root");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("main git repository"),
            "Error should explain it's a main repo"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_assert_not_main_repo_allows_worktree() {
        let repo_dir = create_temp_test_dir("kild_test_assert_wt_repo");
        let worktree_base = create_temp_test_dir("kild_test_assert_wt_base");
        init_test_repo(&repo_dir);

        let repo = Repository::open(&repo_dir).unwrap();
        let worktree_path = worktree_base.join("my-worktree");
        let head_ref = repo.head().unwrap();
        let mut opts = WorktreeAddOptions::new();
        opts.reference(Some(&head_ref));
        let _ = repo.worktree("my-worktree", &worktree_path, Some(&opts));

        // Worktree has .git file (not directory) → should be allowed
        if worktree_path.exists() {
            let result = assert_not_main_repo(&worktree_path);
            assert!(result.is_ok(), "Should allow worktree removal");
        }

        let _ = std::fs::remove_dir_all(&repo_dir);
        let _ = std::fs::remove_dir_all(&worktree_base);
    }

    #[test]
    fn test_remove_worktree_force_blocks_main_repo() {
        let dir = create_temp_test_dir("kild_test_force_blocks_main");
        init_test_repo(&dir);

        let result = remove_worktree_force(&dir);
        assert!(result.is_err(), "Should refuse to force-remove main repo");

        // Directory must still exist
        assert!(dir.exists(), "Main repo directory must not be deleted");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_remove_worktree_by_path_blocks_main_repo() {
        let dir = create_temp_test_dir("kild_test_bypath_blocks_main");
        init_test_repo(&dir);

        let result = remove_worktree_by_path(&dir);
        assert!(result.is_err(), "Should refuse to remove main repo");

        // Directory must still exist
        assert!(dir.exists(), "Main repo directory must not be deleted");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
