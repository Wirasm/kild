use crate::terminal::types::TerminalType;
use serde::{Deserialize, Serialize};

/// Represents a single agent process spawned within a kild session.
///
/// Multiple agents can run concurrently in the same kild via `kild open`.
/// Each open operation appends an `AgentProcess` to the session's `agents` vec.
///
/// Invariant: `process_id`, `process_name`, and `process_start_time` must all
/// be `Some` or all be `None`. This ensures PID reuse protection always has
/// the metadata it needs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(into = "AgentProcessDto")]
pub struct AgentProcess {
    agent: String,
    /// Unique identifier for this agent spawn within the session.
    ///
    /// Format: `{session_id}_{spawn_index}` (e.g., `"abc123_0"`, `"abc123_1"`).
    /// Used for per-agent PID file isolation and Ghostty window title generation.
    /// Computed by `compute_spawn_id()` in the session handler.
    ///
    /// Empty string for legacy sessions created before per-agent spawn tracking.
    /// Code handling spawn_id must check `spawn_id().is_empty()` and fall back
    /// to session-level PID files for backward compatibility.
    spawn_id: String,
    process_id: Option<u32>,
    process_name: Option<String>,
    process_start_time: Option<u64>,
    terminal_type: Option<TerminalType>,
    terminal_window_id: Option<String>,
    command: String,
    opened_at: String,
    /// Daemon session ID when this agent runs in a daemon-owned PTY.
    /// Contains the spawn_id (e.g., `"myproject_feature-auth_0"`) to uniquely
    /// identify this agent's PTY session in the daemon.
    /// When `Some`, process_id/process_name/process_start_time are `None`
    /// and operations route through `daemon::client` instead of PID-based tracking.
    daemon_session_id: Option<String>,
}

/// Internal serde representation that routes through [`AgentProcess::new`]
/// on deserialization to enforce the PID metadata invariant.
#[derive(Serialize, Deserialize)]
struct AgentProcessDto {
    agent: String,
    /// See [`AgentProcess`] `spawn_id` field. Defaults to empty for backward compat.
    #[serde(default)]
    spawn_id: String,
    process_id: Option<u32>,
    process_name: Option<String>,
    process_start_time: Option<u64>,
    terminal_type: Option<TerminalType>,
    terminal_window_id: Option<String>,
    command: String,
    opened_at: String,
    #[serde(default)]
    daemon_session_id: Option<String>,
}

impl From<AgentProcess> for AgentProcessDto {
    fn from(ap: AgentProcess) -> Self {
        Self {
            agent: ap.agent,
            spawn_id: ap.spawn_id,
            process_id: ap.process_id,
            process_name: ap.process_name,
            process_start_time: ap.process_start_time,
            terminal_type: ap.terminal_type,
            terminal_window_id: ap.terminal_window_id,
            command: ap.command,
            opened_at: ap.opened_at,
            daemon_session_id: ap.daemon_session_id,
        }
    }
}

impl TryFrom<AgentProcessDto> for AgentProcess {
    type Error = String;

    fn try_from(data: AgentProcessDto) -> Result<Self, Self::Error> {
        AgentProcess::new(
            data.agent,
            data.spawn_id,
            data.process_id,
            data.process_name,
            data.process_start_time,
            data.terminal_type,
            data.terminal_window_id,
            data.command,
            data.opened_at,
            data.daemon_session_id,
        )
        .map_err(|e| e.to_string())
    }
}

impl<'de> Deserialize<'de> for AgentProcess {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let data = AgentProcessDto::deserialize(deserializer)?;
        AgentProcess::try_from(data).map_err(serde::de::Error::custom)
    }
}

impl AgentProcess {
    /// Create a new agent process with validated metadata.
    ///
    /// Returns `InvalidProcessMetadata` if process tracking fields are
    /// inconsistent (e.g., `process_id` is `Some` but `process_name` is `None`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        agent: String,
        spawn_id: String,
        process_id: Option<u32>,
        process_name: Option<String>,
        process_start_time: Option<u64>,
        terminal_type: Option<TerminalType>,
        terminal_window_id: Option<String>,
        command: String,
        opened_at: String,
        daemon_session_id: Option<String>,
    ) -> Result<Self, super::super::errors::SessionError> {
        // Validate: process_id, process_name, process_start_time must all be present or all absent
        let has_pid = process_id.is_some();
        let has_name = process_name.is_some();
        let has_time = process_start_time.is_some();
        if has_pid != has_name || has_pid != has_time {
            return Err(super::super::errors::SessionError::InvalidProcessMetadata);
        }

        Ok(Self {
            agent,
            spawn_id,
            process_id,
            process_name,
            process_start_time,
            terminal_type,
            terminal_window_id,
            command,
            opened_at,
            daemon_session_id,
        })
    }

    pub fn agent(&self) -> &str {
        &self.agent
    }

    pub fn spawn_id(&self) -> &str {
        &self.spawn_id
    }

    pub fn process_id(&self) -> Option<u32> {
        self.process_id
    }

    pub fn process_name(&self) -> Option<&str> {
        self.process_name.as_deref()
    }

    pub fn process_start_time(&self) -> Option<u64> {
        self.process_start_time
    }

    pub fn terminal_type(&self) -> Option<&TerminalType> {
        self.terminal_type.as_ref()
    }

    pub fn terminal_window_id(&self) -> Option<&str> {
        self.terminal_window_id.as_deref()
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn opened_at(&self) -> &str {
        &self.opened_at
    }

    pub fn daemon_session_id(&self) -> Option<&str> {
        self.daemon_session_id.as_deref()
    }

    /// Update terminal attach info after spawning an attach window.
    /// Called in a second save pass to avoid race conditions where the
    /// attach window's `kild attach` runs before the session is persisted.
    pub fn set_attach_info(&mut self, terminal_type: TerminalType, window_id: String) {
        self.terminal_type = Some(terminal_type);
        self.terminal_window_id = Some(window_id);
    }
}
