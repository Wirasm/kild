use std::path::Path;

use crate::git::errors::GitError;

/// Fetch a specific branch from a remote using git CLI.
///
/// Delegates to [`super::cli::fetch`] for centralized CLI handling.
pub fn fetch_remote(repo_path: &Path, remote: &str, branch: &str) -> Result<(), GitError> {
    super::cli::fetch(repo_path, remote, branch)
}

/// Rebase a worktree onto the given base branch.
///
/// Delegates to [`super::cli::rebase`] for centralized CLI handling.
/// On conflict, auto-aborts the rebase and returns `GitError::RebaseConflict`.
pub fn rebase_worktree(worktree_path: &Path, base_branch: &str) -> Result<(), GitError> {
    super::cli::rebase(worktree_path, base_branch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{BranchType, Repository, WorktreeAddOptions};
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

    /// Test helper: Add a file and commit in a repository.
    fn add_and_commit(repo: &Repository, filename: &str, message: &str) {
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new(filename)).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo
            .signature()
            .unwrap_or_else(|_| git2::Signature::now("Test", "test@test.com").unwrap());
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
            .unwrap();
    }

    /// Test helper: Get the default branch name (e.g. "main" or "master").
    fn default_branch_name(repo: &Repository) -> String {
        repo.head().unwrap().shorthand().unwrap().to_string()
    }

    #[test]
    fn test_fetch_remote_rejects_dash_prefixed_remote() {
        let temp_dir = create_temp_test_dir("kild_test_fetch_dash_remote");
        init_test_repo(&temp_dir);

        let result = fetch_remote(&temp_dir, "--upload-pack=evil", "main");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GitError::OperationFailed { .. }
        ));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_fetch_remote_rejects_dash_prefixed_branch() {
        let temp_dir = create_temp_test_dir("kild_test_fetch_dash_branch");
        init_test_repo(&temp_dir);

        let result = fetch_remote(&temp_dir, "origin", "--upload-pack=evil");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GitError::OperationFailed { .. }
        ));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_fetch_remote_fails_with_nonexistent_remote() {
        let temp_dir = create_temp_test_dir("kild_test_fetch_no_remote");
        init_test_repo(&temp_dir);

        let result = fetch_remote(&temp_dir, "nonexistent", "main");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GitError::FetchFailed { .. }));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_rebase_worktree_success() {
        let repo_dir = create_temp_test_dir("kild_test_rebase_success");
        let worktree_base = create_temp_test_dir("kild_test_rebase_success_wt");
        init_test_repo(&repo_dir);

        let repo = Repository::open(&repo_dir).unwrap();
        let base_branch = default_branch_name(&repo);
        let head = repo.head().unwrap();
        let head_commit = head.peel_to_commit().unwrap();

        // Create kild branch from current HEAD
        repo.branch("kild/test", &head_commit, false).unwrap();

        // Create worktree
        let worktree_path = worktree_base.join("test");
        let branch_ref = repo
            .find_branch("kild/test", BranchType::Local)
            .unwrap()
            .into_reference();
        let mut opts = WorktreeAddOptions::new();
        opts.reference(Some(&branch_ref));
        repo.worktree("kild-test", &worktree_path, Some(&opts))
            .unwrap();

        // Canonicalize for macOS /tmp -> /private/tmp
        let canonical_wt = worktree_path.canonicalize().unwrap();

        // Rebase onto base branch (no-op since branch is already at HEAD)
        let result = rebase_worktree(&canonical_wt, &base_branch);
        assert!(result.is_ok(), "Clean rebase should succeed: {:?}", result);

        let _ = std::fs::remove_dir_all(&repo_dir);
        let _ = std::fs::remove_dir_all(&worktree_base);
    }

    #[test]
    fn test_rebase_worktree_conflict_auto_abort() {
        let repo_dir = create_temp_test_dir("kild_test_rebase_conflict");
        let worktree_base = create_temp_test_dir("kild_test_rebase_conflict_wt");
        init_test_repo(&repo_dir);

        let repo = Repository::open(&repo_dir).unwrap();
        let base_branch = default_branch_name(&repo);
        let head = repo.head().unwrap();
        let head_commit = head.peel_to_commit().unwrap();

        // Create kild branch from current HEAD
        repo.branch("kild/test", &head_commit, false).unwrap();

        // Create worktree
        let worktree_path = worktree_base.join("test");
        let branch_ref = repo
            .find_branch("kild/test", BranchType::Local)
            .unwrap()
            .into_reference();
        let mut opts = WorktreeAddOptions::new();
        opts.reference(Some(&branch_ref));
        repo.worktree("kild-test", &worktree_path, Some(&opts))
            .unwrap();

        // Add conflicting file on base branch
        std::fs::write(repo_dir.join("conflict.txt"), "main version\n").unwrap();
        add_and_commit(&repo, "conflict.txt", "main: add conflict file");

        // Add conflicting file in worktree
        let wt_repo = Repository::open(&worktree_path).unwrap();
        std::fs::write(worktree_path.join("conflict.txt"), "branch version\n").unwrap();
        add_and_commit(&wt_repo, "conflict.txt", "branch: add conflict file");

        // Canonicalize for macOS /tmp -> /private/tmp
        let canonical_wt = worktree_path.canonicalize().unwrap();

        // Attempt rebase — should detect conflict and auto-abort
        let result = rebase_worktree(&canonical_wt, &base_branch);
        assert!(result.is_err(), "Rebase with conflicts should fail");

        match result.unwrap_err() {
            GitError::RebaseConflict {
                base_branch: err_base,
                worktree_path: err_path,
            } => {
                assert_eq!(err_base, base_branch);
                assert_eq!(err_path, canonical_wt);
            }
            other => panic!("Expected RebaseConflict, got: {:?}", other),
        }

        // Verify worktree is clean after auto-abort
        let wt_repo = Repository::open(&canonical_wt).unwrap();
        let statuses = wt_repo.statuses(None).unwrap();
        assert_eq!(
            statuses.len(),
            0,
            "Worktree should be clean after auto-abort"
        );

        let _ = std::fs::remove_dir_all(&repo_dir);
        let _ = std::fs::remove_dir_all(&worktree_base);
    }

    #[test]
    fn test_rebase_worktree_rejects_dash_prefixed_branch() {
        let temp_dir = create_temp_test_dir("kild_test_rebase_dash");
        init_test_repo(&temp_dir);

        let result = rebase_worktree(&temp_dir, "--upload-pack=evil");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GitError::OperationFailed { .. }
        ));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_rebase_worktree_rejects_control_chars() {
        let temp_dir = create_temp_test_dir("kild_test_rebase_control");
        init_test_repo(&temp_dir);

        let result = rebase_worktree(&temp_dir, "main\x00evil");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GitError::OperationFailed { .. }
        ));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
