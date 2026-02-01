use tracing::{debug, error, info};

use crate::config::KildConfig;
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
///
/// Project operations are not yet wired and return `NotImplemented` errors.
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
            Command::AddProject { .. } => {
                Err(DispatchError::NotImplemented("AddProject".to_string()))
            }
            Command::RemoveProject { .. } => {
                Err(DispatchError::NotImplemented("RemoveProject".to_string()))
            }
            Command::SelectProject { .. } => {
                Err(DispatchError::NotImplemented("SelectProject".to_string()))
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
    fn test_core_store_add_project_returns_not_implemented() {
        let mut store = CoreStore::new(KildConfig::default());
        let result = store.dispatch(Command::AddProject {
            path: PathBuf::from("/tmp/project"),
            name: "Test".to_string(),
        });
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Command not implemented: AddProject"
        );
    }

    #[test]
    fn test_core_store_remove_project_returns_not_implemented() {
        let mut store = CoreStore::new(KildConfig::default());
        let result = store.dispatch(Command::RemoveProject {
            path: PathBuf::from("/tmp/project"),
        });
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Command not implemented: RemoveProject"
        );
    }

    #[test]
    fn test_core_store_select_project_returns_not_implemented() {
        let mut store = CoreStore::new(KildConfig::default());
        let result = store.dispatch(Command::SelectProject {
            path: Some(PathBuf::from("/tmp/project")),
        });
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Command not implemented: SelectProject"
        );
    }

    #[test]
    fn test_core_store_select_project_none_returns_not_implemented() {
        let mut store = CoreStore::new(KildConfig::default());
        let result = store.dispatch(Command::SelectProject { path: None });
        assert!(result.is_err());
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
