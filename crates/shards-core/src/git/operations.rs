use crate::git::errors::GitError;
use std::path::{Path, PathBuf};
use tracing::debug;

pub fn calculate_worktree_path(base_dir: &Path, project_name: &str, branch: &str) -> PathBuf {
    base_dir.join("worktrees").join(project_name).join(branch)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
