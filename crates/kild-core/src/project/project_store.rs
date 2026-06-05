use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::project_errors::ProjectError;
use super::project_types::Project;

/// On-disk shape (`~/.config/kild/projects.json`). Wrapped in an object so fields can
/// be added later without breaking the format.
#[derive(Default, Serialize, Deserialize)]
struct ProjectsFile {
    projects: Vec<Project>,
}

fn projects_path() -> Result<PathBuf, ProjectError> {
    crate::paths::projects_file().ok_or(ProjectError::NoHome)
}

/// Load all projects (empty if the store doesn't exist yet).
pub fn load_projects() -> Result<Vec<Project>, ProjectError> {
    match fs::read(projects_path()?) {
        Ok(bytes) => Ok(serde_json::from_slice::<ProjectsFile>(&bytes)?.projects),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(ProjectError::Io(e)),
    }
}

fn save_projects(projects: &[Project]) -> Result<(), ProjectError> {
    let path = projects_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = ProjectsFile {
        projects: projects.to_vec(),
    };
    fs::write(path, serde_json::to_vec_pretty(&file)?)?;
    Ok(())
}

/// Register a project. The path must be an existing directory; names are unique.
/// A leading `~/` is expanded to `$HOME`.
pub fn add_project(name: String, path: String) -> Result<Project, ProjectError> {
    if name.trim().is_empty() {
        return Err(ProjectError::EmptyName);
    }
    let path = expand_tilde(&path);
    if !path.is_dir() {
        return Err(ProjectError::NotADirectory(path.display().to_string()));
    }
    let mut projects = load_projects()?;
    if projects.iter().any(|p| p.name == name) {
        return Err(ProjectError::DuplicateName(name));
    }
    let project = Project { name, path };
    projects.push(project.clone());
    save_projects(&projects)?;
    Ok(project)
}

/// Remove a project by name (no-op if it doesn't exist).
pub fn remove_project(name: &str) -> Result<(), ProjectError> {
    let mut projects = load_projects()?;
    projects.retain(|p| p.name != name);
    save_projects(&projects)
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}
