use std::path::{Path, PathBuf};

use super::errors::ProjectError;
use super::types::Project;

/// Encapsulates project list with enforced invariants.
///
/// Key invariant: `active_index` always points to a valid index in `projects`,
/// or is `None` (meaning "all projects" view). This invariant is maintained
/// automatically when projects are added or removed.
#[derive(Clone, Debug, Default)]
pub struct ProjectManager {
    /// List of registered projects (private to enforce invariants).
    projects: Vec<Project>,
    /// Index of the active project, or None for "all projects" view.
    active_index: Option<usize>,
}

impl ProjectManager {
    /// Create a new empty project manager.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a project manager from existing data.
    ///
    /// If `active_path` doesn't match any project, sets active_index to None.
    pub fn from_data(projects: Vec<Project>, active_path: Option<PathBuf>) -> Self {
        let active_index = active_path
            .as_ref()
            .and_then(|path| projects.iter().position(|p| p.path() == path));

        Self {
            projects,
            active_index,
        }
    }

    /// Select a project by path.
    ///
    /// # Errors
    /// Returns `ProjectError::NotFound` if no project matches the path.
    pub fn select(&mut self, path: &Path) -> Result<(), ProjectError> {
        let index = self
            .projects
            .iter()
            .position(|p| p.path() == path)
            .ok_or(ProjectError::NotFound)?;
        self.active_index = Some(index);
        Ok(())
    }

    /// Select "all projects" view (clears active project selection).
    pub fn select_all(&mut self) {
        self.active_index = None;
    }

    /// Add a project to the list.
    ///
    /// If this is the first project added, it becomes active automatically.
    ///
    /// # Errors
    /// Returns `ProjectError::AlreadyExists` if a project with the same path exists.
    pub fn add(&mut self, project: Project) -> Result<(), ProjectError> {
        if self.projects.iter().any(|p| p.path() == project.path()) {
            return Err(ProjectError::AlreadyExists);
        }

        self.projects.push(project);

        // First project becomes active automatically
        if self.projects.len() == 1 {
            self.active_index = Some(0);
        }

        Ok(())
    }

    /// Remove a project by path.
    ///
    /// Automatically adjusts `active_index` to maintain invariant:
    /// - If removed project was active: selects first project, or None if empty
    /// - If removed project was before active: decrements active_index
    ///
    /// # Errors
    /// Returns `ProjectError::NotFound` if no project matches the path.
    pub fn remove(&mut self, path: &Path) -> Result<Project, ProjectError> {
        let index = self
            .projects
            .iter()
            .position(|p| p.path() == path)
            .ok_or(ProjectError::NotFound)?;

        // Adjust active_index before removal
        self.active_index = match self.active_index {
            Some(active) if active == index => {
                // Removed project was active - select first remaining, or None
                if self.projects.len() > 1 {
                    Some(0)
                } else {
                    None
                }
            }
            Some(active) if active > index => {
                // Active was after removed - decrement to maintain reference
                Some(active - 1)
            }
            other => other,
        };

        Ok(self.projects.remove(index))
    }

    /// Get the active project, if any.
    pub fn active(&self) -> Option<&Project> {
        self.active_index.map(|i| &self.projects[i])
    }

    /// Get the active project's path, if any.
    pub fn active_path(&self) -> Option<&Path> {
        self.active().map(|p| p.path())
    }

    /// Iterate over all projects.
    pub fn iter(&self) -> impl Iterator<Item = &Project> {
        self.projects.iter()
    }

    /// Check if the project list is empty.
    pub fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }

    /// Get the number of projects.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.projects.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_project_manager_new_is_empty() {
        let pm = ProjectManager::new();
        assert!(pm.is_empty());
        assert_eq!(pm.len(), 0);
        assert!(pm.active().is_none());
        assert!(pm.active_path().is_none());
    }

    #[test]
    fn test_project_manager_add_first_project_becomes_active() {
        let mut pm = ProjectManager::new();
        let project = Project::new_unchecked(
            PathBuf::from("/path/to/project"),
            "Test Project".to_string(),
        );

        pm.add(project).unwrap();

        assert!(!pm.is_empty());
        assert_eq!(pm.len(), 1);
        assert!(pm.active().is_some());
        assert_eq!(pm.active_path(), Some(Path::new("/path/to/project")));
    }

    #[test]
    fn test_project_manager_add_duplicate_returns_error() {
        let mut pm = ProjectManager::new();
        let project1 = Project::new_unchecked(
            PathBuf::from("/path/to/project"),
            "Test Project".to_string(),
        );
        let project2 =
            Project::new_unchecked(PathBuf::from("/path/to/project"), "Same Path".to_string());

        pm.add(project1).unwrap();
        let result = pm.add(project2);

        assert!(matches!(result, Err(ProjectError::AlreadyExists)));
    }

    #[test]
    fn test_project_manager_select_all_clears_active() {
        let mut pm = ProjectManager::new();
        let project = Project::new_unchecked(
            PathBuf::from("/path/to/project"),
            "Test Project".to_string(),
        );

        pm.add(project).unwrap();
        assert!(pm.active().is_some());

        pm.select_all();
        assert!(pm.active().is_none());
    }

    #[test]
    fn test_project_manager_select_valid_path() {
        let mut pm = ProjectManager::new();
        let project1 =
            Project::new_unchecked(PathBuf::from("/path/to/project-a"), "Project A".to_string());
        let project2 =
            Project::new_unchecked(PathBuf::from("/path/to/project-b"), "Project B".to_string());

        pm.add(project1).unwrap();
        pm.add(project2).unwrap();

        assert_eq!(pm.active_path(), Some(Path::new("/path/to/project-a")));

        pm.select(Path::new("/path/to/project-b")).unwrap();
        assert_eq!(pm.active_path(), Some(Path::new("/path/to/project-b")));
    }

    #[test]
    fn test_project_manager_select_invalid_path_returns_error() {
        let mut pm = ProjectManager::new();
        let project = Project::new_unchecked(
            PathBuf::from("/path/to/project"),
            "Test Project".to_string(),
        );

        pm.add(project).unwrap();

        let result = pm.select(Path::new("/nonexistent/path"));
        assert!(matches!(result, Err(ProjectError::NotFound)));
    }

    #[test]
    fn test_project_manager_remove_active_selects_first() {
        let mut pm = ProjectManager::new();
        let project1 =
            Project::new_unchecked(PathBuf::from("/path/to/project-a"), "Project A".to_string());
        let project2 =
            Project::new_unchecked(PathBuf::from("/path/to/project-b"), "Project B".to_string());

        pm.add(project1).unwrap();
        pm.add(project2).unwrap();

        assert_eq!(pm.active_path(), Some(Path::new("/path/to/project-a")));

        pm.remove(Path::new("/path/to/project-a")).unwrap();

        assert_eq!(pm.len(), 1);
        assert_eq!(pm.active_path(), Some(Path::new("/path/to/project-b")));
    }

    #[test]
    fn test_project_manager_remove_non_active_preserves_selection() {
        let mut pm = ProjectManager::new();
        let project1 =
            Project::new_unchecked(PathBuf::from("/path/to/project-a"), "Project A".to_string());
        let project2 =
            Project::new_unchecked(PathBuf::from("/path/to/project-b"), "Project B".to_string());

        pm.add(project1).unwrap();
        pm.add(project2).unwrap();

        assert_eq!(pm.active_path(), Some(Path::new("/path/to/project-a")));

        pm.remove(Path::new("/path/to/project-b")).unwrap();

        assert_eq!(pm.len(), 1);
        assert_eq!(pm.active_path(), Some(Path::new("/path/to/project-a")));
    }

    #[test]
    fn test_project_manager_remove_last_clears_active() {
        let mut pm = ProjectManager::new();
        let project = Project::new_unchecked(
            PathBuf::from("/path/to/project"),
            "Test Project".to_string(),
        );

        pm.add(project).unwrap();
        pm.remove(Path::new("/path/to/project")).unwrap();

        assert!(pm.is_empty());
        assert!(pm.active().is_none());
    }

    #[test]
    fn test_project_manager_remove_before_active_adjusts_index() {
        let mut pm = ProjectManager::new();
        let project1 =
            Project::new_unchecked(PathBuf::from("/path/to/project-a"), "Project A".to_string());
        let project2 =
            Project::new_unchecked(PathBuf::from("/path/to/project-b"), "Project B".to_string());
        let project3 =
            Project::new_unchecked(PathBuf::from("/path/to/project-c"), "Project C".to_string());

        pm.add(project1).unwrap();
        pm.add(project2).unwrap();
        pm.add(project3).unwrap();

        pm.select(Path::new("/path/to/project-c")).unwrap();
        assert_eq!(pm.active_path(), Some(Path::new("/path/to/project-c")));

        pm.remove(Path::new("/path/to/project-a")).unwrap();

        assert_eq!(pm.len(), 2);
        assert_eq!(pm.active_path(), Some(Path::new("/path/to/project-c")));
    }

    #[test]
    fn test_project_manager_from_data_with_valid_active() {
        let projects = vec![
            Project::new_unchecked(PathBuf::from("/path/to/project-a"), "Project A".to_string()),
            Project::new_unchecked(PathBuf::from("/path/to/project-b"), "Project B".to_string()),
        ];

        let pm = ProjectManager::from_data(projects, Some(PathBuf::from("/path/to/project-b")));

        assert_eq!(pm.len(), 2);
        assert_eq!(pm.active_path(), Some(Path::new("/path/to/project-b")));
    }

    #[test]
    fn test_project_manager_from_data_with_invalid_active() {
        let projects = vec![Project::new_unchecked(
            PathBuf::from("/path/to/project-a"),
            "Project A".to_string(),
        )];

        let pm = ProjectManager::from_data(projects, Some(PathBuf::from("/nonexistent/path")));

        assert_eq!(pm.len(), 1);
        assert!(
            pm.active().is_none(),
            "Invalid active path should be ignored"
        );
    }

    #[test]
    fn test_project_manager_iter() {
        let mut pm = ProjectManager::new();
        let project1 =
            Project::new_unchecked(PathBuf::from("/path/to/project-a"), "Project A".to_string());
        let project2 =
            Project::new_unchecked(PathBuf::from("/path/to/project-b"), "Project B".to_string());

        pm.add(project1).unwrap();
        pm.add(project2).unwrap();

        let paths: Vec<_> = pm.iter().map(|p| p.path()).collect();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], Path::new("/path/to/project-a"));
        assert_eq!(paths[1], Path::new("/path/to/project-b"));
    }
}
