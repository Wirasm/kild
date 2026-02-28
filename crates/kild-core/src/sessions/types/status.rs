use kild_protocol::AgentStatus;
use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    #[serde(alias = "Active")]
    Active,
    #[serde(alias = "Stopped")]
    Stopped,
    #[serde(alias = "Destroyed")]
    Destroyed,
}

/// Convert a daemon protocol `SessionStatus` into a core `SessionStatus`.
///
/// `Running` and `Creating` both map to `Active` (the session is alive).
/// `Stopped` maps to `Stopped`. There is no protocol equivalent for `Destroyed`
/// since destruction is a core-only concept.
impl From<kild_protocol::SessionStatus> for SessionStatus {
    fn from(status: kild_protocol::SessionStatus) -> Self {
        match status {
            kild_protocol::SessionStatus::Running | kild_protocol::SessionStatus::Creating => {
                SessionStatus::Active
            }
            kild_protocol::SessionStatus::Stopped => SessionStatus::Stopped,
            // SessionStatus is #[non_exhaustive]; treat unknown variants as Active
            // (alive until proven otherwise).
            _ => {
                warn!(
                    event = "core.session.status_conversion_unknown",
                    "Unknown daemon SessionStatus variant; treating as Active"
                );
                SessionStatus::Active
            }
        }
    }
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Stopped => write!(f, "stopped"),
            Self::Destroyed => write!(f, "destroyed"),
        }
    }
}

/// Sidecar file content for agent status reporting.
///
/// Stored as `{session_id}.status` alongside the session `.json` file.
/// Written by `kild agent-status`, read by `kild list` and `kild status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentStatusRecord {
    pub status: AgentStatus,
    pub updated_at: String,
}

/// Process status for a kild session.
///
/// Represents whether the agent process is currently running, stopped,
/// or in an unknown state (detection failed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    /// Process is confirmed running
    Running,
    /// Process is confirmed stopped (or no PID exists)
    Stopped,
    /// Could not determine status (process check failed)
    Unknown,
}

/// Git working tree status for a kild session.
///
/// Represents whether the worktree has uncommitted changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitStatus {
    /// Worktree has no uncommitted changes
    Clean,
    /// Worktree has uncommitted changes
    Dirty,
    /// Could not determine git status (error occurred)
    Unknown,
}
