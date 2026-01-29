use std::path::PathBuf;

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
