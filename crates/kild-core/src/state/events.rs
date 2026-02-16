use std::path::PathBuf;

use kild_protocol::{BranchName, SessionId};
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
    KildCreated {
        branch: BranchName,
        session_id: SessionId,
    },
    /// A kild session was destroyed (worktree removed, session file deleted).
    KildDestroyed { branch: BranchName },
    /// An additional agent terminal was opened in an existing kild.
    KildOpened { branch: BranchName, agent: String },
    /// The agent process in a kild was stopped (kild preserved).
    KildStopped { branch: BranchName },
    /// A kild was completed (PR checked, branch cleaned, session destroyed).
    KildCompleted { branch: BranchName },
    /// Agent status was updated for a kild session.
    AgentStatusUpdated {
        branch: BranchName,
        status: AgentStatus,
    },
    /// PR status was refreshed for a kild session.
    PrStatusRefreshed { branch: BranchName },
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
            branch: "my-feature".into(),
            session_id: "abc-123".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_all_event_variants_serialize() {
        let events = vec![
            Event::KildCreated {
                branch: "feature".into(),
                session_id: "id-1".into(),
            },
            Event::KildDestroyed {
                branch: "feature".into(),
            },
            Event::KildOpened {
                branch: "feature".into(),
                agent: "claude".to_string(),
            },
            Event::KildStopped {
                branch: "feature".into(),
            },
            Event::KildCompleted {
                branch: "feature".into(),
            },
            Event::AgentStatusUpdated {
                branch: "feature".into(),
                status: AgentStatus::Working,
            },
            Event::PrStatusRefreshed {
                branch: "feature".into(),
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
                branch: "test".into(),
                session_id: "id-2".into(),
            },
            Event::KildDestroyed {
                branch: "test".into(),
            },
            Event::KildOpened {
                branch: "test".into(),
                agent: "claude".to_string(),
            },
            Event::KildStopped {
                branch: "test".into(),
            },
            Event::KildCompleted {
                branch: "test".into(),
            },
            Event::AgentStatusUpdated {
                branch: "feature".into(),
                status: AgentStatus::Working,
            },
            Event::PrStatusRefreshed {
                branch: "feature".into(),
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
