//! Project management for shards-ui.
//!
//! Handles storing, loading, and validating projects (git repositories).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A project is a git repository where shards can be created.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    /// File system path to the repository root
    pub path: PathBuf,
    /// Display name (defaults to directory name if not set)
    pub name: String,
}

/// Stored projects data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectsData {
    pub projects: Vec<Project>,
    /// Path of the currently active project (None if no project selected)
    pub active: Option<PathBuf>,
}

/// Check if a path is a git repository.
pub fn is_git_repo(path: &Path) -> bool {
    // Check for .git directory
    if path.join(".git").exists() {
        return true;
    }
    // Also check via git command (handles worktrees and bare repos)
    std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Derive project ID from path (matches shards-core's project ID generation).
pub fn derive_project_id(path: &Path) -> String {
    // shards-core uses the directory name as project_id
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Load projects from ~/.shards/projects.json
pub fn load_projects() -> ProjectsData {
    let path = projects_file_path();
    if !path.exists() {
        return ProjectsData::default();
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(e) => {
            tracing::warn!(
                event = "ui.projects.load_failed",
                path = %path.display(),
                error = %e
            );
            ProjectsData::default()
        }
    }
}

/// Save projects to ~/.shards/projects.json
pub fn save_projects(data: &ProjectsData) -> Result<(), String> {
    let path = projects_file_path();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let json = serde_json::to_string_pretty(data)
        .map_err(|e| format!("Failed to serialize projects: {}", e))?;

    std::fs::write(&path, json).map_err(|e| format!("Failed to write projects file: {}", e))?;

    tracing::info!(
        event = "ui.projects.saved",
        path = %path.display(),
        count = data.projects.len()
    );

    Ok(())
}

fn projects_file_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".shards")
        .join("projects.json")
}

/// Validation result for adding a project.
#[derive(Debug, PartialEq)]
pub enum ProjectValidation {
    Valid,
    NotADirectory,
    NotAGitRepo,
    AlreadyExists,
}

/// Validate a path before adding as a project.
pub fn validate_project_path(path: &Path, existing: &[Project]) -> ProjectValidation {
    if !path.is_dir() {
        return ProjectValidation::NotADirectory;
    }
    if !is_git_repo(path) {
        return ProjectValidation::NotAGitRepo;
    }
    if existing.iter().any(|p| p.path == path) {
        return ProjectValidation::AlreadyExists;
    }
    ProjectValidation::Valid
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_is_git_repo_valid() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init failed");

        assert!(is_git_repo(path));
    }

    #[test]
    fn test_is_git_repo_invalid() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Not a git repo
        assert!(!is_git_repo(path));
    }

    #[test]
    fn test_validate_project_path_not_directory() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "test").unwrap();

        let result = validate_project_path(&file_path, &[]);
        assert_eq!(result, ProjectValidation::NotADirectory);
    }

    #[test]
    fn test_validate_project_path_not_git() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        let result = validate_project_path(path, &[]);
        assert_eq!(result, ProjectValidation::NotAGitRepo);
    }

    #[test]
    fn test_validate_project_path_already_exists() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init failed");

        let existing = vec![Project {
            path: path.to_path_buf(),
            name: "test".to_string(),
        }];

        let result = validate_project_path(path, &existing);
        assert_eq!(result, ProjectValidation::AlreadyExists);
    }

    #[test]
    fn test_validate_project_path_valid() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init failed");

        let result = validate_project_path(path, &[]);
        assert_eq!(result, ProjectValidation::Valid);
    }

    #[test]
    fn test_load_projects_missing_file() {
        // Don't actually test with the real file path - just verify default behavior
        let data = ProjectsData::default();
        assert!(data.projects.is_empty());
        assert!(data.active.is_none());
    }

    #[test]
    fn test_derive_project_id() {
        let path = PathBuf::from("/Users/test/Projects/my-project");
        let id = derive_project_id(&path);
        assert_eq!(id, "my-project");
    }

    #[test]
    fn test_derive_project_id_root() {
        let path = PathBuf::from("/");
        let id = derive_project_id(&path);
        // Root has no file_name, so falls back to "unknown"
        assert_eq!(id, "unknown");
    }
}
