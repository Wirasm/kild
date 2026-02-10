use super::events::Event;
use super::types::Command;

/// Trait for dispatching business commands.
///
/// Decouples command definitions from their execution. Interfaces (CLI, UI)
/// implement this trait to execute commands with their specific needs
/// (e.g., UI adds event emission and async handling, CLI runs synchronously).
///
/// # Semantics
///
/// - **Ordering**: Commands execute in the order received. No implicit batching.
/// - **Idempotency**: Commands are not idempotent (e.g., `CreateKild` fails if
///   the branch already exists). Callers must avoid duplicate dispatches.
/// - **Error handling**: Implementations define their own error type. Errors
///   should distinguish user errors (invalid input) from system errors (IO failure).
/// - **Events**: On success, dispatch returns a non-empty `Vec<Event>` describing
///   what changed. Currently each command produces exactly one event. The vector
///   allows future commands to emit multiple events for compound operations.
///   Events within a single dispatch are ordered chronologically. Callers can use
///   these to react without polling or disk re-reads.
pub trait Store {
    type Error;
    fn dispatch(&mut self, cmd: Command) -> Result<Vec<Event>, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_store_trait_is_implementable() {
        struct TestStore;
        impl Store for TestStore {
            type Error = String;
            fn dispatch(&mut self, _cmd: Command) -> Result<Vec<Event>, String> {
                Ok(vec![Event::SessionsRefreshed])
            }
        }
        let mut store = TestStore;
        let result = store.dispatch(Command::RefreshSessions);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_store_impl_can_return_error() {
        struct FailingStore;
        impl Store for FailingStore {
            type Error = String;
            fn dispatch(&mut self, _cmd: Command) -> Result<Vec<Event>, String> {
                Err("not implemented".to_string())
            }
        }
        let mut store = FailingStore;
        assert!(store.dispatch(Command::RefreshSessions).is_err());
    }

    /// Documents the expected event contract for each command.
    ///
    /// CoreStore's implemented commands delegate to session handlers that
    /// require real I/O (git repos, filesystem), so the dispatch→event
    /// mapping is tested here via a contract Store implementation.
    /// This ensures phase 3c consumers can rely on the event types.
    #[test]
    fn test_event_contract_per_command() {
        struct ContractStore;
        impl Store for ContractStore {
            type Error = String;
            fn dispatch(&mut self, cmd: Command) -> Result<Vec<Event>, String> {
                match cmd {
                    Command::CreateKild { branch, .. } => Ok(vec![Event::KildCreated {
                        branch,
                        session_id: "test-id".to_string(),
                    }]),
                    Command::DestroyKild { branch, .. } => {
                        Ok(vec![Event::KildDestroyed { branch }])
                    }
                    Command::OpenKild { branch, .. } => Ok(vec![Event::KildOpened {
                        branch,
                        agent: "claude".to_string(),
                    }]),
                    Command::StopKild { branch } => Ok(vec![Event::KildStopped { branch }]),
                    Command::CompleteKild { branch, .. } => {
                        Ok(vec![Event::KildCompleted { branch }])
                    }
                    Command::UpdateAgentStatus { branch, status } => {
                        Ok(vec![Event::AgentStatusUpdated { branch, status }])
                    }

                    Command::RefreshSessions => Ok(vec![Event::SessionsRefreshed]),
                    Command::AddProject { path, name } => Ok(vec![Event::ProjectAdded {
                        path,
                        name: name.unwrap_or_default(),
                    }]),
                    Command::RemoveProject { path } => Ok(vec![Event::ProjectRemoved { path }]),
                    Command::RefreshPrStatus { branch } => {
                        Ok(vec![Event::PrStatusRefreshed { branch }])
                    }
                    Command::SelectProject { path } => {
                        Ok(vec![Event::ActiveProjectChanged { path }])
                    }
                }
            }
        }

        let mut store = ContractStore;

        // Session commands → session events
        let events = store
            .dispatch(Command::CreateKild {
                branch: "feat".to_string(),
                agent_mode: crate::state::types::AgentMode::DefaultAgent,
                note: None,
                project_path: None,
            })
            .unwrap();
        assert!(matches!(&events[0], Event::KildCreated { branch, .. } if branch == "feat"));

        let events = store
            .dispatch(Command::DestroyKild {
                branch: "feat".to_string(),
                force: false,
            })
            .unwrap();
        assert!(matches!(&events[0], Event::KildDestroyed { branch } if branch == "feat"));

        let events = store
            .dispatch(Command::OpenKild {
                branch: "feat".to_string(),
                mode: crate::state::types::OpenMode::DefaultAgent,
                runtime_mode: crate::state::types::RuntimeMode::Terminal,
                resume: false,
            })
            .unwrap();
        assert!(matches!(&events[0], Event::KildOpened { branch, .. } if branch == "feat"));

        let events = store
            .dispatch(Command::StopKild {
                branch: "feat".to_string(),
            })
            .unwrap();
        assert!(matches!(&events[0], Event::KildStopped { branch } if branch == "feat"));

        let events = store
            .dispatch(Command::CompleteKild {
                branch: "feat".to_string(),
            })
            .unwrap();
        assert!(matches!(&events[0], Event::KildCompleted { branch } if branch == "feat"));

        let events = store.dispatch(Command::RefreshSessions).unwrap();
        assert!(matches!(&events[0], Event::SessionsRefreshed));

        // Project commands → project events
        let events = store
            .dispatch(Command::AddProject {
                path: PathBuf::from("/tmp"),
                name: Some("Test".to_string()),
            })
            .unwrap();
        assert!(matches!(&events[0], Event::ProjectAdded { name, .. } if name == "Test"));

        let events = store
            .dispatch(Command::RemoveProject {
                path: PathBuf::from("/tmp"),
            })
            .unwrap();
        assert!(matches!(&events[0], Event::ProjectRemoved { .. }));

        let events = store
            .dispatch(Command::SelectProject {
                path: Some(PathBuf::from("/tmp")),
            })
            .unwrap();
        assert!(matches!(
            &events[0],
            Event::ActiveProjectChanged { path: Some(_) }
        ));
    }

    #[test]
    fn test_every_command_returns_exactly_one_event() {
        struct CountingStore;
        impl Store for CountingStore {
            type Error = String;
            fn dispatch(&mut self, cmd: Command) -> Result<Vec<Event>, String> {
                let branch = "test".to_string();
                match cmd {
                    Command::CreateKild { .. } => Ok(vec![Event::KildCreated {
                        branch,
                        session_id: "id".to_string(),
                    }]),
                    Command::DestroyKild { .. } => Ok(vec![Event::KildDestroyed { branch }]),
                    Command::OpenKild { .. } => Ok(vec![Event::KildOpened {
                        branch,
                        agent: "claude".to_string(),
                    }]),
                    Command::StopKild { .. } => Ok(vec![Event::KildStopped { branch }]),
                    Command::CompleteKild { .. } => Ok(vec![Event::KildCompleted { branch }]),
                    Command::UpdateAgentStatus { branch, status } => {
                        Ok(vec![Event::AgentStatusUpdated { branch, status }])
                    }

                    Command::RefreshSessions => Ok(vec![Event::SessionsRefreshed]),
                    Command::AddProject { path, name } => Ok(vec![Event::ProjectAdded {
                        path,
                        name: name.unwrap_or_default(),
                    }]),
                    Command::RemoveProject { path } => Ok(vec![Event::ProjectRemoved { path }]),
                    Command::RefreshPrStatus { branch } => {
                        Ok(vec![Event::PrStatusRefreshed { branch }])
                    }
                    Command::SelectProject { path } => {
                        Ok(vec![Event::ActiveProjectChanged { path }])
                    }
                }
            }
        }

        let mut store = CountingStore;
        let commands: Vec<Command> = vec![
            Command::CreateKild {
                branch: "b".to_string(),
                agent_mode: crate::state::types::AgentMode::DefaultAgent,
                note: None,
                project_path: None,
            },
            Command::DestroyKild {
                branch: "b".to_string(),
                force: false,
            },
            Command::OpenKild {
                branch: "b".to_string(),
                mode: crate::state::types::OpenMode::DefaultAgent,
                runtime_mode: crate::state::types::RuntimeMode::Terminal,
                resume: false,
            },
            Command::StopKild {
                branch: "b".to_string(),
            },
            Command::CompleteKild {
                branch: "b".to_string(),
            },
            Command::UpdateAgentStatus {
                branch: "b".to_string(),
                status: crate::sessions::types::AgentStatus::Working,
            },
            Command::RefreshPrStatus {
                branch: "b".to_string(),
            },
            Command::RefreshSessions,
            Command::AddProject {
                path: PathBuf::from("/tmp"),
                name: Some("T".to_string()),
            },
            Command::RemoveProject {
                path: PathBuf::from("/tmp"),
            },
            Command::SelectProject { path: None },
        ];

        for cmd in commands {
            let events = store.dispatch(cmd).unwrap();
            assert_eq!(
                events.len(),
                1,
                "Each command should produce exactly one event"
            );
        }
    }
}
