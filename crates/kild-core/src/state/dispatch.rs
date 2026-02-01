use tracing::{debug, error, info};

use crate::config::KildConfig;
use crate::sessions::handler as session_ops;
use crate::sessions::types::CreateSessionRequest;
use crate::state::errors::DispatchError;
use crate::state::store::Store;
use crate::state::types::Command;

/// Default Store implementation that routes commands to kild-core handlers.
///
/// Loads config on construction. Session operations delegate to `sessions::handler`.
/// Project operations are not yet wired (logged and return Ok).
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

    fn dispatch(&mut self, cmd: Command) -> Result<(), DispatchError> {
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
                session_ops::create_session(request, &self.config)?;
                Ok(())
            }
            Command::DestroyKild { branch, force } => {
                session_ops::destroy_session(&branch, force)?;
                Ok(())
            }
            Command::OpenKild { branch, agent } => {
                session_ops::open_session(&branch, agent)?;
                Ok(())
            }
            Command::StopKild { branch } => {
                session_ops::stop_session(&branch)?;
                Ok(())
            }
            Command::CompleteKild { branch, force } => {
                session_ops::complete_session(&branch, force)?;
                Ok(())
            }
            Command::RefreshSessions => {
                session_ops::list_sessions()?;
                Ok(())
            }
            Command::AddProject { .. }
            | Command::RemoveProject { .. }
            | Command::SelectProject { .. } => {
                debug!(
                    event = "core.state.dispatch_skipped",
                    reason = "project commands not yet wired"
                );
                Ok(())
            }
        };

        match &result {
            Ok(()) => info!(event = "core.state.dispatch_completed"),
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
    fn test_core_store_add_project_returns_ok() {
        let mut store = CoreStore::new(KildConfig::default());
        let result = store.dispatch(Command::AddProject {
            path: PathBuf::from("/tmp/project"),
            name: "Test".to_string(),
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_core_store_remove_project_returns_ok() {
        let mut store = CoreStore::new(KildConfig::default());
        let result = store.dispatch(Command::RemoveProject {
            path: PathBuf::from("/tmp/project"),
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_core_store_select_project_returns_ok() {
        let mut store = CoreStore::new(KildConfig::default());
        let result = store.dispatch(Command::SelectProject {
            path: Some(PathBuf::from("/tmp/project")),
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_core_store_select_project_none_returns_ok() {
        let mut store = CoreStore::new(KildConfig::default());
        let result = store.dispatch(Command::SelectProject { path: None });
        assert!(result.is_ok());
    }
}
