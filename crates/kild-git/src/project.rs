use git2::Repository;
use std::path::Path;
use tracing::{debug, info};

use crate::{errors::GitError, naming, types::*};

pub fn detect_project() -> Result<GitProjectState, GitError> {
    info!(event = "core.git.project.detect_started");

    let current_dir = std::env::current_dir().map_err(|e| GitError::IoError { source: e })?;

    let repo = Repository::discover(&current_dir).map_err(|_| GitError::NotInRepository)?;

    let repo_path = repo.workdir().ok_or_else(|| GitError::OperationFailed {
        message: "Repository has no working directory".to_string(),
    })?;

    let remote_url = repo
        .find_remote("origin")
        .ok()
        .and_then(|remote| remote.url().map(|s| s.to_string()));

    let project_name = if let Some(ref url) = remote_url {
        naming::derive_project_name_from_remote(url)
    } else {
        naming::derive_project_name_from_path(repo_path)
    };

    let project_id = naming::generate_project_id(repo_path);

    let project = GitProjectState::new(
        project_id.to_string(),
        project_name.clone(),
        repo_path.to_path_buf(),
        remote_url.clone(),
    );

    info!(
        event = "core.git.project.detect_completed",
        project_id = %project_id,
        project_name = project_name,
        repo_path = %repo_path.display(),
        remote_url = remote_url.as_deref().unwrap_or("none")
    );

    Ok(project)
}

/// Detect project from a specific path (for UI usage).
///
/// Unlike `detect_project()` which uses current directory, this function
/// uses the provided path to discover the git repository. The path can be
/// anywhere within the repository - `Repository::discover` will walk up
/// the directory tree to find the repository root.
///
/// # Errors
///
/// Returns `GitError::NotInRepository` if the path is not within a git repository.
/// Returns `GitError::OperationFailed` if the repository has no working directory (bare repo).
pub fn detect_project_at(path: &Path) -> Result<GitProjectState, GitError> {
    info!(event = "core.git.project.detect_at_started", path = %path.display());

    let repo = Repository::discover(path).map_err(|e| {
        debug!(
            event = "core.git.project.discover_failed",
            path = %path.display(),
            error = %e,
            "Repository discovery failed - path may not be in a git repository"
        );
        GitError::NotInRepository
    })?;

    let repo_path = repo.workdir().ok_or_else(|| GitError::OperationFailed {
        message: "Repository has no working directory".to_string(),
    })?;

    let remote_url = repo
        .find_remote("origin")
        .ok()
        .and_then(|remote| remote.url().map(|s| s.to_string()));

    let project_name = if let Some(ref url) = remote_url {
        naming::derive_project_name_from_remote(url)
    } else {
        naming::derive_project_name_from_path(repo_path)
    };

    let project_id = naming::generate_project_id(repo_path);

    let project = GitProjectState::new(
        project_id.to_string(),
        project_name.clone(),
        repo_path.to_path_buf(),
        remote_url.clone(),
    );

    info!(
        event = "core.git.project.detect_at_completed",
        project_id = %project_id,
        project_name = project_name,
        repo_path = %repo_path.display(),
        remote_url = remote_url.as_deref().unwrap_or("none")
    );

    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_project_not_in_repo() {
        let original_dir = std::env::current_dir().unwrap();

        if std::env::set_current_dir("/tmp").is_ok() {
            let result = detect_project();
            if result.is_err() {
                assert!(matches!(result.unwrap_err(), GitError::NotInRepository));
            }

            let _ = std::env::set_current_dir(original_dir);
        }
    }

    fn create_temp_test_dir(prefix: &str) -> PathBuf {
        let temp_dir = std::env::temp_dir().join(format!("{}_{}", prefix, std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
        temp_dir
    }

    fn init_test_repo(path: &Path) {
        let repo = Repository::init(path).expect("Failed to init git repo");
        let sig = repo
            .signature()
            .unwrap_or_else(|_| git2::Signature::now("Test", "test@test.com").unwrap());
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .expect("Failed to create initial commit");
    }

    #[test]
    fn test_detect_project_at_not_in_repo() {
        let temp_dir = create_temp_test_dir("shards_test_not_a_repo");

        let result = detect_project_at(&temp_dir);

        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), GitError::NotInRepository),
            "Expected NotInRepository error for non-git directory"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_project_at_uses_provided_path_not_cwd() {
        let temp_dir = create_temp_test_dir("shards_test_project_at");
        init_test_repo(&temp_dir);

        let result = detect_project_at(&temp_dir);

        assert!(
            result.is_ok(),
            "detect_project_at should succeed for valid git repo"
        );

        let project = result.unwrap();

        let expected_path = temp_dir.canonicalize().unwrap();
        let actual_path = project.path.canonicalize().unwrap();
        assert_eq!(
            actual_path, expected_path,
            "GitProjectState.path should match the provided path, not cwd"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_project_at_discovers_from_subdirectory() {
        let temp_dir = create_temp_test_dir("shards_test_subdir");
        init_test_repo(&temp_dir);

        let subdir = temp_dir.join("src").join("nested").join("deep");
        std::fs::create_dir_all(&subdir).expect("Failed to create subdirectory");

        let result = detect_project_at(&subdir);

        assert!(
            result.is_ok(),
            "detect_project_at should discover repo from subdirectory"
        );

        let project = result.unwrap();

        let expected_path = temp_dir.canonicalize().unwrap();
        let actual_path = project.path.canonicalize().unwrap();
        assert_eq!(
            actual_path, expected_path,
            "GitProjectState.path should be repo root, not subdirectory"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_project_at_project_id_consistent() {
        let temp_dir = create_temp_test_dir("shards_test_consistent_id");
        init_test_repo(&temp_dir);

        let subdir = temp_dir.join("src");
        std::fs::create_dir_all(&subdir).expect("Failed to create subdirectory");

        let project_from_root = detect_project_at(&temp_dir).unwrap();
        let project_from_subdir = detect_project_at(&subdir).unwrap();

        assert_eq!(
            project_from_root.id, project_from_subdir.id,
            "Project ID should be consistent regardless of path within repo"
        );

        assert_eq!(
            project_from_root.path, project_from_subdir.path,
            "Project path should be repo root regardless of input path"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
