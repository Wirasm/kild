use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::errors::ProjectError;

/// A project is a git repository where kilds can be created.
///
/// Projects are stored with canonical paths to ensure consistent hashing
/// for filtering. Use [`Project::new`] to create validated projects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    /// File system path to the repository root (canonical).
    path: PathBuf,
    /// Display name (defaults to directory name if not set).
    name: String,
}

impl Project {
    /// Create a new validated project with canonical path.
    ///
    /// This validates that the path is a git repository and canonicalizes it
    /// to ensure consistent hashing for filtering.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Path cannot be canonicalized (doesn't exist or is inaccessible)
    /// - Path is not a directory
    /// - Path is not a git repository
    pub fn new(path: PathBuf, name: Option<String>) -> Result<Self, ProjectError> {
        let canonical = path
            .canonicalize()
            .map_err(|source| ProjectError::CanonicalizationFailed { source })?;

        if !canonical.is_dir() {
            return Err(ProjectError::NotADirectory);
        }

        if !is_git_repo(&canonical)? {
            return Err(ProjectError::NotAGitRepo);
        }

        let name = name.unwrap_or_else(|| derive_display_name(&canonical));

        Ok(Self {
            path: canonical,
            name,
        })
    }

    /// Create a project without validation (for deserialization/migration/tests).
    ///
    /// Use [`Project::new`] when adding projects from user input.
    pub(crate) fn new_unchecked(path: PathBuf, name: String) -> Self {
        Self { path, name }
    }

    /// Get the project path (canonical if created via `new()`).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get the project name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Update the path to its canonical form (used during migration).
    ///
    /// # Errors
    ///
    /// Returns `ProjectError::CanonicalizationFailed` if the path cannot be resolved.
    pub(crate) fn canonicalize_path(&mut self) -> Result<bool, ProjectError> {
        let canonical = self
            .path
            .canonicalize()
            .map_err(|source| ProjectError::CanonicalizationFailed { source })?;
        if canonical != self.path {
            self.path = canonical;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Test utilities for creating projects without validation.
///
/// Public so downstream crates (kild-ui) can create test fixtures.
#[doc(hidden)]
pub mod test_helpers {
    use super::*;

    /// Create a project without validation (for test fixtures only).
    pub fn make_test_project(path: PathBuf, name: String) -> Project {
        Project::new_unchecked(path, name)
    }
}

/// Stored projects data (serialization DTO for `~/.kild/projects.json`).
///
/// This is a **data transfer type** for persistence only. Fields are public
/// for serialization and direct use by persistence/action code.
/// All business logic (invariant enforcement, active-index tracking) belongs
/// in [`super::ProjectManager`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectsData {
    pub projects: Vec<Project>,
    /// Path of the currently active project (None if no project selected).
    pub active: Option<PathBuf>,
    /// Error message if loading failed (file corrupted, unreadable, etc.).
    /// Transient — never serialized.
    #[serde(skip)]
    pub load_error: Option<String>,
}

/// Check if a path is a git repository using git2.
///
/// Uses `git2::Repository::discover` which handles standard repos, worktrees,
/// and bare repos. Returns `Ok(false)` if the path is not in a git repository.
///
/// # Errors
///
/// Returns `ProjectError::Git2CheckFailed` if the git2 library encounters an
/// unexpected error (e.g., permission denied). This is distinct from returning
/// `Ok(false)` which means "path is not a git repository".
pub fn is_git_repo(path: &Path) -> Result<bool, ProjectError> {
    match git2::Repository::discover(path) {
        Ok(_) => Ok(true),
        Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(false),
        Err(e) => {
            tracing::error!(
                event = "core.projects.git2_check_failed",
                path = %path.display(),
                error = %e,
                "Failed to check if path is a git repository"
            );
            Err(ProjectError::Git2CheckFailed { source: e })
        }
    }
}

/// Get a human-readable display name from a path.
///
/// Returns the final directory component, or "unknown" for edge cases like root "/".
pub fn derive_display_name(path: &Path) -> String {
    match path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name.to_string(),
        None => {
            tracing::warn!(
                event = "core.projects.derive_name_fallback",
                path = %path.display(),
                "Could not derive display name from path, using 'unknown'"
            );
            "unknown".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_git_repo_valid() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init failed");

        assert_eq!(is_git_repo(path).unwrap(), true);
    }

    #[test]
    fn test_is_git_repo_invalid() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        assert_eq!(is_git_repo(temp_dir.path()).unwrap(), false);
    }

    #[test]
    fn test_derive_display_name() {
        let path = PathBuf::from("/Users/test/Projects/my-project");
        assert_eq!(derive_display_name(&path), "my-project");
    }

    #[test]
    fn test_derive_display_name_root() {
        let path = PathBuf::from("/");
        assert_eq!(derive_display_name(&path), "unknown");
    }

    #[test]
    fn test_projects_data_serialization_roundtrip() {
        let data = ProjectsData {
            projects: vec![
                Project::new_unchecked(
                    PathBuf::from("/path/to/project-a"),
                    "Project A".to_string(),
                ),
                Project::new_unchecked(
                    PathBuf::from("/path/to/project-b"),
                    "Project B".to_string(),
                ),
            ],
            active: Some(PathBuf::from("/path/to/project-a")),
            load_error: None,
        };

        let json = serde_json::to_string(&data).expect("Failed to serialize");
        let loaded: ProjectsData = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(loaded.projects.len(), 2);
        assert_eq!(loaded.projects[0].path(), Path::new("/path/to/project-a"));
        assert_eq!(loaded.projects[0].name(), "Project A");
        assert_eq!(loaded.projects[1].path(), Path::new("/path/to/project-b"));
        assert_eq!(loaded.projects[1].name(), "Project B");
        assert_eq!(loaded.active, Some(PathBuf::from("/path/to/project-a")));
    }

    #[test]
    fn test_projects_data_default() {
        let data = ProjectsData::default();
        assert!(data.projects.is_empty());
        assert!(data.active.is_none());
    }

    #[test]
    fn test_project_equality() {
        let project1 =
            Project::new_unchecked(PathBuf::from("/path/to/project"), "Project".to_string());
        let project2 =
            Project::new_unchecked(PathBuf::from("/path/to/project"), "Project".to_string());
        let project3 =
            Project::new_unchecked(PathBuf::from("/different/path"), "Project".to_string());

        assert_eq!(project1, project2);
        assert_ne!(project1, project3);
    }

    #[test]
    fn test_path_canonicalization_consistency() {
        use crate::git::operations::generate_project_id;

        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path();

        let canonical1 = path.canonicalize().unwrap();
        let canonical2 = path.canonicalize().unwrap();
        assert_eq!(canonical1, canonical2);

        let id1 = generate_project_id(&canonical1);
        let id2 = generate_project_id(&canonical2);
        assert_eq!(
            id1, id2,
            "Same canonical path should produce same project ID"
        );
    }

    #[test]
    fn test_generate_project_id_different_for_non_canonical() {
        use crate::git::operations::generate_project_id;

        let path1 = PathBuf::from("/users/test/project");
        let path2 = PathBuf::from("/Users/test/project");

        let id1 = generate_project_id(&path1);
        let id2 = generate_project_id(&path2);

        assert_ne!(
            id1, id2,
            "Non-canonical paths produce different hashes (this is why canonicalization is needed)"
        );
    }

    #[test]
    fn test_generate_project_id_consistency() {
        use crate::git::operations::generate_project_id;

        let path = PathBuf::from("/Users/test/Projects/my-project");
        let id1 = generate_project_id(&path);
        let id2 = generate_project_id(&path);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_generate_project_id_different_paths() {
        use crate::git::operations::generate_project_id;

        let path1 = PathBuf::from("/Users/test/Projects/project-a");
        let path2 = PathBuf::from("/Users/test/Projects/project-b");
        assert_ne!(generate_project_id(&path1), generate_project_id(&path2));
    }

    #[test]
    fn test_generate_project_id_is_hex() {
        use crate::git::operations::generate_project_id;

        let path = PathBuf::from("/Users/test/Projects/my-project");
        let id = generate_project_id(&path);
        assert!(!id.is_empty());
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_filtering_works_after_path_canonicalization() {
        use crate::git::operations::generate_project_id;

        let temp_dir = tempfile::TempDir::new().unwrap();
        let canonical_path = temp_dir.path().canonicalize().unwrap();

        let id_from_canonical = generate_project_id(&canonical_path);
        let session_project_id = generate_project_id(&canonical_path);
        let active_project_id = generate_project_id(&canonical_path);

        assert_eq!(
            session_project_id, active_project_id,
            "Canonical paths should produce identical project IDs for filtering"
        );
        assert_eq!(id_from_canonical, session_project_id);
    }

    #[test]
    fn test_migration_handles_missing_paths_gracefully() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let existing_path = temp_dir.path().to_path_buf();
        let missing_path = PathBuf::from("/this/path/definitely/does/not/exist/anywhere");

        assert!(existing_path.canonicalize().is_ok());
        assert!(missing_path.canonicalize().is_err());

        let paths = vec![existing_path.clone(), missing_path.clone()];
        let mut results = Vec::new();

        for path in &paths {
            match path.canonicalize() {
                Ok(canonical) => results.push(("canonicalized", canonical)),
                Err(_) => results.push(("unchanged", path.clone())),
            }
        }

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "canonicalized");
        assert_eq!(results[1].0, "unchanged");
        assert_eq!(results[1].1, missing_path);
    }

    #[test]
    fn test_project_new_nonexistent_path() {
        let result = Project::new(PathBuf::from("/nonexistent/path/nowhere"), None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProjectError::CanonicalizationFailed { .. }
        ));
    }

    #[test]
    fn test_project_new_not_a_directory() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let file_path = temp_dir.path().join("a_file.txt");
        std::fs::write(&file_path, "content").unwrap();

        let result = Project::new(file_path, None);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProjectError::NotADirectory));
    }

    #[test]
    fn test_project_new_not_a_git_repo() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let result = Project::new(temp_dir.path().to_path_buf(), None);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProjectError::NotAGitRepo));
    }

    #[test]
    fn test_project_new_valid_git_repo() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .expect("git init failed");

        let result = Project::new(temp_dir.path().to_path_buf(), None);
        assert!(result.is_ok());
        let project = result.unwrap();
        // Name is derived from directory name
        assert!(!project.name().is_empty());
    }

    #[test]
    fn test_project_new_with_custom_name() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .expect("git init failed");

        let project = Project::new(
            temp_dir.path().to_path_buf(),
            Some("My Custom Name".to_string()),
        )
        .unwrap();
        assert_eq!(project.name(), "My Custom Name");
    }

    #[test]
    fn test_project_new_none_name_derives_from_path() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .expect("git init failed");

        let project = Project::new(temp_dir.path().to_path_buf(), None).unwrap();
        let expected_name = temp_dir
            .path()
            .canonicalize()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert_eq!(project.name(), expected_name);
    }

    #[test]
    fn test_project_new_canonicalizes_path() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .expect("git init failed");

        let project = Project::new(temp_dir.path().to_path_buf(), None).unwrap();
        let canonical = temp_dir.path().canonicalize().unwrap();
        assert_eq!(project.path(), canonical);
    }
}
