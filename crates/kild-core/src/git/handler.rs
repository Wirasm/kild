use git2::{BranchType, Repository, WorktreeAddOptions};
use std::path::Path;
use tracing::{debug, error, info, warn};

use crate::files;
use kild_config::GitConfig;
use kild_config::KildConfig;
use kild_git::{
    errors::GitError,
    naming,
    types::{GitProjectState, WorktreeState},
    validation,
};

// Helper function to reduce boilerplate
fn io_error(e: std::io::Error) -> GitError {
    GitError::IoError { source: e }
}

fn git2_error(e: git2::Error) -> GitError {
    GitError::Git2Error { source: e }
}

/// Calls `repo.worktree()` with retry on `git2::ErrorCode::Exists`.
///
/// libgit2's `git_worktree_add()` creates `.git/worktrees/` with a non-atomic
/// mkdir. When two `kild create` processes run concurrently, the second fails
/// with `Exists(-4)` because the first just created the directory. A retry
/// always succeeds since the directory now exists and libgit2 proceeds normally.
///
/// Only retries when the admin entry (`.git/worktrees/<name>`) does not yet
/// exist — meaning the `Exists` error is from the parent dir race, not a genuine
/// duplicate worktree name. If the admin entry already exists, the error is
/// propagated immediately without burning retry budget.
fn add_git_worktree_with_retry(
    repo: &Repository,
    name: &str,
    path: &std::path::Path,
    opts: &WorktreeAddOptions<'_>,
) -> Result<(), GitError> {
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(50);

    let mut attempt = 0;
    loop {
        match repo.worktree(name, path, Some(opts)) {
            Ok(_) => return Ok(()),
            Err(e) if e.code() == git2::ErrorCode::Exists && attempt < MAX_RETRIES => {
                // Distinguish the transient parent-dir race from a genuine duplicate
                // admin entry. If `.git/worktrees/<name>` already exists, the error
                // is permanent — retrying would not help.
                let admin_exists = repo.path().join("worktrees").join(name).exists();
                if admin_exists {
                    return Err(git2_error(e));
                }
                attempt += 1;
                warn!(
                    event = "core.git.worktree.create_retry",
                    attempt = attempt,
                    error = %e,
                    "Retrying worktree creation after concurrent mkdir race"
                );
                std::thread::sleep(RETRY_DELAY);
            }
            Err(e) => return Err(git2_error(e)),
        }
    }
}

pub fn create_worktree(
    base_dir: &Path,
    project: &GitProjectState,
    branch: &str,
    config: Option<&KildConfig>,
    git_config: &GitConfig,
) -> Result<WorktreeState, GitError> {
    let validated_branch = validation::validate_branch_name(branch)?;

    info!(
        event = "core.git.worktree.create_started",
        project_id = project.id,
        branch = %validated_branch,
        repo_path = %project.path.display()
    );

    let repo = Repository::open(&project.path).map_err(git2_error)?;

    let worktree_path = naming::calculate_worktree_path(base_dir, &project.name, &validated_branch);

    // Check if worktree already exists
    if worktree_path.exists() {
        error!(
            event = "core.git.worktree.create_failed",
            project_id = project.id,
            branch = %validated_branch,
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
    let kild_branch = naming::kild_branch_name(&validated_branch);

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
            kild_git::fetch_remote(&project.path, git_config.remote(), git_config.base_branch())?;
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
    let worktree_name = naming::kild_worktree_admin_name(&validated_branch);
    let branch_ref = repo
        .find_branch(&kild_branch, BranchType::Local)
        .map_err(git2_error)?;
    let reference = branch_ref.into_reference();

    let mut opts = WorktreeAddOptions::new();
    opts.reference(Some(&reference));

    add_git_worktree_with_retry(&repo, &worktree_name, &worktree_path, &opts)?;

    let worktree_info = WorktreeState::new(
        worktree_path.clone(),
        validated_branch.to_string(),
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
            branch = %validated_branch,
            patterns = ?include_config.patterns
        );

        match files::handler::copy_include_files(&project.path, &worktree_path, include_config) {
            Ok((copied_count, failed_count)) => {
                if failed_count > 0 {
                    warn!(
                        event = "core.git.worktree.file_copy_completed_with_errors",
                        project_id = project.id,
                        branch = %validated_branch,
                        files_copied = copied_count,
                        files_failed = failed_count
                    );
                } else {
                    info!(
                        event = "core.git.worktree.file_copy_completed",
                        project_id = project.id,
                        branch = %validated_branch,
                        files_copied = copied_count
                    );
                }
            }
            Err(e) => {
                warn!(
                    event = "core.git.worktree.file_copy_failed",
                    project_id = project.id,
                    branch = %validated_branch,
                    error = %e,
                    message = "File copying failed, but worktree creation succeeded"
                );
            }
        }
    }

    Ok(worktree_info)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
    fn test_create_worktree_no_orphaned_branch() {
        let temp_dir = create_temp_test_dir("kild_test_no_orphan");
        init_test_repo(&temp_dir);

        let project = GitProjectState::new(
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
    fn test_create_worktree_slashed_branch_admin_name_decoupling() {
        let temp_dir = create_temp_test_dir("kild_test_slashed");
        init_test_repo(&temp_dir);

        let project = GitProjectState::new(
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
    fn test_resolve_base_commit_falls_back_to_head() {
        use kild_config::GitConfig;

        let temp_dir = create_temp_test_dir("kild_test_resolve_base");
        init_test_repo(&temp_dir);

        let repo = Repository::open(&temp_dir).unwrap();
        let git_config = GitConfig {
            remote: Some("origin".to_string()),
            base_branch: Some("main".to_string()),
            fetch_before_create: Some(false),
            ..Default::default()
        };

        // No remote set up, should fall back to HEAD
        let commit = resolve_base_commit(&repo, &git_config, false).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(commit.id(), head.id());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_resolve_base_commit_uses_remote_ref_when_present() {
        use kild_config::GitConfig;

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
            ..Default::default()
        };

        let commit = resolve_base_commit(&repo, &git_config, false).unwrap();
        assert_eq!(commit.id(), head_oid);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_create_worktree_succeeds_with_nonexistent_remote() {
        // fetch_before_create=true with nonexistent remote should skip fetch and succeed
        let temp_dir = create_temp_test_dir("kild_test_fetch_fail");
        init_test_repo(&temp_dir);

        let project = GitProjectState::new(
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

        let project = GitProjectState::new(
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

        let project = GitProjectState::new(
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
    fn test_concurrent_worktree_creation_different_branches() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let temp_dir = create_temp_test_dir("kild_test_concurrent");
        init_test_repo(&temp_dir);
        let base_dir = create_temp_test_dir("kild_test_concurrent_base");

        let temp_dir = Arc::new(temp_dir);
        let base_dir = Arc::new(base_dir);
        let barrier = Arc::new(Barrier::new(2));

        let handles: Vec<_> = ["branch-a", "branch-b"]
            .iter()
            .map(|branch| {
                let temp_dir = Arc::clone(&temp_dir);
                let base_dir = Arc::clone(&base_dir);
                let barrier = Arc::clone(&barrier);
                let branch = branch.to_string();

                thread::spawn(move || {
                    let project = GitProjectState::new(
                        "test-id".to_string(),
                        "test-project".to_string(),
                        (*temp_dir).clone(),
                        None,
                    );
                    let git_config = GitConfig {
                        fetch_before_create: Some(false),
                        ..GitConfig::default()
                    };

                    // Synchronize both threads to start create_worktree simultaneously.
                    // This exercises concurrent in-process creates of different branches.
                    // The inter-process race (two separate OS processes) is not reproducible
                    // in a unit test without subprocess spawning, but this covers the same
                    // libgit2 non-atomic mkdir path since both threads call mkdir(2) directly.
                    barrier.wait();

                    create_worktree(&base_dir, &project, &branch, None, &git_config)
                })
            })
            .collect();

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        assert!(
            results.iter().all(|r| r.is_ok()),
            "Both concurrent worktree creations should succeed, got: {:?}",
            results.iter().filter(|r| r.is_err()).collect::<Vec<_>>()
        );

        let _ = std::fs::remove_dir_all(temp_dir.as_ref());
        let _ = std::fs::remove_dir_all(base_dir.as_ref());
    }
}
