use serde::{Deserialize, Serialize};

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

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Stopped => write!(f, "stopped"),
            Self::Destroyed => write!(f, "destroyed"),
        }
    }
}

/// Agent-reported activity status, written via `kild agent-status` command.
///
/// This is distinct from `ProcessStatus` (running/stopped) and `HealthStatus`
/// (inferred from metrics). `AgentStatus` is explicitly reported by the agent
/// via hooks, giving real-time insight into what the agent is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Working,
    Idle,
    Waiting,
    Done,
    Error,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Working => write!(f, "working"),
            Self::Idle => write!(f, "idle"),
            Self::Waiting => write!(f, "waiting"),
            Self::Done => write!(f, "done"),
            Self::Error => write!(f, "error"),
        }
    }
}

impl std::str::FromStr for AgentStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "working" => Ok(Self::Working),
            "idle" => Ok(Self::Idle),
            "waiting" => Ok(Self::Waiting),
            "done" => Ok(Self::Done),
            "error" => Ok(Self::Error),
            other => Err(format!(
                "Invalid agent status: '{}'. Valid: working, idle, waiting, done, error",
                other
            )),
        }
    }
}

/// Sidecar file content for agent status reporting.
///
/// Stored as `{session_id}.status` alongside the session `.json` file.
/// Written by `kild agent-status`, read by `kild list` and `kild status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentStatusInfo {
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
