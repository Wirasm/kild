use std::path::PathBuf;

/// Git diff statistics for a worktree.
///
/// Represents the number of lines added, removed, and files changed
/// between the index (staging area) and the working directory.
/// This captures **unstaged changes only**, not staged changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiffStats {
    /// Number of lines added
    pub insertions: usize,
    /// Number of lines removed
    pub deletions: usize,
    /// Number of files changed
    pub files_changed: usize,
}

impl DiffStats {
    /// Returns true if there are any line changes.
    pub fn has_changes(&self) -> bool {
        self.insertions > 0 || self.deletions > 0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub project_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectInfo {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub remote_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub exists: bool,
    pub is_current: bool,
}

impl WorktreeInfo {
    pub fn new(path: PathBuf, branch: String, project_id: String) -> Self {
        Self {
            path,
            branch,
            project_id,
        }
    }
}

/// Comprehensive worktree status for destroy safety checks.
///
/// Contains information about uncommitted changes, unpushed commits,
/// and remote branch existence to help users make informed decisions
/// before destroying a kild.
#[derive(Debug, Clone, Default)]
pub struct WorktreeStatus {
    /// Whether there are uncommitted changes (staged, modified, or untracked).
    pub has_uncommitted_changes: bool,
    /// Number of commits ahead of the remote tracking branch.
    /// Zero if the branch tracks a remote and is up-to-date, or if there's no remote.
    pub unpushed_commit_count: usize,
    /// Whether a remote tracking branch exists for this branch.
    /// False means the branch has never been pushed.
    pub has_remote_branch: bool,
    /// Details about uncommitted changes (file counts by category).
    pub uncommitted_details: Option<UncommittedDetails>,
}

/// Detailed breakdown of uncommitted changes.
#[derive(Debug, Clone, Default)]
pub struct UncommittedDetails {
    /// Number of files staged for commit.
    pub staged_files: usize,
    /// Number of tracked files with unstaged modifications.
    pub modified_files: usize,
    /// Number of untracked files.
    pub untracked_files: usize,
}

impl ProjectInfo {
    pub fn new(id: String, name: String, path: PathBuf, remote_url: Option<String>) -> Self {
        Self {
            id,
            name,
            path,
            remote_url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worktree_info() {
        let worktree = WorktreeInfo::new(
            PathBuf::from("/tmp/test"),
            "feature-branch".to_string(),
            "test-project".to_string(),
        );

        assert_eq!(worktree.branch, "feature-branch");
        assert_eq!(worktree.project_id, "test-project");
        assert_eq!(worktree.path, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn test_worktree_info_preserves_original_branch_name() {
        // WorktreeInfo stores the original branch name (with slashes),
        // not the sanitized version used for the worktree path/directory.
        // This ensures git operations use the correct branch name.
        let original_branch = "feature/auth";
        let sanitized_path = PathBuf::from("/tmp/worktrees/project/feature-auth");

        let info = WorktreeInfo::new(
            sanitized_path,
            original_branch.to_string(),
            "test-project".to_string(),
        );

        // Original branch name with slash is preserved
        assert_eq!(info.branch, "feature/auth");
        assert_ne!(info.branch, "feature-auth");
    }

    #[test]
    fn test_project_info() {
        let project = ProjectInfo::new(
            "test-id".to_string(),
            "test-project".to_string(),
            PathBuf::from("/path/to/project"),
            Some("https://github.com/user/repo.git".to_string()),
        );

        assert_eq!(project.id, "test-id");
        assert_eq!(project.name, "test-project");
        assert_eq!(
            project.remote_url,
            Some("https://github.com/user/repo.git".to_string())
        );
    }

    #[test]
    fn test_branch_info() {
        let branch = BranchInfo {
            name: "main".to_string(),
            exists: true,
            is_current: true,
        };

        assert_eq!(branch.name, "main");
        assert!(branch.exists);
        assert!(branch.is_current);
    }
}
