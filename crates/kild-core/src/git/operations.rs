use crate::git::errors::GitError;
use crate::git::types::DiffStats;
use git2::Repository;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Sanitize a string for safe use in filesystem paths and git2 worktree names.
///
/// Replaces `/` with `-` to prevent nested directory creation. Git branch names
/// like `feature/foo` are valid, but git2's `repo.worktree()` treats the name
/// parameter as a directory name under `.git/worktrees/`, interpreting slashes
/// as path separators and attempting to create nested directories.
///
/// The `-` replacement matches the pattern in `process/pid_file.rs`.
pub fn sanitize_for_path(s: &str) -> String {
    s.replace('/', "-")
}

pub fn calculate_worktree_path(base_dir: &Path, project_name: &str, branch: &str) -> PathBuf {
    let safe_branch = sanitize_for_path(branch);
    base_dir
        .join("worktrees")
        .join(project_name)
        .join(safe_branch)
}

pub fn derive_project_name_from_path(repo_path: &Path) -> String {
    repo_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string()
}

pub fn derive_project_name_from_remote(remote_url: &str) -> String {
    // Extract repo name from URLs like:
    // https://github.com/user/repo.git -> repo
    // git@github.com:user/repo.git -> repo

    let url = remote_url.trim_end_matches(".git");

    if let Some(last_slash) = url.rfind('/') {
        url[last_slash + 1..].to_string()
    } else if let Some(colon) = url.rfind(':') {
        if let Some(slash) = url[colon..].find('/') {
            url[colon + slash + 1..].to_string()
        } else {
            url[colon + 1..].to_string()
        }
    } else {
        "unknown".to_string()
    }
}

pub fn generate_project_id(repo_path: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    repo_path.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

pub fn validate_branch_name(branch: &str) -> Result<String, GitError> {
    let trimmed = branch.trim();

    if trimmed.is_empty() {
        return Err(GitError::OperationFailed {
            message: "Branch name cannot be empty".to_string(),
        });
    }

    // Git branch name validation rules
    if trimmed.contains("..")
        || trimmed.starts_with('-')
        || trimmed.contains(' ')
        || trimmed.contains('\t')
        || trimmed.contains('\n')
    {
        return Err(GitError::OperationFailed {
            message: format!("Invalid branch name: '{}'", trimmed),
        });
    }

    Ok(trimmed.to_string())
}

/// Gets the current branch name from the repository.
///
/// Returns `None` if the repository is in a detached HEAD state.
///
/// # Errors
/// Returns `GitError::Git2Error` if the repository HEAD cannot be accessed.
pub fn get_current_branch(repo: &git2::Repository) -> Result<Option<String>, GitError> {
    let head = repo.head().map_err(|e| GitError::Git2Error { source: e })?;

    if let Some(branch_name) = head.shorthand() {
        Ok(Some(branch_name.to_string()))
    } else {
        // Detached HEAD state - no current branch
        debug!("Repository is in detached HEAD state, no current branch available");
        Ok(None)
    }
}

/// Determines if the current branch should be used for the worktree.
///
/// Returns `true` if the current branch name exactly matches the requested branch name.
pub fn should_use_current_branch(current_branch: &str, requested_branch: &str) -> bool {
    current_branch == requested_branch
}

pub fn is_valid_git_directory(path: &Path) -> bool {
    path.join(".git").exists()
}

/// Get diff statistics for unstaged changes in a worktree.
///
/// Returns the number of insertions, deletions, and files changed
/// between the index (staging area) and the working directory.
/// This does not include staged changes.
///
/// # Errors
///
/// Returns `GitError::Git2Error` if the repository cannot be opened
/// or the diff cannot be computed.
pub fn get_diff_stats(worktree_path: &Path) -> Result<DiffStats, GitError> {
    let repo = Repository::open(worktree_path).map_err(|e| GitError::Git2Error { source: e })?;

    let diff = repo
        .diff_index_to_workdir(None, None)
        .map_err(|e| GitError::Git2Error { source: e })?;

    let stats = diff
        .stats()
        .map_err(|e| GitError::Git2Error { source: e })?;

    Ok(DiffStats {
        insertions: stats.insertions(),
        deletions: stats.deletions(),
        files_changed: stats.files_changed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_for_path() {
        assert_eq!(sanitize_for_path("feature/foo"), "feature-foo");
        assert_eq!(sanitize_for_path("bugfix/auth/login"), "bugfix-auth-login");
        assert_eq!(sanitize_for_path("simple-branch"), "simple-branch");
        assert_eq!(sanitize_for_path("no_slashes_here"), "no_slashes_here");
    }

    #[test]
    fn test_sanitize_for_path_edge_cases() {
        // Multiple consecutive slashes
        assert_eq!(sanitize_for_path("feature//auth"), "feature--auth");

        // Leading slash (invalid git branch, but document behavior)
        assert_eq!(sanitize_for_path("/feature"), "-feature");

        // Trailing slash (invalid git branch, but document behavior)
        assert_eq!(sanitize_for_path("feature/"), "feature-");

        // Mixed valid characters preserved
        assert_eq!(sanitize_for_path("feat/bug_fix-123"), "feat-bug_fix-123");
    }

    #[test]
    fn test_sanitize_collision_awareness() {
        // Document that different branches can sanitize to the same name.
        // Git2 will reject duplicate worktree names at creation time.
        let sanitized_with_slash = sanitize_for_path("feature/foo");
        let sanitized_with_hyphen = sanitize_for_path("feature-foo");

        // Both sanitize to the same filesystem-safe name
        assert_eq!(sanitized_with_slash, sanitized_with_hyphen);
        assert_eq!(sanitized_with_slash, "feature-foo");
    }

    #[test]
    fn test_calculate_worktree_path() {
        let base = Path::new("/home/user/.shards");
        let path = calculate_worktree_path(base, "my-project", "feature-branch");

        assert_eq!(
            path,
            PathBuf::from("/home/user/.shards/worktrees/my-project/feature-branch")
        );
    }

    #[test]
    fn test_calculate_worktree_path_with_slashes() {
        let base = Path::new("/home/user/.kild");

        // Branch with single slash
        let path = calculate_worktree_path(base, "my-project", "feature/auth");
        assert_eq!(
            path,
            PathBuf::from("/home/user/.kild/worktrees/my-project/feature-auth")
        );

        // Branch with multiple slashes
        let path = calculate_worktree_path(base, "my-project", "feature/auth/oauth");
        assert_eq!(
            path,
            PathBuf::from("/home/user/.kild/worktrees/my-project/feature-auth-oauth")
        );

        // Branch without slashes (unchanged behavior)
        let path = calculate_worktree_path(base, "my-project", "simple-branch");
        assert_eq!(
            path,
            PathBuf::from("/home/user/.kild/worktrees/my-project/simple-branch")
        );
    }

    #[test]
    fn test_derive_project_name_from_path() {
        let path = Path::new("/home/user/projects/my-awesome-project");
        let name = derive_project_name_from_path(path);
        assert_eq!(name, "my-awesome-project");

        let root_path = Path::new("/");
        let root_name = derive_project_name_from_path(root_path);
        assert_eq!(root_name, "unknown");
    }

    #[test]
    fn test_derive_project_name_from_remote() {
        assert_eq!(
            derive_project_name_from_remote("https://github.com/user/repo.git"),
            "repo"
        );

        assert_eq!(
            derive_project_name_from_remote("git@github.com:user/repo.git"),
            "repo"
        );

        assert_eq!(
            derive_project_name_from_remote("https://gitlab.com/group/subgroup/project.git"),
            "project"
        );

        assert_eq!(derive_project_name_from_remote("invalid-url"), "unknown");
    }

    #[test]
    fn test_generate_project_id() {
        let path1 = Path::new("/path/to/project");
        let path2 = Path::new("/different/path");

        let id1 = generate_project_id(path1);
        let id2 = generate_project_id(path2);

        assert_ne!(id1, id2);
        assert!(!id1.is_empty());
        assert!(!id2.is_empty());

        // Same path should generate same ID
        let id1_again = generate_project_id(path1);
        assert_eq!(id1, id1_again);
    }

    #[test]
    fn test_validate_branch_name() {
        assert!(validate_branch_name("feature-branch").is_ok());
        assert!(validate_branch_name("feat/auth").is_ok());
        assert!(validate_branch_name("v1.2.3").is_ok());

        assert!(validate_branch_name("").is_err());
        assert!(validate_branch_name("  ").is_err());
        assert!(validate_branch_name("branch..name").is_err());
        assert!(validate_branch_name("-branch").is_err());
        assert!(validate_branch_name("branch name").is_err());
        assert!(validate_branch_name("branch\tname").is_err());
        assert!(validate_branch_name("branch\nname").is_err());
    }

    #[test]
    fn test_is_valid_git_directory() {
        // This will fail in most test environments, but tests the logic
        let current_dir = std::env::current_dir().unwrap();
        let _is_git = is_valid_git_directory(&current_dir);

        let non_git_dir = Path::new("/tmp");
        assert!(!is_valid_git_directory(non_git_dir) || non_git_dir.join(".git").exists());
    }

    #[test]
    fn test_should_use_current_branch() {
        assert!(should_use_current_branch(
            "feature-branch",
            "feature-branch"
        ));
        assert!(!should_use_current_branch("main", "feature-branch"));
        assert!(!should_use_current_branch("feature-branch", "main"));
        assert!(should_use_current_branch("issue-33", "issue-33"));
    }

    // --- get_diff_stats tests ---

    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_git_repo(dir: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .expect("Failed to init git repo");
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .expect("Failed to set git email");
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .expect("Failed to set git name");
    }

    #[test]
    fn test_get_diff_stats_clean_repo() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Create and commit a file
        fs::write(dir.path().join("test.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let stats = get_diff_stats(dir.path()).unwrap();
        assert_eq!(stats.insertions, 0);
        assert_eq!(stats.deletions, 0);
        assert_eq!(stats.files_changed, 0);
    }

    #[test]
    fn test_get_diff_stats_with_changes() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Create and commit a file
        fs::write(dir.path().join("test.txt"), "line1\nline2\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Make changes
        fs::write(dir.path().join("test.txt"), "line1\nmodified\nnew line\n").unwrap();

        let stats = get_diff_stats(dir.path()).unwrap();
        assert!(stats.insertions > 0 || stats.deletions > 0);
        assert_eq!(stats.files_changed, 1);
    }

    #[test]
    fn test_get_diff_stats_not_a_repo() {
        let dir = TempDir::new().unwrap();
        // Don't init git

        let result = get_diff_stats(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_get_diff_stats_staged_changes_not_included() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Initial commit
        fs::write(dir.path().join("test.txt"), "line1\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Stage a change (but don't commit)
        fs::write(dir.path().join("test.txt"), "line1\nstaged line\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Staged changes should NOT appear (diff_index_to_workdir only sees unstaged)
        let stats = get_diff_stats(dir.path()).unwrap();
        assert_eq!(
            stats.insertions, 0,
            "Staged changes should not appear in index-to-workdir diff"
        );
        assert_eq!(stats.files_changed, 0);
        assert!(!stats.has_changes());
    }

    #[test]
    fn test_get_diff_stats_binary_file() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Create binary file (PNG header bytes)
        let png_header: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        fs::write(dir.path().join("image.png"), png_header).unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Modify binary
        let modified: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0xFF, 0xFF, 0xFF, 0xFF];
        fs::write(dir.path().join("image.png"), modified).unwrap();

        let stats = get_diff_stats(dir.path()).unwrap();
        // Binary files are detected as changed
        assert_eq!(
            stats.files_changed, 1,
            "Binary file change should be detected"
        );
        // Note: git2 may report small line counts for binary files depending on content
        // The key assertion is that the file change is detected
    }

    #[test]
    fn test_get_diff_stats_untracked_files_not_included() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        fs::write(dir.path().join("committed.txt"), "initial").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Create untracked file (NOT staged)
        fs::write(dir.path().join("untracked.txt"), "new file content\n").unwrap();

        let stats = get_diff_stats(dir.path()).unwrap();
        // Untracked files don't appear in index-to-workdir diff
        assert_eq!(
            stats.files_changed, 0,
            "Untracked files should not be counted"
        );
        assert!(!stats.has_changes());
    }

    #[test]
    fn test_diff_stats_has_changes() {
        use crate::git::types::DiffStats;

        let no_changes = DiffStats::default();
        assert!(!no_changes.has_changes());

        let insertions_only = DiffStats {
            insertions: 5,
            deletions: 0,
            files_changed: 1,
        };
        assert!(insertions_only.has_changes());

        let deletions_only = DiffStats {
            insertions: 0,
            deletions: 3,
            files_changed: 1,
        };
        assert!(deletions_only.has_changes());

        let both = DiffStats {
            insertions: 10,
            deletions: 5,
            files_changed: 2,
        };
        assert!(both.has_changes());

        // Edge case: files_changed but no line counts
        // This can happen with binary files or certain edge cases
        let files_only = DiffStats {
            insertions: 0,
            deletions: 0,
            files_changed: 1,
        };
        // has_changes() only checks line counts, not files_changed
        assert!(
            !files_only.has_changes(),
            "has_changes() checks line counts only"
        );
    }
}
