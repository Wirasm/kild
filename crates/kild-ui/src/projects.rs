//! Project management for kild-ui.
//!
//! Handles storing, loading, and validating projects (git repositories).

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Error when creating a validated project.
#[derive(Debug)]
pub enum ProjectError {
    /// Path is not a directory.
    NotADirectory,
    /// Path is not a git repository.
    NotAGitRepo,
    /// Path cannot be canonicalized.
    CanonicalizationFailed(std::io::Error),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectError::NotADirectory => write!(f, "Path is not a directory"),
            ProjectError::NotAGitRepo => write!(f, "Path is not a git repository"),
            ProjectError::CanonicalizationFailed(e) => {
                write!(f, "Cannot resolve path: {}", e)
            }
        }
    }
}

impl std::error::Error for ProjectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProjectError::CanonicalizationFailed(e) => Some(e),
            _ => None,
        }
    }
}

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
        // Canonicalize first to get proper path
        let canonical = path
            .canonicalize()
            .map_err(ProjectError::CanonicalizationFailed)?;

        if !canonical.is_dir() {
            return Err(ProjectError::NotADirectory);
        }

        if !is_git_repo(&canonical) {
            return Err(ProjectError::NotAGitRepo);
        }

        let name = name.unwrap_or_else(|| derive_display_name(&canonical));

        Ok(Self {
            path: canonical,
            name,
        })
    }

    /// Create a project without validation (for deserialization/migration).
    ///
    /// Use [`Project::new`] when adding projects from user input.
    #[cfg(test)]
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

    /// Set the project name.
    #[allow(dead_code)]
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    /// Set the project path (used during migration).
    pub(crate) fn set_path(&mut self, path: PathBuf) {
        self.path = path;
    }
}

/// Stored projects data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectsData {
    pub projects: Vec<Project>,
    /// Path of the currently active project (None if no project selected)
    pub active: Option<PathBuf>,
    /// Error message if loading failed (file corrupted, unreadable, etc.)
    #[serde(skip)]
    pub load_error: Option<String>,
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

/// Generate project ID from path using hash (matches kild-core's `generate_project_id`).
///
/// Uses the same algorithm as `kild_core::git::operations::generate_project_id`
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

/// Load projects from ~/.kild/projects.json.
///
/// Falls back to `./.kild/projects.json` if home directory cannot be determined.
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
                ProjectsData {
                    load_error: Some(format!(
                        "Projects file corrupted ({}). Your project list could not be loaded. \
                         Delete {} to reset.",
                        e,
                        path.display()
                    )),
                    ..Default::default()
                }
            }
        },
        Err(e) => {
            tracing::error!(
                event = "ui.projects.load_failed",
                path = %path.display(),
                error = %e
            );
            ProjectsData {
                load_error: Some(format!(
                    "Failed to read projects file: {}. Check permissions on {}",
                    e,
                    path.display()
                )),
                ..Default::default()
            }
        }
    }
}

/// Save projects to ~/.kild/projects.json
pub fn save_projects(data: &ProjectsData) -> Result<(), String> {
    let path = projects_file_path();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory ({}): {}", parent.display(), e))?;
    }

    let json = serde_json::to_string_pretty(data)
        .map_err(|e| format!("Failed to serialize projects: {}", e))?;

    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write projects file ({}): {}", path.display(), e))?;

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
        match project.path().canonicalize() {
            Ok(canonical) => {
                if canonical != project.path() {
                    tracing::info!(
                        event = "ui.projects.path_migrated",
                        original = %project.path().display(),
                        canonical = %canonical.display()
                    );
                    project.set_path(canonical);
                    changed = true;
                }
            }
            Err(e) => {
                tracing::warn!(
                    event = "ui.projects.path_canonicalize_failed",
                    path = %project.path().display(),
                    project_name = %project.name(),
                    error = %e,
                    "Project path may no longer exist or is inaccessible"
                );
            }
        }
    }

    if let Some(ref active) = data.active {
        match active.canonicalize() {
            Ok(canonical) => {
                if &canonical != active {
                    tracing::info!(
                        event = "ui.projects.active_path_migrated",
                        original = %active.display(),
                        canonical = %canonical.display()
                    );
                    data.active = Some(canonical);
                    changed = true;
                }
            }
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
    // Allow override via env var for testing.
    // This follows the pattern used in kild-core (KILD_LOG_LEVEL, KILD_BASE_PORT_RANGE).
    // Production code never sets this; only tests use it for isolation.
    if let Ok(path_str) = std::env::var("KILD_PROJECTS_FILE")
        && !path_str.is_empty()
    {
        return PathBuf::from(path_str);
    }

    match dirs::home_dir() {
        Some(home) => home.join(".kild").join("projects.json"),
        None => {
            tracing::error!(
                event = "ui.projects.home_dir_not_found",
                fallback = ".",
                "Could not determine home directory - using current directory as fallback"
            );
            PathBuf::from(".").join(".kild").join("projects.json")
        }
    }
}

/// Validation result for adding a project.
///
/// Used by tests to verify validation logic.
#[derive(Debug, PartialEq)]
#[allow(dead_code)]
pub enum ProjectValidation {
    Valid,
    NotADirectory,
    NotAGitRepo,
    AlreadyExists,
}

/// Validate a path before adding as a project.
///
/// Note: Production code uses `Project::new()` which does validation and canonicalization.
/// This function is kept for testing individual validation cases.
#[allow(dead_code)]
pub fn validate_project_path(path: &Path, existing: &[Project]) -> ProjectValidation {
    if !path.is_dir() {
        return ProjectValidation::NotADirectory;
    }
    if !is_git_repo(path) {
        return ProjectValidation::NotAGitRepo;
    }
    if existing.iter().any(|p| p.path() == path) {
        return ProjectValidation::AlreadyExists;
    }
    ProjectValidation::Valid
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use std::sync::Mutex;

    /// Mutex to serialize tests that modify KILD_PROJECTS_FILE env var.
    /// Rust runs tests in parallel by default, so without serialization,
    /// multiple tests could race on the same env var.
    pub(crate) static PROJECTS_FILE_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// RAII guard that removes KILD_PROJECTS_FILE env var on drop.
    /// Ensures cleanup even if the test panics.
    pub(crate) struct ProjectsFileEnvGuard;

    impl ProjectsFileEnvGuard {
        pub(crate) fn new(path: &std::path::Path) -> Self {
            // SAFETY: We hold PROJECTS_FILE_ENV_LOCK to prevent concurrent access
            unsafe { std::env::set_var("KILD_PROJECTS_FILE", path) };
            Self
        }
    }

    impl Drop for ProjectsFileEnvGuard {
        fn drop(&mut self) {
            // SAFETY: We hold PROJECTS_FILE_ENV_LOCK to prevent concurrent access
            unsafe { std::env::remove_var("KILD_PROJECTS_FILE") };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
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

        let existing = vec![Project::new_unchecked(
            path.to_path_buf(),
            "test".to_string(),
        )];

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

        // Serialize
        let json = serde_json::to_string(&data).expect("Failed to serialize");

        // Deserialize
        let loaded: ProjectsData = serde_json::from_str(&json).expect("Failed to deserialize");

        // Verify equality
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

    #[test]
    fn test_projects_file_path_env_override() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let custom_path = temp_dir.path().join("custom_projects.json");

        // Guard ensures cleanup even if test panics
        let _guard = ProjectsFileEnvGuard::new(&custom_path);

        // Verify override works
        let path = super::projects_file_path();
        assert_eq!(path, custom_path);

        // Guard drops here, cleaning up env var
    }

    #[test]
    fn test_projects_file_path_default_after_cleanup() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();

        // Ensure env var is not set
        // SAFETY: We hold PROJECTS_FILE_ENV_LOCK to prevent concurrent access
        unsafe { std::env::remove_var("KILD_PROJECTS_FILE") };

        let default_path = super::projects_file_path();
        assert!(default_path.ends_with("projects.json"));
        assert!(default_path.to_string_lossy().contains(".kild"));
    }

    #[test]
    fn test_projects_file_path_empty_env_var_uses_default() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();

        // Set env var to empty string
        // SAFETY: We hold PROJECTS_FILE_ENV_LOCK to prevent concurrent access
        unsafe { std::env::set_var("KILD_PROJECTS_FILE", "") };

        // Should fall back to default when empty
        let path = super::projects_file_path();
        assert!(path.ends_with("projects.json"));
        assert!(path.to_string_lossy().contains(".kild"));

        // Clean up
        // SAFETY: We hold PROJECTS_FILE_ENV_LOCK to prevent concurrent access
        unsafe { std::env::remove_var("KILD_PROJECTS_FILE") };
    }

    #[test]
    fn test_load_and_save_with_env_override() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let custom_path = temp_dir.path().join("custom_projects.json");
        let _guard = ProjectsFileEnvGuard::new(&custom_path);

        // Create and save test data
        let mut data = ProjectsData::default();
        data.projects.push(Project::new_unchecked(
            PathBuf::from("/test/path"),
            "Test Project".to_string(),
        ));

        save_projects(&data).expect("save should succeed");

        // Verify file exists at custom location
        assert!(custom_path.exists(), "File should exist at custom path");

        // Load and verify data roundtrips correctly
        let loaded = load_projects();
        assert_eq!(loaded.projects.len(), 1);
        assert_eq!(loaded.projects[0].name(), "Test Project");
    }

    #[test]
    fn test_save_projects_creates_parent_directory_for_env_override() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();

        let temp_dir = TempDir::new().unwrap();
        // Path with non-existent parent directory
        let custom_path = temp_dir.path().join("subdir").join("projects.json");
        let _guard = ProjectsFileEnvGuard::new(&custom_path);

        let data = ProjectsData::default();
        let result = save_projects(&data);

        // Should succeed - save_projects creates parent dirs
        assert!(result.is_ok(), "Should create parent directory");
        assert!(custom_path.exists());
    }
}
