use git2::{BranchType, Repository};
use std::path::Path;
use tracing::{debug, error, info, warn};

use crate::config::ShardsConfig;
use crate::files;
use crate::git::{errors::GitError, operations, types::*};

// Helper function to reduce boilerplate
fn io_error(e: std::io::Error) -> GitError {
    GitError::IoError { source: e }
}

fn git2_error(e: git2::Error) -> GitError {
    GitError::Git2Error { source: e }
}

pub fn detect_project() -> Result<ProjectInfo, GitError> {
    info!(event = "git.project.detect_started");

    let current_dir = std::env::current_dir().map_err(io_error)?;

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
    config: Option<&ShardsConfig>,
) -> Result<WorktreeInfo, GitError> {
    let validated_branch = operations::validate_branch_name(branch)?;

    info!(
        event = "git.worktree.create_started",
        project_id = project.id,
        branch = validated_branch,
        repo_path = %project.path.display()
    );

    let repo = Repository::open(&project.path).map_err(git2_error)?;

    // Check current branch for smart worktree naming
    let current_branch = operations::get_current_branch(&repo)?;
    let use_current = current_branch
        .as_ref()
        .map(|cb| operations::should_use_current_branch(cb, &validated_branch))
        .unwrap_or(false);

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
        std::fs::create_dir_all(parent).map_err(io_error)?;
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

    // Create worktree - use smart naming based on current branch
    let worktree_name = if use_current {
        validated_branch.clone()
    } else {
        format!("worktree-{}", validated_branch)
    };
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

    info!(
        event = "git.worktree.branch_decision",
        project_id = project.id,
        requested_branch = validated_branch,
        current_branch = current_branch.as_deref().unwrap_or("none"),
        used_current = use_current,
        worktree_name = worktree_name,
        reason = if use_current {
            "current_branch_matches"
        } else {
            "current_branch_different"
        }
    );

    // Copy include pattern files if configured
    if let Some(config) = config
        && let Some(include_config) = &config.include_patterns
    {
        info!(
            event = "git.worktree.file_copy_started",
            project_id = project.id,
            branch = validated_branch,
            patterns = ?include_config.patterns
        );

        match files::handler::copy_include_files(&project.path, &worktree_path, include_config) {
            Ok((copied_count, failed_count)) => {
                if failed_count > 0 {
                    warn!(
                        event = "git.worktree.file_copy_completed_with_errors",
                        project_id = project.id,
                        branch = validated_branch,
                        files_copied = copied_count,
                        files_failed = failed_count
                    );
                } else {
                    info!(
                        event = "git.worktree.file_copy_completed",
                        project_id = project.id,
                        branch = validated_branch,
                        files_copied = copied_count
                    );
                }
            }
            Err(e) => {
                warn!(
                    event = "git.worktree.file_copy_failed",
                    project_id = project.id,
                    branch = validated_branch,
                    error = %e,
                    message = "File copying failed, but worktree creation succeeded"
                );
            }
        }
    }

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

pub fn remove_worktree_by_path(worktree_path: &Path) -> Result<(), GitError> {
    info!(
        event = "git.worktree.remove_by_path_started",
        worktree_path = %worktree_path.display()
    );

    // Try to open the worktree directly first
    let repo = if let Ok(repo) = Repository::open(worktree_path) {
        // If we can open it as a repo, get the main repository
        if let Some(main_repo_path) = repo
            .path()
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
        {
            Repository::open(main_repo_path).map_err(|e| GitError::Git2Error { source: e })?
        } else {
            repo
        }
    } else {
        // Fallback: try to find the main repository by looking for .git in parent directories
        let mut current_path = worktree_path;
        let mut repo_path = None;

        // Look up the directory tree to find the main repository
        while let Some(parent) = current_path.parent() {
            if parent.join(".git").exists() && parent.join(".git").is_dir() {
                repo_path = Some(parent);
                break;
            }
            current_path = parent;
        }

        let repo_path = repo_path.ok_or_else(|| GitError::OperationFailed {
            message: format!(
                "Could not find main repository for worktree at {}. Searched up directory tree but no .git directory found.",
                worktree_path.display()
            ),
        })?;

        Repository::open(repo_path).map_err(|e| GitError::OperationFailed {
            message: format!(
                "Found potential repository at {} but failed to open it: {}",
                repo_path.display(),
                e
            ),
        })?
    };

    // Find worktree by path
    let worktrees = repo
        .worktrees()
        .map_err(|e| GitError::Git2Error { source: e })?;

    let mut found_worktree = None;
    for worktree_name in worktrees.iter().flatten() {
        if let Ok(worktree) = repo.find_worktree(worktree_name) {
            let wt_path = worktree.path();
            if wt_path == worktree_path {
                found_worktree = Some(worktree);
                break;
            }
        }
    }

    if let Some(worktree) = found_worktree {
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

        // Delete associated branch if it exists and follows worktree naming pattern
        if let Some(ref branch_name) = branch_name
            && branch_name.starts_with("worktree-")
        {
            match repo.find_branch(branch_name, BranchType::Local) {
                Ok(mut branch) => {
                    match branch.delete() {
                        Ok(()) => {
                            info!(
                                event = "git.branch.delete_completed",
                                branch = branch_name,
                                worktree_path = %worktree_path.display()
                            );
                        }
                        Err(e) => {
                            // Handle potential race conditions where branch might be deleted concurrently
                            let error_msg = e.to_string();
                            if error_msg.contains("not found")
                                || error_msg.contains("does not exist")
                            {
                                debug!(
                                    event = "git.branch.delete_race_condition",
                                    branch = branch_name,
                                    worktree_path = %worktree_path.display(),
                                    message = "Branch was deleted by another process"
                                );
                            } else {
                                warn!(
                                    event = "git.branch.delete_failed",
                                    branch = branch_name,
                                    worktree_path = %worktree_path.display(),
                                    error = %e,
                                    error_type = "concurrent_operation_or_permission"
                                );
                            }
                            // Don't fail the whole operation if branch deletion fails
                        }
                    }
                }
                Err(e) => {
                    debug!(
                        event = "git.branch.not_found_for_cleanup",
                        branch = branch_name,
                        worktree_path = %worktree_path.display(),
                        error = %e,
                        message = "Branch already deleted or never existed"
                    );
                    // Branch already gone or not found - that's fine
                }
            }
        }

        info!(
            event = "git.worktree.remove_by_path_completed",
            worktree_path = %worktree_path.display()
        );
    } else {
        // Worktree not found in git registry - state inconsistency detected
        warn!(
            event = "git.worktree.state_inconsistency",
            worktree_path = %worktree_path.display(),
            message = "Worktree directory exists but not registered in git - cleaning up orphaned directory"
        );

        // If worktree not found in git, just remove the directory
        if worktree_path.exists() {
            std::fs::remove_dir_all(worktree_path).map_err(|e| GitError::IoError { source: e })?;
            info!(
                event = "git.worktree.remove_by_path_directory_only",
                worktree_path = %worktree_path.display()
            );
        }
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
