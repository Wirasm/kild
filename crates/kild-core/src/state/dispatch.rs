use tracing::{debug, error, info};

use crate::config::KildConfig;
use crate::projects::{Project, load_projects, save_projects};
use crate::sessions::handler as session_ops;
use crate::sessions::types::CreateSessionRequest;
use crate::state::errors::DispatchError;
use crate::state::events::Event;
use crate::state::store::Store;
use crate::state::types::Command;

/// Default Store implementation that routes commands to kild-core handlers.
///
/// Holds a `KildConfig` used only by the `CreateKild` command. Other session
/// commands (`DestroyKild`, `OpenKild`, `StopKild`, `CompleteKild`) load their
/// own config internally via their handlers.
pub struct CoreStore {
    config: KildConfig,
}

impl CoreStore {
    pub fn new(config: KildConfig) -> Self {
        Self { config }
    }
}

impl Store for CoreStore {
    type Error = DispatchError;

    fn dispatch(&mut self, cmd: Command) -> Result<Vec<Event>, DispatchError> {
        debug!(event = "core.state.dispatch_started", command = ?cmd);

        let result = match cmd {
            Command::CreateKild {
                branch,
                agent,
                note,
                project_path,
            } => {
                let request = match project_path {
                    Some(path) => {
                        CreateSessionRequest::with_project_path(branch, agent, note, path)
                    }
                    None => CreateSessionRequest::new(branch, agent, note),
                };
                let session = session_ops::create_session(request, &self.config)?;
                Ok(vec![Event::KildCreated {
                    branch: session.branch,
                    session_id: session.id,
                }])
            }
            Command::DestroyKild { branch, force } => {
                session_ops::destroy_session(&branch, force)?;
                Ok(vec![Event::KildDestroyed { branch }])
            }
            Command::OpenKild { branch, agent } => {
                session_ops::open_session(&branch, agent)?;
                Ok(vec![Event::KildOpened { branch }])
            }
            Command::StopKild { branch } => {
                session_ops::stop_session(&branch)?;
                Ok(vec![Event::KildStopped { branch }])
            }
            Command::CompleteKild { branch, force } => {
                session_ops::complete_session(&branch, force)?;
                Ok(vec![Event::KildCompleted { branch }])
            }
            Command::RefreshSessions => {
                session_ops::list_sessions()?;
                Ok(vec![Event::SessionsRefreshed])
            }
            Command::AddProject { path, name } => {
                let project = Project::new(path.clone(), name)?;
                let mut data = load_projects();

                if data.projects.iter().any(|p| p.path() == project.path()) {
                    return Err(DispatchError::Project(
                        crate::projects::ProjectError::AlreadyExists,
                    ));
                }

                let canonical_path = project.path().to_path_buf();
                let project_name = project.name().to_string();

                // Auto-select first project
                if data.projects.is_empty() {
                    data.active = Some(canonical_path.clone());
                }
                data.projects.push(project);
                save_projects(&data)?;

                Ok(vec![Event::ProjectAdded {
                    path: canonical_path,
                    name: project_name,
                }])
            }
            Command::RemoveProject { path } => {
                let mut data = load_projects();

                let original_len = data.projects.len();
                data.projects.retain(|p| p.path() != path);

                if data.projects.len() == original_len {
                    return Err(DispatchError::Project(
                        crate::projects::ProjectError::NotFound,
                    ));
                }

                // Clear active if removed, select first remaining
                if data.active.as_deref() == Some(&path) {
                    data.active = data.projects.first().map(|p| p.path().to_path_buf());
                }
                save_projects(&data)?;

                Ok(vec![Event::ProjectRemoved { path }])
            }
            Command::SelectProject { path } => {
                let mut data = load_projects();

                if let Some(p) = &path
                    && !data.projects.iter().any(|proj| proj.path() == p.as_path())
                {
                    return Err(DispatchError::Project(
                        crate::projects::ProjectError::NotFound,
                    ));
                }

                data.active = path.clone();
                save_projects(&data)?;

                Ok(vec![Event::ActiveProjectChanged { path }])
            }
        };

        match &result {
            Ok(events) => info!(
                event = "core.state.dispatch_completed",
                event_count = events.len()
            ),
            Err(e) => error!(event = "core.state.dispatch_failed", error = %e),
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_core_store_implements_store_trait() {
        // Verify CoreStore compiles as a Store implementation
        fn assert_store<T: Store>(_s: &T) {}
        let store = CoreStore::new(KildConfig::default());
        assert_store(&store);
    }

    #[test]
    fn test_core_store_add_project_validates_path() {
        let mut store = CoreStore::new(KildConfig::default());
        let result = store.dispatch(Command::AddProject {
            path: PathBuf::from("/nonexistent/path/that/does/not/exist"),
            name: Some("Test".to_string()),
        });
        assert!(result.is_err(), "Should fail for nonexistent path");
    }

    #[test]
    fn test_core_store_remove_project_validates_path() {
        let mut store = CoreStore::new(KildConfig::default());
        let result = store.dispatch(Command::RemoveProject {
            path: PathBuf::from("/nonexistent/project"),
        });
        assert!(result.is_err(), "Should fail for nonexistent project");
    }

    #[test]
    fn test_core_store_select_project_validates_path() {
        let mut store = CoreStore::new(KildConfig::default());
        let result = store.dispatch(Command::SelectProject {
            path: Some(PathBuf::from("/nonexistent/project")),
        });
        assert!(result.is_err(), "Should fail for nonexistent project");
    }

    #[test]
    fn test_core_store_select_project_none_succeeds() {
        let mut store = CoreStore::new(KildConfig::default());
        let result = store.dispatch(Command::SelectProject { path: None });
        assert!(result.is_ok(), "Select all should succeed");
        assert_eq!(
            result.unwrap(),
            vec![Event::ActiveProjectChanged { path: None }]
        );
    }

    #[test]
    fn test_create_request_with_project_path() {
        let request = CreateSessionRequest::with_project_path(
            "test-branch".to_string(),
            Some("claude".to_string()),
            Some("a note".to_string()),
            PathBuf::from("/tmp/project"),
        );
        assert_eq!(request.branch, "test-branch");
        assert_eq!(request.agent, Some("claude".to_string()));
        assert_eq!(request.note, Some("a note".to_string()));
        assert_eq!(request.project_path, Some(PathBuf::from("/tmp/project")));
    }

    #[test]
    fn test_create_request_without_project_path() {
        let request =
            CreateSessionRequest::new("test-branch".to_string(), Some("claude".to_string()), None);
        assert_eq!(request.branch, "test-branch");
        assert_eq!(request.agent, Some("claude".to_string()));
        assert_eq!(request.note, None);
        assert_eq!(request.project_path, None);
    }
}
