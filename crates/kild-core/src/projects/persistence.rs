use std::path::PathBuf;

use super::errors::ProjectError;
use super::types::ProjectsData;

/// Load projects from ~/.kild/projects.json.
///
/// Falls back to `./.kild/projects.json` if home directory cannot be determined.
/// Returns default empty state if file doesn't exist or is corrupted (with error logged).
pub fn load_projects() -> ProjectsData {
    let path = projects_file_path();
    if !path.exists() {
        return ProjectsData::default();
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(data) => data,
            Err(e) => {
                // ERROR (not warn): file exists but is corrupted — indicates
                // data loss or external tampering, requires user action.
                tracing::error!(
                    event = "core.projects.json_parse_failed",
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
            // ERROR (not warn): file exists but can't be read — likely a
            // permission issue or filesystem problem requiring user action.
            tracing::error!(
                event = "core.projects.load_failed",
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
pub fn save_projects(data: &ProjectsData) -> Result<(), ProjectError> {
    let path = projects_file_path();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ProjectError::SaveFailed {
            message: format!("Failed to create directory ({}): {}", parent.display(), e),
        })?;
    }

    let json = serde_json::to_string_pretty(data).map_err(|e| ProjectError::SaveFailed {
        message: format!("Failed to serialize projects: {}", e),
    })?;

    std::fs::write(&path, json).map_err(|e| ProjectError::SaveFailed {
        message: format!("Failed to write projects file ({}): {}", path.display(), e),
    })?;

    tracing::info!(
        event = "core.projects.saved",
        path = %path.display(),
        count = data.projects.len()
    );

    Ok(())
}

/// Migrate existing stored projects to use canonical paths.
///
/// This fixes a historical issue where paths were stored without canonicalization,
/// causing case mismatches on macOS.
///
/// Called once on app startup to fix existing project paths.
pub fn migrate_projects_to_canonical() -> Result<(), ProjectError> {
    let mut data = load_projects();
    let mut changed = false;

    for project in &mut data.projects {
        let original_path = project.path().to_path_buf();
        match project.canonicalize_path() {
            Ok(did_change) => {
                if did_change {
                    tracing::info!(
                        event = "core.projects.path_migrated",
                        original = %original_path.display(),
                        canonical = %project.path().display()
                    );
                    changed = true;
                }
            }
            Err(e) => {
                // WARN (not error): expected during migration when projects live
                // on disconnected drives or paths have been deleted.
                tracing::warn!(
                    event = "core.projects.path_canonicalize_failed",
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
                        event = "core.projects.active_path_migrated",
                        original = %active.display(),
                        canonical = %canonical.display()
                    );
                    data.active = Some(canonical);
                    changed = true;
                }
            }
            Err(e) => {
                // WARN (not error): the active project path may no longer exist
                // (e.g., drive disconnected). Clearing selection is safe recovery.
                tracing::warn!(
                    event = "core.projects.active_path_canonicalize_failed",
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
    if let Ok(path_str) = std::env::var("KILD_PROJECTS_FILE")
        && !path_str.is_empty()
    {
        return PathBuf::from(path_str);
    }

    match dirs::home_dir() {
        Some(home) => home.join(".kild").join("projects.json"),
        None => {
            tracing::error!(
                event = "core.projects.home_dir_not_found",
                fallback = ".",
                "Could not determine home directory - using current directory as fallback"
            );
            PathBuf::from(".").join(".kild").join("projects.json")
        }
    }
}

/// Test utilities for projects persistence.
///
/// Public so downstream crates (kild-ui) can use the env lock/guard in their tests.
#[doc(hidden)]
pub mod test_helpers {
    use std::sync::Mutex;

    /// Mutex to serialize tests that modify KILD_PROJECTS_FILE env var.
    pub static PROJECTS_FILE_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// RAII guard that removes KILD_PROJECTS_FILE env var on drop.
    pub struct ProjectsFileEnvGuard;

    impl ProjectsFileEnvGuard {
        pub fn new(path: &std::path::Path) -> Self {
            // SAFETY: Caller must hold PROJECTS_FILE_ENV_LOCK to serialize access
            // from Rust test code. This is inherently unsafe as other threads or
            // C code could read the environment, but acceptable in test-only code.
            unsafe { std::env::set_var("KILD_PROJECTS_FILE", path) };
            Self
        }
    }

    impl Drop for ProjectsFileEnvGuard {
        fn drop(&mut self) {
            // SAFETY: Caller must hold PROJECTS_FILE_ENV_LOCK throughout guard
            // lifetime. See safety comment in new().
            unsafe { std::env::remove_var("KILD_PROJECTS_FILE") };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
    use super::*;
    use crate::projects::types::Project;
    use tempfile::TempDir;

    #[test]
    fn test_load_projects_missing_file() {
        let data = ProjectsData::default();
        assert!(data.projects.is_empty());
        assert!(data.active.is_none());
    }

    #[test]
    fn test_projects_file_path_env_override() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let custom_path = temp_dir.path().join("custom_projects.json");

        let _guard = ProjectsFileEnvGuard::new(&custom_path);

        let path = super::projects_file_path();
        assert_eq!(path, custom_path);
    }

    #[test]
    fn test_projects_file_path_default_after_cleanup() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();

        // SAFETY: We hold PROJECTS_FILE_ENV_LOCK to serialize test access
        unsafe { std::env::remove_var("KILD_PROJECTS_FILE") };

        let default_path = super::projects_file_path();
        assert!(default_path.ends_with("projects.json"));
        assert!(default_path.to_string_lossy().contains(".kild"));
    }

    #[test]
    fn test_projects_file_path_empty_env_var_uses_default() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();

        // SAFETY: We hold PROJECTS_FILE_ENV_LOCK to serialize test access
        unsafe { std::env::set_var("KILD_PROJECTS_FILE", "") };

        let path = super::projects_file_path();
        assert!(path.ends_with("projects.json"));
        assert!(path.to_string_lossy().contains(".kild"));

        // SAFETY: We hold PROJECTS_FILE_ENV_LOCK to serialize test access
        unsafe { std::env::remove_var("KILD_PROJECTS_FILE") };
    }

    #[test]
    fn test_load_and_save_with_env_override() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let custom_path = temp_dir.path().join("custom_projects.json");
        let _guard = ProjectsFileEnvGuard::new(&custom_path);

        let mut data = ProjectsData::default();
        data.projects.push(Project::new_unchecked(
            PathBuf::from("/test/path"),
            "Test Project".to_string(),
        ));

        save_projects(&data).expect("save should succeed");

        assert!(custom_path.exists(), "File should exist at custom path");

        let loaded = load_projects();
        assert_eq!(loaded.projects.len(), 1);
        assert_eq!(loaded.projects[0].name(), "Test Project");
    }

    #[test]
    fn test_load_corrupted_json_returns_default_with_error() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let custom_path = temp_dir.path().join("corrupted.json");
        std::fs::write(&custom_path, "{ this is not valid json }").unwrap();
        let _guard = ProjectsFileEnvGuard::new(&custom_path);

        let data = load_projects();
        assert!(data.projects.is_empty());
        assert!(data.active.is_none());
        assert!(data.load_error.is_some());
        assert!(data.load_error.unwrap().contains("corrupted"));
    }

    #[test]
    fn test_load_unreadable_file_returns_default_with_error() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let custom_path = temp_dir.path().join("projects.json");
        // Create a directory where a file is expected — causes a read error
        std::fs::create_dir_all(&custom_path).unwrap();
        let _guard = ProjectsFileEnvGuard::new(&custom_path);

        let data = load_projects();
        assert!(data.projects.is_empty());
        assert!(data.load_error.is_some());
    }

    #[test]
    fn test_save_projects_creates_parent_directory_for_env_override() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let custom_path = temp_dir.path().join("subdir").join("projects.json");
        let _guard = ProjectsFileEnvGuard::new(&custom_path);

        let data = ProjectsData::default();
        let result = save_projects(&data);

        assert!(result.is_ok(), "Should create parent directory");
        assert!(custom_path.exists());
    }
}
