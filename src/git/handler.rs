use git2::{BranchType, Repository};
use std::path::Path;
use tracing::{debug, error, info};

use crate::git::{errors::GitError, operations, types::*};

pub fn detect_project() -> Result<ProjectInfo, GitError> {
    info!(event = "git.project.detect_started");

    let current_dir = std::env::current_dir().map_err(|e| GitError::IoError { source: e })?;

    let repo = Repository::discover(&current_dir).map_err(|_| GitError::NotInRepository)?;

    let repo_path = repo.workdir().ok_or_else(|| GitError::OperationFailed {
        message: "Repository has no working directory".to_string(),
    })?;

    let remote_url = repo
        .find_remote("origin")
        .ok()
        .and_then(|remote| remote.url().map(|s| s.to_string()));

    let project_name = if let Some(ref url) = remote_url {
        operations::derive_project_name_from_remote(url)
    } else {
        operations::derive_project_name_from_path(repo_path)
    };

    let project_id = operations::generate_project_id(repo_path);

    let project = ProjectInfo::new(
        project_id.clone(),
        project_name.clone(),
        repo_path.to_path_buf(),
        remote_url.clone(),
    );

    info!(
        event = "git.project.detect_completed",
        project_id = project_id,
        project_name = project_name,
        repo_path = %repo_path.display(),
        remote_url = remote_url.as_deref().unwrap_or("none")
    );

    Ok(project)
}

pub fn create_worktree(
    base_dir: &Path,
    project: &ProjectInfo,
    branch: &str,
) -> Result<WorktreeInfo, GitError> {
    let validated_branch = operations::validate_branch_name(branch)?;

    info!(
        event = "git.worktree.create_started",
        project_id = project.id,
        branch = validated_branch,
        repo_path = %project.path.display()
    );

    let repo = Repository::open(&project.path).map_err(|e| GitError::Git2Error { source: e })?;

    let worktree_path =
        operations::calculate_worktree_path(base_dir, &project.name, &validated_branch);

    // Check if worktree already exists
    if worktree_path.exists() {
        error!(
            event = "git.worktree.create_failed",
            project_id = project.id,
            branch = validated_branch,
            worktree_path = %worktree_path.display(),
            error = "worktree already exists"
        );
        return Err(GitError::WorktreeAlreadyExists {
            path: worktree_path.display().to_string(),
        });
    }

    // Create parent directories
    if let Some(parent) = worktree_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| GitError::IoError { source: e })?;
    }

    // Check if branch exists
    let branch_exists = repo
        .find_branch(&validated_branch, BranchType::Local)
        .is_ok();

    debug!(
        event = "git.branch.check_completed",
        project_id = project.id,
        branch = validated_branch,
        exists = branch_exists
    );

    // Only create branch if it doesn't exist
    if !branch_exists {
        debug!(
            event = "git.branch.create_started",
            project_id = project.id,
            branch = validated_branch
        );

        // Create new branch from HEAD
        let head = repo.head().map_err(|e| GitError::Git2Error { source: e })?;
        let head_commit = head
            .peel_to_commit()
            .map_err(|e| GitError::Git2Error { source: e })?;

        repo.branch(&validated_branch, &head_commit, false)
            .map_err(|e| GitError::Git2Error { source: e })?;

        debug!(
            event = "git.branch.create_completed",
            project_id = project.id,
            branch = validated_branch
        );
    }

    // Create worktree - use a different name to avoid conflicts
    let worktree_name = format!("worktree-{}", validated_branch);
    repo.worktree(&worktree_name, &worktree_path, None)
        .map_err(|e| GitError::Git2Error { source: e })?;

    let worktree_info = WorktreeInfo::new(
        worktree_path.clone(),
        validated_branch.clone(),
        project.id.clone(),
    );

    info!(
        event = "git.worktree.create_completed",
        project_id = project.id,
        branch = validated_branch,
        worktree_path = %worktree_path.display()
    );

    Ok(worktree_info)
}

pub fn remove_worktree(project: &ProjectInfo, worktree_path: &Path) -> Result<(), GitError> {
    info!(
        event = "git.worktree.remove_started",
        project_id = project.id,
        worktree_path = %worktree_path.display()
    );

    let repo = Repository::open(&project.path).map_err(|e| GitError::Git2Error { source: e })?;

    // Find worktree by path
    let worktrees = repo
        .worktrees()
        .map_err(|e| GitError::Git2Error { source: e })?;

    let mut found_worktree = None;
    for worktree_name in worktrees.iter().flatten() {
        if let Ok(worktree) = repo.find_worktree(worktree_name) {
            // Get worktree path - this returns the path directly
            let wt_path = worktree.path();
            if wt_path == worktree_path {
                found_worktree = Some(worktree);
                break;
            }
        }
    }

    if let Some(worktree) = found_worktree {
        // Remove worktree
        worktree
            .prune(None)
            .map_err(|e| GitError::Git2Error { source: e })?;

        // Remove directory if it still exists
        if worktree_path.exists() {
            std::fs::remove_dir_all(worktree_path).map_err(|e| GitError::IoError { source: e })?;
        }

        info!(
            event = "git.worktree.remove_completed",
            project_id = project.id,
            worktree_path = %worktree_path.display()
        );
    } else {
        error!(
            event = "git.worktree.remove_failed",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_project_not_in_repo() {
        // This test assumes we're not in a git repo at /tmp
        let original_dir = std::env::current_dir().unwrap();

        // Try to change to a non-git directory for testing
        if std::env::set_current_dir("/tmp").is_ok() {
            let result = detect_project();
            // Should fail if /tmp is not a git repo
            if result.is_err() {
                assert!(matches!(result.unwrap_err(), GitError::NotInRepository));
            }

            // Restore original directory
            let _ = std::env::set_current_dir(original_dir);
        }
    }

    // Note: Other tests would require setting up actual git repositories
    // which is complex for unit tests. Integration tests would be better.
}
