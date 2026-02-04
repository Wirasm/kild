use git2::{BranchType, Repository, WorktreeAddOptions};
use std::path::Path;
use tracing::{debug, error, info, warn};

use crate::config::KildConfig;
use crate::config::types::GitConfig;
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
    git_config: &GitConfig,
) -> Result<WorktreeInfo, GitError> {
    let validated_branch = operations::validate_branch_name(branch)?;

    info!(
        event = "core.git.worktree.create_started",
        project_id = project.id,
        branch = validated_branch,
        repo_path = %project.path.display()
    );

    let repo = Repository::open(&project.path).map_err(git2_error)?;

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

    // With kild/<branch> namespacing and WorktreeAddOptions::reference(), the worktree
    // admin name is always kild-<sanitized_branch> regardless of the current branch.
    // The previous use_current optimization is no longer needed.

    // Branch name: kild/<user_branch> (git-native namespace)
    let kild_branch = operations::kild_branch_name(&validated_branch);

    // Check if kild branch already exists (e.g. recreating a destroyed kild)
    let branch_exists = repo.find_branch(&kild_branch, BranchType::Local).is_ok();

    debug!(
        event = "core.git.branch.check_completed",
        project_id = project.id,
        branch = kild_branch,
        exists = branch_exists
    );

    if !branch_exists {
        debug!(
            event = "core.git.branch.create_started",
            project_id = project.id,
            branch = kild_branch
        );

        // Fetch latest base branch from remote if configured and remote exists
        let remote_exists = repo.find_remote(git_config.remote()).is_ok();

        if git_config.fetch_before_create() && remote_exists {
            fetch_remote(&project.path, git_config.remote(), git_config.base_branch())?;
        } else if git_config.fetch_before_create() && !remote_exists {
            info!(
                event = "core.git.fetch_skipped",
                remote = git_config.remote(),
                reason = "remote not configured"
            );
            eprintln!(
                "Note: Remote '{}' not found, branching from local HEAD.",
                git_config.remote()
            );
        }

        // Resolve base commit: prefer remote tracking branch, fall back to HEAD
        // Only consider fetch "enabled" if remote actually exists — no warning for local repos
        let fetched = git_config.fetch_before_create() && remote_exists;
        let base_commit = resolve_base_commit(&repo, git_config, fetched)?;

        repo.branch(&kild_branch, &base_commit, false)
            .map_err(git2_error)?;

        debug!(
            event = "core.git.branch.create_completed",
            project_id = project.id,
            branch = kild_branch
        );
    }

    // Worktree admin name: kild-<sanitized_branch> (filesystem-safe, flat)
    // Decoupled from branch name via WorktreeAddOptions::reference()
    let worktree_name = operations::kild_worktree_admin_name(&validated_branch);
    let branch_ref = repo
        .find_branch(&kild_branch, BranchType::Local)
        .map_err(git2_error)?;
    let reference = branch_ref.into_reference();

    let mut opts = WorktreeAddOptions::new();
    opts.reference(Some(&reference));

    repo.worktree(&worktree_name, &worktree_path, Some(&opts))
        .map_err(git2_error)?;

    let worktree_info = WorktreeInfo::new(
        worktree_path.clone(),
        validated_branch.clone(),
        project.id.clone(),
    );

    info!(
        event = "core.git.worktree.create_completed",
        project_id = project.id,
        branch = kild_branch,
        worktree_name = worktree_name,
        worktree_path = %worktree_path.display()
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

/// Validate a git argument to prevent injection.
///
/// Rejects:
/// - Dash-prefixed values (git interprets as flags)
/// - Control characters (invisible, could confuse terminal)
/// - `::` sequences (git pseudo-URL refspec syntax)
fn validate_git_arg(value: &str, label: &str) -> Result<(), GitError> {
    if value.starts_with('-') {
        return Err(GitError::OperationFailed {
            message: format!("Invalid {label}: '{value}' (must not start with '-')"),
        });
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(GitError::OperationFailed {
            message: format!("Invalid {label}: contains control characters"),
        });
    }
    if value.contains("::") {
        return Err(GitError::OperationFailed {
            message: format!("Invalid {label}: '::' sequences are not allowed"),
        });
    }
    Ok(())
}

/// Fetch a specific branch from a remote using git CLI.
///
/// Uses `git fetch` CLI to inherit the user's existing auth setup
/// (SSH agent, credential helpers, etc.) with zero auth code.
fn fetch_remote(repo_path: &Path, remote: &str, branch: &str) -> Result<(), GitError> {
    validate_git_arg(remote, "remote name")?;
    validate_git_arg(branch, "branch name")?;

    info!(
        event = "core.git.fetch_started",
        remote = remote,
        branch = branch,
        repo_path = %repo_path.display()
    );

    let output = std::process::Command::new("git")
        .current_dir(repo_path)
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

/// Resolve the base commit for a new branch.
///
/// Tries the remote tracking branch first (e.g., `origin/main`),
/// falls back to local HEAD if the remote ref doesn't exist.
///
/// When `fetch_was_enabled` is true and the remote ref is missing, warns the user
/// since they expected to branch from the remote. When false (--no-fetch), the
/// fallback to HEAD is silent since the user explicitly opted out of fetching.
fn resolve_base_commit<'repo>(
    repo: &'repo Repository,
    git_config: &GitConfig,
    fetch_was_enabled: bool,
) -> Result<git2::Commit<'repo>, GitError> {
    let remote_ref = format!(
        "refs/remotes/{}/{}",
        git_config.remote(),
        git_config.base_branch()
    );

    match repo.find_reference(&remote_ref) {
        Ok(reference) => {
            let commit = reference.peel_to_commit().map_err(git2_error)?;
            info!(
                event = "core.git.base_resolved",
                source = "remote",
                reference = remote_ref,
                commit = %commit.id()
            );
            Ok(commit)
        }
        Err(e) if e.code() == git2::ErrorCode::NotFound => {
            // Remote ref not found - fall back to HEAD
            warn!(
                event = "core.git.base_fallback_to_head",
                remote_ref = remote_ref,
                reason = "remote tracking branch not found"
            );
            // Only warn users when fetch was enabled — they expected the remote ref to exist.
            // With --no-fetch, falling back to HEAD is the expected behavior.
            if fetch_was_enabled {
                eprintln!(
                    "Warning: Remote tracking branch '{}/{}' not found, using local HEAD. \
                     Consider running 'git fetch' first.",
                    git_config.remote(),
                    git_config.base_branch()
                );
            }
            let head = repo.head().map_err(git2_error)?;
            let commit = head.peel_to_commit().map_err(git2_error)?;
            info!(
                event = "core.git.base_resolved",
                source = "head",
                commit = %commit.id()
            );
            Ok(commit)
        }
        Err(e) => Err(git2_error(e)),
    }
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

        // Delete associated branch if it exists and follows worktree naming pattern.
        // Accepts both kild/ (current) and kild_ (legacy) prefixes for backward compatibility.
        if let Some(ref branch_name) = branch_name
            && (branch_name.starts_with(operations::KILD_BRANCH_PREFIX)
                || branch_name.starts_with("kild_"))
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
    fn test_create_worktree_no_orphaned_branch() {
        let temp_dir = create_temp_test_dir("kild_test_no_orphan");
        init_test_repo(&temp_dir);

        let project = ProjectInfo::new(
            "test-id".to_string(),
            "test-project".to_string(),
            temp_dir.clone(),
            None,
        );

        let base_dir = create_temp_test_dir("kild_test_no_orphan_base");
        let git_config = GitConfig {
            fetch_before_create: Some(false),
            ..GitConfig::default()
        };
        let result = create_worktree(&base_dir, &project, "my-feature", None, &git_config);
        assert!(result.is_ok(), "create_worktree should succeed");

        let repo = Repository::open(&temp_dir).unwrap();

        // kild/my-feature branch MUST exist
        assert!(
            repo.find_branch("kild/my-feature", git2::BranchType::Local)
                .is_ok(),
            "kild/my-feature branch should exist"
        );

        // my-feature branch must NOT exist (the core fix for #200)
        assert!(
            repo.find_branch("my-feature", git2::BranchType::Local)
                .is_err(),
            "orphaned my-feature branch should not exist"
        );

        // Worktree should be checked out on kild/my-feature
        let worktree_info = result.unwrap();
        let wt_repo = Repository::open(&worktree_info.path).unwrap();
        let head = wt_repo.head().unwrap();
        assert_eq!(
            head.shorthand().unwrap(),
            "kild/my-feature",
            "worktree HEAD should be on kild/my-feature"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_dir_all(&base_dir);
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
    fn test_create_worktree_slashed_branch_admin_name_decoupling() {
        let temp_dir = create_temp_test_dir("kild_test_slashed");
        init_test_repo(&temp_dir);

        let project = ProjectInfo::new(
            "test-id".to_string(),
            "test-project".to_string(),
            temp_dir.clone(),
            None,
        );

        let base_dir = create_temp_test_dir("kild_test_slashed_base");
        let git_config = GitConfig {
            fetch_before_create: Some(false),
            ..GitConfig::default()
        };
        let result = create_worktree(&base_dir, &project, "feature/auth", None, &git_config);
        assert!(result.is_ok(), "create_worktree should succeed");

        let repo = Repository::open(&temp_dir).unwrap();

        // kild/feature/auth branch should exist (slashes preserved in branch name)
        assert!(
            repo.find_branch("kild/feature/auth", git2::BranchType::Local)
                .is_ok(),
            "kild/feature/auth branch should exist"
        );

        // Admin name should be sanitized: .git/worktrees/kild-feature-auth
        let admin_path = temp_dir.join(".git/worktrees/kild-feature-auth");
        assert!(
            admin_path.exists(),
            "worktree admin dir .git/worktrees/kild-feature-auth should exist"
        );

        // Worktree should be checked out on kild/feature/auth
        let worktree_info = result.unwrap();
        let wt_repo = Repository::open(&worktree_info.path).unwrap();
        let head = wt_repo.head().unwrap();
        assert_eq!(
            head.shorthand().unwrap(),
            "kild/feature/auth",
            "worktree HEAD should be on kild/feature/auth"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_dir_all(&base_dir);
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

    #[test]
    fn test_resolve_base_commit_falls_back_to_head() {
        use crate::config::types::GitConfig;

        let temp_dir = create_temp_test_dir("kild_test_resolve_base");
        init_test_repo(&temp_dir);

        let repo = Repository::open(&temp_dir).unwrap();
        let git_config = GitConfig {
            remote: Some("origin".to_string()),
            base_branch: Some("main".to_string()),
            fetch_before_create: Some(false),
        };

        // No remote set up, should fall back to HEAD
        let commit = resolve_base_commit(&repo, &git_config, false).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(commit.id(), head.id());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_resolve_base_commit_uses_remote_ref_when_present() {
        use crate::config::types::GitConfig;

        let temp_dir = create_temp_test_dir("kild_test_resolve_remote");
        init_test_repo(&temp_dir);

        let repo = Repository::open(&temp_dir).unwrap();

        // Create a fake remote ref to simulate a fetched remote tracking branch
        let head = repo.head().unwrap();
        let head_oid = head.target().unwrap();
        repo.reference(
            "refs/remotes/origin/main",
            head_oid,
            false,
            "test: create remote tracking ref",
        )
        .unwrap();

        let git_config = GitConfig {
            remote: Some("origin".to_string()),
            base_branch: Some("main".to_string()),
            fetch_before_create: Some(false),
        };

        let commit = resolve_base_commit(&repo, &git_config, false).unwrap();
        assert_eq!(commit.id(), head_oid);

        let _ = std::fs::remove_dir_all(&temp_dir);
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
    fn test_create_worktree_succeeds_with_nonexistent_remote() {
        // fetch_before_create=true with nonexistent remote should skip fetch and succeed
        let temp_dir = create_temp_test_dir("kild_test_fetch_fail");
        init_test_repo(&temp_dir);

        let project = ProjectInfo::new(
            "test-id".to_string(),
            "test-project".to_string(),
            temp_dir.clone(),
            None,
        );

        let base_dir = create_temp_test_dir("kild_test_fetch_fail_base");
        let git_config = GitConfig {
            remote: Some("nonexistent".to_string()),
            fetch_before_create: Some(true),
            ..GitConfig::default()
        };

        let result = create_worktree(&base_dir, &project, "test-branch", None, &git_config);
        assert!(
            result.is_ok(),
            "should succeed when remote doesn't exist (fetch skipped): {:?}",
            result.err()
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn test_create_worktree_succeeds_without_remote() {
        // fetch_before_create=true (default) but no remote configured should succeed
        let temp_dir = create_temp_test_dir("kild_test_no_remote");
        init_test_repo(&temp_dir);

        let project = ProjectInfo::new(
            "test-id".to_string(),
            "test-project".to_string(),
            temp_dir.clone(),
            None,
        );

        let base_dir = create_temp_test_dir("kild_test_no_remote_base");
        let git_config = GitConfig::default(); // fetch_before_create defaults to true

        let result = create_worktree(&base_dir, &project, "test-branch", None, &git_config);
        assert!(
            result.is_ok(),
            "should succeed in repo without remote even with fetch enabled: {:?}",
            result.err()
        );

        // Verify worktree was created and is on the correct branch
        let worktree_info = result.unwrap();
        let wt_repo = Repository::open(&worktree_info.path).unwrap();
        let head = wt_repo.head().unwrap();
        assert_eq!(
            head.shorthand().unwrap(),
            "kild/test-branch",
            "worktree HEAD should be on kild/test-branch"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn test_create_worktree_skips_fetch_when_disabled() {
        // fetch_before_create=false with nonexistent remote should still succeed
        let temp_dir = create_temp_test_dir("kild_test_skip_fetch");
        init_test_repo(&temp_dir);

        let project = ProjectInfo::new(
            "test-id".to_string(),
            "test-project".to_string(),
            temp_dir.clone(),
            None,
        );

        let base_dir = create_temp_test_dir("kild_test_skip_fetch_base");
        let git_config = GitConfig {
            remote: Some("nonexistent".to_string()),
            fetch_before_create: Some(false),
            ..GitConfig::default()
        };

        let result = create_worktree(&base_dir, &project, "test-branch", None, &git_config);
        assert!(
            result.is_ok(),
            "should succeed when fetch is disabled: {:?}",
            result.err()
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn test_validate_git_arg_rejects_control_chars() {
        let result = validate_git_arg("origin\x00evil", "remote name");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("control characters")
        );

        let result = validate_git_arg("main\ttab", "branch name");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("control characters")
        );
    }

    #[test]
    fn test_validate_git_arg_rejects_pseudo_urls() {
        let result = validate_git_arg("evil::protocol", "remote name");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("::"));

        let result = validate_git_arg("git::https://evil.com", "remote name");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_git_arg_accepts_valid_values() {
        assert!(validate_git_arg("origin", "remote name").is_ok());
        assert!(validate_git_arg("main", "branch name").is_ok());
        assert!(validate_git_arg("feature/auth", "branch name").is_ok());
        assert!(validate_git_arg("my-remote", "remote name").is_ok());
    }
}
