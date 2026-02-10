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
                agent_mode,
                note,
                project_path,
            } => {
                let request = match project_path {
                    Some(path) => {
                        CreateSessionRequest::with_project_path(branch, agent_mode, note, path)
                    }
                    None => CreateSessionRequest::new(branch, agent_mode, note),
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
            Command::OpenKild {
                branch,
                mode,
                runtime_mode,
            } => {
                let session = session_ops::open_session(&branch, mode, runtime_mode)?;
                Ok(vec![Event::KildOpened {
                    branch,
                    agent: session.agent,
                }])
            }
            Command::StopKild { branch } => {
                session_ops::stop_session(&branch)?;
                Ok(vec![Event::KildStopped { branch }])
            }
            Command::CompleteKild { branch } => {
                session_ops::complete_session(&branch)?;
                Ok(vec![Event::KildCompleted { branch }])
            }
            Command::UpdateAgentStatus { branch, status } => {
                session_ops::update_agent_status(&branch, status)?;
                Ok(vec![Event::AgentStatusUpdated { branch, status }])
            }
            Command::RefreshPrStatus { branch } => {
                // Look up session, fetch PR info, write sidecar
                let session = session_ops::get_session(&branch)?;
                let kild_branch = crate::git::operations::kild_branch_name(&branch);
                if session_ops::has_remote_configured(&session.worktree_path)
                    && let Some(pr_info) =
                        session_ops::fetch_pr_info(&session.worktree_path, &kild_branch)
                {
                    let config = crate::config::Config::new();
                    crate::sessions::persistence::write_pr_info(
                        &config.sessions_dir(),
                        &session.id,
                        &pr_info,
                    )?;
                }
                Ok(vec![Event::PrStatusRefreshed { branch }])
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
    use crate::projects::persistence::test_helpers::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Create a temp directory with an initialized git repo.
    fn create_temp_git_repo() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .expect("git init failed");
        temp_dir
    }

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
        use crate::state::types::AgentMode;
        let request = CreateSessionRequest::with_project_path(
            "test-branch".to_string(),
            AgentMode::Agent("claude".to_string()),
            Some("a note".to_string()),
            PathBuf::from("/tmp/project"),
        );
        assert_eq!(request.branch, "test-branch");
        assert_eq!(request.agent_mode, AgentMode::Agent("claude".to_string()));
        assert_eq!(request.note, Some("a note".to_string()));
        assert_eq!(request.project_path, Some(PathBuf::from("/tmp/project")));
    }

    #[test]
    fn test_create_request_without_project_path() {
        use crate::state::types::AgentMode;
        let request = CreateSessionRequest::new(
            "test-branch".to_string(),
            AgentMode::Agent("claude".to_string()),
            None,
        );
        assert_eq!(request.branch, "test-branch");
        assert_eq!(request.agent_mode, AgentMode::Agent("claude".to_string()));
        assert_eq!(request.note, None);
        assert_eq!(request.project_path, None);
    }

    // --- Project dispatch integration tests ---

    #[test]
    fn test_add_project_persists_and_emits_event() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let projects_file = temp_dir.path().join("projects.json");
        let _guard = ProjectsFileEnvGuard::new(&projects_file);

        let repo = create_temp_git_repo();
        let mut store = CoreStore::new(KildConfig::default());

        let events = store
            .dispatch(Command::AddProject {
                path: repo.path().to_path_buf(),
                name: Some("Test Project".to_string()),
            })
            .expect("AddProject should succeed");

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Event::ProjectAdded { name, .. } if name == "Test Project"));

        // Verify persisted to disk
        let loaded = load_projects();
        assert_eq!(loaded.projects.len(), 1);
        assert_eq!(loaded.projects[0].name(), "Test Project");

        // First project should be auto-selected
        assert_eq!(loaded.active, Some(repo.path().canonicalize().unwrap()));
    }

    #[test]
    fn test_add_project_derives_name_from_path_when_none() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let projects_file = temp_dir.path().join("projects.json");
        let _guard = ProjectsFileEnvGuard::new(&projects_file);

        let repo = create_temp_git_repo();
        let mut store = CoreStore::new(KildConfig::default());

        let events = store
            .dispatch(Command::AddProject {
                path: repo.path().to_path_buf(),
                name: None,
            })
            .expect("AddProject should succeed");

        // Name should be derived from the directory name
        let expected_name = repo
            .path()
            .canonicalize()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(matches!(&events[0], Event::ProjectAdded { name, .. } if name == &expected_name));
    }

    #[test]
    fn test_add_project_duplicate_path_fails() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let projects_file = temp_dir.path().join("projects.json");
        let _guard = ProjectsFileEnvGuard::new(&projects_file);

        let repo = create_temp_git_repo();
        let mut store = CoreStore::new(KildConfig::default());

        store
            .dispatch(Command::AddProject {
                path: repo.path().to_path_buf(),
                name: Some("First".to_string()),
            })
            .expect("First add should succeed");

        let result = store.dispatch(Command::AddProject {
            path: repo.path().to_path_buf(),
            name: Some("Duplicate".to_string()),
        });
        assert!(result.is_err(), "Duplicate path should fail");
    }

    #[test]
    fn test_add_project_does_not_change_active_when_not_first() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let projects_file = temp_dir.path().join("projects.json");
        let _guard = ProjectsFileEnvGuard::new(&projects_file);

        let repo1 = create_temp_git_repo();
        let repo2 = create_temp_git_repo();
        let mut store = CoreStore::new(KildConfig::default());

        store
            .dispatch(Command::AddProject {
                path: repo1.path().to_path_buf(),
                name: Some("First".to_string()),
            })
            .unwrap();

        store
            .dispatch(Command::AddProject {
                path: repo2.path().to_path_buf(),
                name: Some("Second".to_string()),
            })
            .unwrap();

        // Active should still be the first project
        let loaded = load_projects();
        assert_eq!(loaded.projects.len(), 2);
        assert_eq!(loaded.active, Some(repo1.path().canonicalize().unwrap()));
    }

    #[test]
    fn test_remove_project_persists_and_adjusts_active() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let projects_file = temp_dir.path().join("projects.json");
        let _guard = ProjectsFileEnvGuard::new(&projects_file);

        let repo1 = create_temp_git_repo();
        let repo2 = create_temp_git_repo();
        let mut store = CoreStore::new(KildConfig::default());

        store
            .dispatch(Command::AddProject {
                path: repo1.path().to_path_buf(),
                name: Some("First".to_string()),
            })
            .unwrap();
        store
            .dispatch(Command::AddProject {
                path: repo2.path().to_path_buf(),
                name: Some("Second".to_string()),
            })
            .unwrap();

        // Remove the active (first) project
        let canonical1 = repo1.path().canonicalize().unwrap();
        let events = store
            .dispatch(Command::RemoveProject {
                path: canonical1.clone(),
            })
            .expect("RemoveProject should succeed");

        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Event::ProjectRemoved { path } if path == &canonical1));

        // Active should switch to the remaining project
        let loaded = load_projects();
        assert_eq!(loaded.projects.len(), 1);
        assert_eq!(loaded.active, Some(repo2.path().canonicalize().unwrap()));
    }

    #[test]
    fn test_remove_nonexistent_project_fails() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let projects_file = temp_dir.path().join("projects.json");
        let _guard = ProjectsFileEnvGuard::new(&projects_file);

        let mut store = CoreStore::new(KildConfig::default());
        let result = store.dispatch(Command::RemoveProject {
            path: PathBuf::from("/does/not/exist"),
        });
        assert!(result.is_err(), "Should fail for nonexistent project");
    }

    #[test]
    fn test_select_project_persists_to_disk() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let projects_file = temp_dir.path().join("projects.json");
        let _guard = ProjectsFileEnvGuard::new(&projects_file);

        let repo1 = create_temp_git_repo();
        let repo2 = create_temp_git_repo();
        let mut store = CoreStore::new(KildConfig::default());

        store
            .dispatch(Command::AddProject {
                path: repo1.path().to_path_buf(),
                name: Some("First".to_string()),
            })
            .unwrap();
        store
            .dispatch(Command::AddProject {
                path: repo2.path().to_path_buf(),
                name: Some("Second".to_string()),
            })
            .unwrap();

        // Select the second project
        let canonical2 = repo2.path().canonicalize().unwrap();
        let events = store
            .dispatch(Command::SelectProject {
                path: Some(canonical2.clone()),
            })
            .expect("SelectProject should succeed");

        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            Event::ActiveProjectChanged { path: Some(p) } if p == &canonical2
        ));

        // Verify persisted
        let loaded = load_projects();
        assert_eq!(loaded.active, Some(canonical2));
    }

    #[test]
    fn test_select_project_none_clears_active() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let projects_file = temp_dir.path().join("projects.json");
        let _guard = ProjectsFileEnvGuard::new(&projects_file);

        let repo = create_temp_git_repo();
        let mut store = CoreStore::new(KildConfig::default());

        store
            .dispatch(Command::AddProject {
                path: repo.path().to_path_buf(),
                name: Some("Project".to_string()),
            })
            .unwrap();

        // Select "all projects" (None)
        let events = store
            .dispatch(Command::SelectProject { path: None })
            .expect("Select all should succeed");

        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            Event::ActiveProjectChanged { path: None }
        ));

        // Verify persisted
        let loaded = load_projects();
        assert!(loaded.active.is_none());
    }
}
