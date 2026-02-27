//! Test helpers for creating git repositories, branches, and worktrees.
//!
//! These functions wrap git2 operations so test code outside the `git/` module
//! doesn't need to import git2 directly.

use std::path::Path;

use git2::{BranchType, Repository, WorktreeAddOptions};

use crate::errors::GitError;

/// Initialize a new git repository at the given path with an initial commit.
pub fn init_repo_with_commit(path: &Path) -> Result<(), GitError> {
    let repo = Repository::init(path).map_err(|e| GitError::Git2Error { source: e })?;
    let sig = repo
        .signature()
        .unwrap_or_else(|_| git2::Signature::now("Test", "test@test.com").unwrap());
    let tree_id = repo
        .index()
        .map_err(|e| GitError::Git2Error { source: e })?
        .write_tree()
        .map_err(|e| GitError::Git2Error { source: e })?;
    let tree = repo
        .find_tree(tree_id)
        .map_err(|e| GitError::Git2Error { source: e })?;
    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
        .map_err(|e| GitError::Git2Error { source: e })?;
    Ok(())
}

/// Create a local branch pointing at HEAD.
pub fn create_branch(path: &Path, name: &str) -> Result<(), GitError> {
    let repo = Repository::open(path).map_err(|e| GitError::Git2Error { source: e })?;
    let head = repo.head().map_err(|e| GitError::Git2Error { source: e })?;
    let commit = head
        .peel_to_commit()
        .map_err(|e| GitError::Git2Error { source: e })?;
    repo.branch(name, &commit, false)
        .map_err(|e| GitError::Git2Error { source: e })?;
    Ok(())
}

/// Create a worktree checked out on an existing branch.
pub fn create_worktree_for_branch(
    repo_path: &Path,
    admin_name: &str,
    worktree_path: &Path,
    branch_name: &str,
) -> Result<(), GitError> {
    let repo = Repository::open(repo_path).map_err(|e| GitError::Git2Error { source: e })?;
    let branch_ref = repo
        .find_branch(branch_name, BranchType::Local)
        .map_err(|e| GitError::Git2Error { source: e })?
        .into_reference();
    let mut opts = WorktreeAddOptions::new();
    opts.reference(Some(&branch_ref));
    repo.worktree(admin_name, worktree_path, Some(&opts))
        .map_err(|e| GitError::Git2Error { source: e })?;
    Ok(())
}
