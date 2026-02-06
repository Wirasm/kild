use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::sessions::types::AgentStatus;

/// All business state changes that can result from a dispatched command.
///
/// Each variant describes _what happened_, not what should happen. Only
/// successful state changes produce events — failures use the `Result`
/// error channel (`Err(DispatchError)`), not the event stream.
///
/// Events use owned types (`String`, `PathBuf`) so they can be serialized,
/// stored, and sent across boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    /// A new kild session was created.
    KildCreated { branch: String, session_id: String },
    /// A kild session was destroyed (worktree removed, session file deleted).
    KildDestroyed { branch: String },
    /// An additional agent terminal was opened in an existing kild.
    KildOpened { branch: String, agent: String },
    /// The agent process in a kild was stopped (kild preserved).
    KildStopped { branch: String },
    /// A kild was completed (PR checked, branch cleaned, session destroyed).
    KildCompleted { branch: String },
    /// Agent status was updated for a kild session.
    AgentStatusUpdated { branch: String, status: AgentStatus },
    /// PR status was refreshed for a kild session.
    PrStatusRefreshed { branch: String },
    /// The session list was refreshed from disk.
    SessionsRefreshed,

    /// A project was added to the project list.
    ProjectAdded { path: PathBuf, name: String },
    /// A project was removed from the project list.
    ProjectRemoved { path: PathBuf },
    /// The active project selection changed.
    ActiveProjectChanged { path: Option<PathBuf> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serde_roundtrip() {
        let event = Event::KildCreated {
            branch: "my-feature".to_string(),
            session_id: "abc-123".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_all_event_variants_serialize() {
        let events = vec![
            Event::KildCreated {
                branch: "feature".to_string(),
                session_id: "id-1".to_string(),
            },
            Event::KildDestroyed {
                branch: "feature".to_string(),
            },
            Event::KildOpened {
                branch: "feature".to_string(),
                agent: "claude".to_string(),
            },
            Event::KildStopped {
                branch: "feature".to_string(),
            },
            Event::KildCompleted {
                branch: "feature".to_string(),
            },
            Event::AgentStatusUpdated {
                branch: "feature".to_string(),
                status: AgentStatus::Working,
            },
            Event::PrStatusRefreshed {
                branch: "feature".to_string(),
            },
            Event::SessionsRefreshed,
            Event::ProjectAdded {
                path: PathBuf::from("/projects/app"),
                name: "App".to_string(),
            },
            Event::ProjectRemoved {
                path: PathBuf::from("/projects/app"),
            },
            Event::ActiveProjectChanged {
                path: Some(PathBuf::from("/projects/app")),
            },
            Event::ActiveProjectChanged { path: None },
        ];
        for event in events {
            assert!(
                serde_json::to_string(&event).is_ok(),
                "Failed to serialize: {:?}",
                event
            );
        }
    }

    #[test]
    fn test_event_deserialize_all_variants() {
        let events = vec![
            Event::KildCreated {
                branch: "test".to_string(),
                session_id: "id-2".to_string(),
            },
            Event::KildDestroyed {
                branch: "test".to_string(),
            },
            Event::KildOpened {
                branch: "test".to_string(),
                agent: "claude".to_string(),
            },
            Event::KildStopped {
                branch: "test".to_string(),
            },
            Event::KildCompleted {
                branch: "test".to_string(),
            },
            Event::AgentStatusUpdated {
                branch: "feature".to_string(),
                status: AgentStatus::Working,
            },
            Event::PrStatusRefreshed {
                branch: "feature".to_string(),
            },
            Event::SessionsRefreshed,
            Event::ProjectAdded {
                path: PathBuf::from("/tmp"),
                name: "Tmp".to_string(),
            },
            Event::ProjectRemoved {
                path: PathBuf::from("/tmp"),
            },
            Event::ActiveProjectChanged { path: None },
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let roundtripped: Event = serde_json::from_str(&json).unwrap();
            assert_eq!(event, roundtripped);
        }
    }
}
