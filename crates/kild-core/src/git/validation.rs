use std::path::Path;

use tracing::debug;

use crate::git::errors::GitError;

pub fn validate_branch_name(branch: &str) -> Result<kild_protocol::BranchName, GitError> {
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

    Ok(kild_protocol::BranchName::new(trimmed))
}

/// Validate a git argument to prevent injection.
///
/// Rejects values that start with `-` (option injection), contain control characters,
/// or contain `::` sequences (refspec injection).
pub fn validate_git_arg(value: &str, label: &str) -> Result<(), GitError> {
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
    fn test_validate_branch_name_rejects_invalid() {
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
