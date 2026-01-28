use git2::{BranchType, Repository};
use std::path::Path;
use tracing::{debug, error, info, warn};

use crate::config::KildConfig;
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
    info!(event = "core.git.project.detect_started");

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
        event = "core.git.project.detect_completed",
        project_id = project_id,
        project_name = project_name,
        repo_path = %repo_path.display(),
        remote_url = remote_url.as_deref().unwrap_or("none")
    );

    Ok(project)
}

/// Detect project from a specific path (for UI usage).
///
/// Unlike `detect_project()` which uses current directory, this function
/// uses the provided path to discover the git repository. The path can be
/// anywhere within the repository - `Repository::discover` will walk up
/// the directory tree to find the repository root.
///
/// # Errors
///
/// Returns `GitError::NotInRepository` if the path is not within a git repository.
/// Returns `GitError::OperationFailed` if the repository has no working directory (bare repo).
pub fn detect_project_at(path: &Path) -> Result<ProjectInfo, GitError> {
    info!(event = "core.git.project.detect_at_started", path = %path.display());

    let repo = Repository::discover(path).map_err(|e| {
        debug!(
            event = "core.git.project.discover_failed",
            path = %path.display(),
            error = %e,
            "Repository discovery failed - path may not be in a git repository"
        );
        GitError::NotInRepository
    })?;

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
        event = "core.git.project.detect_at_completed",
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
    config: Option<&KildConfig>,
) -> Result<WorktreeInfo, GitError> {
    let validated_branch = operations::validate_branch_name(branch)?;

    info!(
        event = "core.git.worktree.create_started",
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
            event = "core.git.worktree.create_failed",
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
        event = "core.git.branch.check_completed",
        project_id = project.id,
        branch = validated_branch,
        exists = branch_exists
    );

    // Only create branch if it doesn't exist
    if !branch_exists {
        debug!(
            event = "core.git.branch.create_started",
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
            event = "core.git.branch.create_completed",
            project_id = project.id,
            branch = validated_branch
        );
    }

    // Create worktree - use smart naming based on current branch
    // Sanitize branch name for worktree directory (.git/worktrees/<name> treats / as path separator)
    let worktree_name = if use_current {
        operations::sanitize_for_path(&validated_branch)
    } else {
        format!("kild_{}", operations::sanitize_for_path(&validated_branch))
    };
    repo.worktree(&worktree_name, &worktree_path, None)
        .map_err(|e| GitError::Git2Error { source: e })?;

    let worktree_info = WorktreeInfo::new(
        worktree_path.clone(),
        validated_branch.clone(),
        project.id.clone(),
    );

    info!(
        event = "core.git.worktree.create_completed",
        project_id = project.id,
        branch = validated_branch,
        worktree_path = %worktree_path.display()
    );

    info!(
        event = "core.git.worktree.branch_decision",
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
            event = "core.git.worktree.file_copy_started",
            project_id = project.id,
            branch = validated_branch,
            patterns = ?include_config.patterns
        );

        match files::handler::copy_include_files(&project.path, &worktree_path, include_config) {
            Ok((copied_count, failed_count)) => {
                if failed_count > 0 {
                    warn!(
                        event = "core.git.worktree.file_copy_completed_with_errors",
                        project_id = project.id,
                        branch = validated_branch,
                        files_copied = copied_count,
                        files_failed = failed_count
                    );
                } else {
                    info!(
                        event = "core.git.worktree.file_copy_completed",
                        project_id = project.id,
                        branch = validated_branch,
                        files_copied = copied_count
                    );
                }
            }
            Err(e) => {
                warn!(
                    event = "core.git.worktree.file_copy_failed",
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
        event = "core.git.worktree.remove_started",
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

pub fn remove_worktree_by_path(worktree_path: &Path) -> Result<(), GitError> {
    info!(
        event = "core.git.worktree.remove_by_path_started",
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
            && branch_name.starts_with("kild_")
        {
            match repo.find_branch(branch_name, BranchType::Local) {
                Ok(mut branch) => match branch.delete() {
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
                },
                Err(e) => {
                    debug!(
                        event = "core.git.branch.not_found_for_cleanup",
                        branch = branch_name,
                        worktree_path = %worktree_path.display(),
                        error = %e,
                        message = "Branch already deleted or never existed"
                    );
                }
            }
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
    info!(
        event = "core.git.worktree.remove_force_started",
        path = %worktree_path.display()
    );

    // Try to open the worktree directly first to get the main repo
    let repo =
        Repository::open(worktree_path)
            .ok()
            .and_then(|repo| {
                repo.path()
                    .parent()
                    .and_then(|p| p.parent())
                    .map(|p| p.to_path_buf())
                    .and_then(|main_repo_path| {
                        Repository::open(&main_repo_path)
                            .map_err(|e| {
                                warn!(
                                    event = "core.git.worktree.remove_force_main_repo_open_failed",
                                    path = %main_repo_path.display(),
                                    error = %e,
                                );
                            })
                            .ok()
                    })
                    .or(Some(repo))
            })
            .or_else(|| {
                // Fallback: find main repository by searching parent directories for .git
                let mut current_path = worktree_path;
                while let Some(parent) = current_path.parent() {
                    if parent.join(".git").exists() && parent.join(".git").is_dir() {
                        return Repository::open(parent).map_err(|e| {
                        warn!(
                            event = "core.git.worktree.remove_force_fallback_repo_open_failed",
                            path = %parent.display(),
                            error = %e,
                        );
                    }).ok();
                    }
                    current_path = parent;
                }
                None
            });

    // Try to prune from git if we found the repo
    if let Some(repo) = repo {
        if let Ok(worktrees) = repo.worktrees() {
            for worktree_name in worktrees.iter().flatten() {
                if let Ok(worktree) = repo.find_worktree(worktree_name)
                    && worktree.path() == worktree_path
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
                    break;
                }
            }
        } else {
            warn!(
                event = "core.git.worktree.remove_force_list_worktrees_failed",
                path = %worktree_path.display(),
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
    use std::path::PathBuf;

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

    #[test]
    fn test_remove_worktree_force_nonexistent_is_ok() {
        // Force removal should not error if directory doesn't exist (idempotent)
        let nonexistent = std::path::Path::new("/tmp/shards-test-nonexistent-kild_12345");
        // Make sure it doesn't exist
        let _ = std::fs::remove_dir_all(nonexistent);

        let result = remove_worktree_force(nonexistent);
        assert!(result.is_ok());
    }

    /// Test helper: Create a temporary directory with unique name.
    /// Returns a PathBuf that will be cleaned up when dropped.
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
    fn test_detect_project_at_not_in_repo() {
        let temp_dir = create_temp_test_dir("shards_test_not_a_repo");

        let result = detect_project_at(&temp_dir);

        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), GitError::NotInRepository),
            "Expected NotInRepository error for non-git directory"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_project_at_uses_provided_path_not_cwd() {
        let temp_dir = create_temp_test_dir("shards_test_project_at");
        init_test_repo(&temp_dir);

        let result = detect_project_at(&temp_dir);

        assert!(
            result.is_ok(),
            "detect_project_at should succeed for valid git repo"
        );

        let project = result.unwrap();

        // Verify the project path matches the temp dir, not the cwd.
        // Canonicalize both paths to handle symlinks (e.g., /tmp -> /private/tmp on macOS).
        let expected_path = temp_dir.canonicalize().unwrap();
        let actual_path = project.path.canonicalize().unwrap();
        assert_eq!(
            actual_path, expected_path,
            "ProjectInfo.path should match the provided path, not cwd"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_project_at_discovers_from_subdirectory() {
        let temp_dir = create_temp_test_dir("shards_test_subdir");
        init_test_repo(&temp_dir);

        let subdir = temp_dir.join("src").join("nested").join("deep");
        std::fs::create_dir_all(&subdir).expect("Failed to create subdirectory");

        let result = detect_project_at(&subdir);

        assert!(
            result.is_ok(),
            "detect_project_at should discover repo from subdirectory"
        );

        let project = result.unwrap();

        // Verify the project path is the repo root, not the subdirectory.
        let expected_path = temp_dir.canonicalize().unwrap();
        let actual_path = project.path.canonicalize().unwrap();
        assert_eq!(
            actual_path, expected_path,
            "ProjectInfo.path should be repo root, not subdirectory"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_project_at_project_id_consistent() {
        let temp_dir = create_temp_test_dir("shards_test_consistent_id");
        init_test_repo(&temp_dir);

        let subdir = temp_dir.join("src");
        std::fs::create_dir_all(&subdir).expect("Failed to create subdirectory");

        let project_from_root = detect_project_at(&temp_dir).unwrap();
        let project_from_subdir = detect_project_at(&subdir).unwrap();

        assert_eq!(
            project_from_root.id, project_from_subdir.id,
            "Project ID should be consistent regardless of path within repo"
        );

        assert_eq!(
            project_from_root.path, project_from_subdir.path,
            "Project path should be repo root regardless of input path"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
