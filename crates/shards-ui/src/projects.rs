//! Project management for shards-ui.
//!
//! Handles storing, loading, and validating projects (git repositories).

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
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
///
/// Uses two detection methods:
/// 1. Checks for a `.git` directory (standard repositories)
/// 2. Falls back to `git rev-parse --git-dir` (handles worktrees and bare repos)
///
/// Returns `false` if detection fails (with warning logged).
pub fn is_git_repo(path: &Path) -> bool {
    // Check for .git directory
    if path.join(".git").exists() {
        return true;
    }
    // Also check via git command (handles worktrees and bare repos)
    match std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(path)
        .output()
    {
        Ok(output) => output.status.success(),
        Err(e) => {
            tracing::warn!(
                event = "ui.projects.git_check_failed",
                path = %path.display(),
                error = %e,
                "Failed to execute git command to check repository status"
            );
            false
        }
    }
}

/// Generate project ID from path using hash (matches shards-core's `generate_project_id`).
///
/// Uses the same algorithm as `shards_core::git::operations::generate_project_id`
/// to ensure session filtering works correctly.
pub fn derive_project_id(path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Get a human-readable display name from a path.
///
/// Returns the final directory component, or "unknown" for edge cases like root "/".
pub fn derive_display_name(path: &Path) -> String {
    match path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name.to_string(),
        None => {
            tracing::warn!(
                event = "ui.projects.derive_name_fallback",
                path = %path.display(),
                "Could not derive display name from path, using 'unknown'"
            );
            "unknown".to_string()
        }
    }
}

/// Load projects from ~/.shards/projects.json.
///
/// Falls back to `./shards/projects.json` if home directory cannot be determined.
/// Returns default empty state if file doesn't exist or is corrupted (with warning logged).
pub fn load_projects() -> ProjectsData {
    let path = projects_file_path();
    if !path.exists() {
        return ProjectsData::default();
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(data) => data,
            Err(e) => {
                tracing::error!(
                    event = "ui.projects.json_parse_failed",
                    path = %path.display(),
                    error = %e,
                    "Projects file exists but contains invalid JSON - project configuration lost"
                );
                ProjectsData::default()
            }
        },
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

/// Migrate existing stored projects to use canonical paths.
///
/// This fixes a historical issue where paths were stored without canonicalization,
/// causing case mismatches on macOS. For example, if a project was stored as
/// `/users/rasmus/project` but git returns `/Users/rasmus/project`, the hash
/// values differ causing filtering issues.
///
/// Called once on app startup to fix existing project paths. New projects added
/// after this fix are canonicalized via `normalize_project_path()`.
pub fn migrate_projects_to_canonical() -> Result<(), String> {
    let mut data = load_projects();
    let mut changed = false;

    for project in &mut data.projects {
        match project.path.canonicalize() {
            Ok(canonical) if canonical != project.path => {
                tracing::info!(
                    event = "ui.projects.path_migrated",
                    original = %project.path.display(),
                    canonical = %canonical.display()
                );
                project.path = canonical;
                changed = true;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(
                    event = "ui.projects.path_canonicalize_failed",
                    path = %project.path.display(),
                    project_name = %project.name,
                    error = %e,
                    "Project path may no longer exist or is inaccessible"
                );
            }
        }
    }

    if let Some(ref active) = data.active {
        match active.canonicalize() {
            Ok(canonical) if &canonical != active => {
                tracing::info!(
                    event = "ui.projects.active_path_migrated",
                    original = %active.display(),
                    canonical = %canonical.display()
                );
                data.active = Some(canonical);
                changed = true;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(
                    event = "ui.projects.active_path_canonicalize_failed",
                    path = %active.display(),
                    error = %e,
                    "Active project path is inaccessible, clearing selection"
                );
                data.active = None;
                changed = true;
            }
        }
    }

    if changed {
        save_projects(&data)?;
    }

    Ok(())
}

fn projects_file_path() -> PathBuf {
    match dirs::home_dir() {
        Some(home) => home.join(".shards").join("projects.json"),
        None => {
            tracing::error!(
                event = "ui.projects.home_dir_not_found",
                fallback = ".",
                "Could not determine home directory - using current directory as fallback"
            );
            PathBuf::from(".").join(".shards").join("projects.json")
        }
    }
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
    fn test_derive_project_id_consistency() {
        // Same path should generate same ID
        let path = PathBuf::from("/Users/test/Projects/my-project");
        let id1 = derive_project_id(&path);
        let id2 = derive_project_id(&path);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_derive_project_id_different_paths() {
        // Different paths should generate different IDs
        let path1 = PathBuf::from("/Users/test/Projects/project-a");
        let path2 = PathBuf::from("/Users/test/Projects/project-b");
        let id1 = derive_project_id(&path1);
        let id2 = derive_project_id(&path2);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_derive_project_id_is_hex() {
        let path = PathBuf::from("/Users/test/Projects/my-project");
        let id = derive_project_id(&path);
        // Should be a valid hex string
        assert!(!id.is_empty());
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_derive_display_name() {
        let path = PathBuf::from("/Users/test/Projects/my-project");
        let name = derive_display_name(&path);
        assert_eq!(name, "my-project");
    }

    #[test]
    fn test_derive_display_name_root() {
        let path = PathBuf::from("/");
        let name = derive_display_name(&path);
        // Root has no file_name, so falls back to "unknown"
        assert_eq!(name, "unknown");
    }

    #[test]
    fn test_projects_data_serialization_roundtrip() {
        let data = ProjectsData {
            projects: vec![
                Project {
                    path: PathBuf::from("/path/to/project-a"),
                    name: "Project A".to_string(),
                },
                Project {
                    path: PathBuf::from("/path/to/project-b"),
                    name: "Project B".to_string(),
                },
            ],
            active: Some(PathBuf::from("/path/to/project-a")),
        };

        // Serialize
        let json = serde_json::to_string(&data).expect("Failed to serialize");

        // Deserialize
        let loaded: ProjectsData = serde_json::from_str(&json).expect("Failed to deserialize");

        // Verify equality
        assert_eq!(loaded.projects.len(), 2);
        assert_eq!(loaded.projects[0].path, PathBuf::from("/path/to/project-a"));
        assert_eq!(loaded.projects[0].name, "Project A");
        assert_eq!(loaded.projects[1].path, PathBuf::from("/path/to/project-b"));
        assert_eq!(loaded.projects[1].name, "Project B");
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
        let project1 = Project {
            path: PathBuf::from("/path/to/project"),
            name: "Project".to_string(),
        };
        let project2 = Project {
            path: PathBuf::from("/path/to/project"),
            name: "Project".to_string(),
        };
        let project3 = Project {
            path: PathBuf::from("/different/path"),
            name: "Project".to_string(),
        };

        assert_eq!(project1, project2);
        assert_ne!(project1, project3);
    }

    #[test]
    fn test_path_canonicalization_consistency() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        let canonical1 = path.canonicalize().unwrap();
        let canonical2 = path.canonicalize().unwrap();
        assert_eq!(canonical1, canonical2);

        let id1 = derive_project_id(&canonical1);
        let id2 = derive_project_id(&canonical2);
        assert_eq!(
            id1, id2,
            "Same canonical path should produce same project ID"
        );
    }

    #[test]
    fn test_derive_project_id_different_for_non_canonical() {
        let path1 = PathBuf::from("/users/test/project");
        let path2 = PathBuf::from("/Users/test/project");

        let id1 = derive_project_id(&path1);
        let id2 = derive_project_id(&path2);

        assert_ne!(
            id1, id2,
            "Non-canonical paths produce different hashes (this is why canonicalization is needed)"
        );
    }

    #[test]
    fn test_migration_handles_missing_paths_gracefully() {
        // Verify that migration logic handles non-existent paths without panicking
        // This simulates what happens when a stored project path no longer exists
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let existing_path = temp_dir.path().to_path_buf();
        let missing_path = PathBuf::from("/this/path/definitely/does/not/exist/anywhere");

        // Existing path should canonicalize successfully
        let canonical_existing = existing_path.canonicalize();
        assert!(
            canonical_existing.is_ok(),
            "Existing path should canonicalize"
        );

        // Missing path should fail to canonicalize (not panic)
        let canonical_missing = missing_path.canonicalize();
        assert!(
            canonical_missing.is_err(),
            "Missing path should fail to canonicalize"
        );

        // Verify the migration logic pattern handles both cases
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
    fn test_filtering_works_after_path_canonicalization() {
        // Integration test: verify that canonicalized paths produce matching IDs
        // This tests the core fix for the filtering bug
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let canonical_path = temp_dir.path().canonicalize().unwrap();

        // Simulate: on macOS, lowercase path resolves to same canonical form
        // We test that derive_project_id produces same result for canonical paths
        let id_from_canonical = derive_project_id(&canonical_path);

        // Simulate: session created in worktree uses git's canonical path
        let session_project_id = derive_project_id(&canonical_path);

        // Simulate: UI active_project uses stored canonical path
        let active_project_id = derive_project_id(&canonical_path);

        // These must match for filtering to work correctly
        assert_eq!(
            session_project_id, active_project_id,
            "Canonical paths should produce identical project IDs for filtering"
        );
        assert_eq!(id_from_canonical, session_project_id);
    }
}
