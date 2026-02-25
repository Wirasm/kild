use kild_protocol::{BranchName, ProjectId, SessionId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::agent_process::AgentProcess;
use super::status::SessionStatus;

fn default_port_start() -> u16 {
    0
}
fn default_port_end() -> u16 {
    0
}
fn default_port_count() -> u16 {
    0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub project_id: ProjectId,
    pub branch: BranchName,
    pub worktree_path: PathBuf,
    /// The agent type for this session (e.g. "claude", "kiro").
    ///
    /// Updated by handlers to match the latest entry in `agents` when a new
    /// agent is opened. Must be kept in sync with `agents.last()` by callers
    /// of `open_session`.
    pub agent: String,
    pub status: SessionStatus,
    pub created_at: String,
    #[serde(default = "default_port_start")]
    pub port_range_start: u16,
    #[serde(default = "default_port_end")]
    pub port_range_end: u16,
    #[serde(default = "default_port_count")]
    pub port_count: u16,

    /// Timestamp of last detected activity for health monitoring.
    ///
    /// This tracks when the session was last active for health status calculation.
    /// Used by the health monitoring system to distinguish between Idle, Stuck, and Crashed states.
    /// Initially set to session creation time, updated by activity monitoring.
    ///
    /// Format: RFC3339 timestamp string (e.g., "2024-01-01T12:00:00Z")
    #[serde(default)]
    pub last_activity: Option<String>,

    /// Optional description of what this kild is for.
    ///
    /// Set via `--note` flag during `kild create`. Shown truncated to 30 chars
    /// in list output, and truncated to 47 chars in status output.
    #[serde(default)]
    pub note: Option<String>,

    /// Agent session ID for resume support.
    ///
    /// Generated on `kild create` and fresh `kild open` for resume-capable agents (e.g., Claude Code).
    /// Injected as `--session-id <uuid>` on initial create or fresh open, and as `--resume <uuid>`
    /// when reopening with `kild open --resume`.
    /// Stored at the Session level (not AgentProcess) so it survives `clear_agents()` on stop.
    #[serde(default)]
    pub agent_session_id: Option<String>,

    /// Previous agent session IDs preserved across fresh opens.
    ///
    /// When `kild open` (without `--resume`) generates a new `agent_session_id`,
    /// the previous ID is pushed here before overwriting. This allows recovery
    /// of earlier conversations that would otherwise become unreachable.
    /// Most recent ID is last in the vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_session_id_history: Vec<String>,

    /// Task list ID for Claude Code task list persistence.
    ///
    /// Generated on `kild create` and fresh `kild open` for Claude agents.
    /// Set as `CLAUDE_CODE_TASK_LIST_ID` env var when spawning the agent.
    /// On resume: reuses existing ID so tasks survive across restarts.
    /// On fresh open: generates new ID for a clean task list.
    /// Cleaned up on `kild destroy` by removing `~/.claude/tasks/{task_list_id}/`.
    #[serde(default)]
    pub task_list_id: Option<String>,

    /// Runtime mode this session was created with (Terminal or Daemon).
    /// Used by `kild open` to auto-detect runtime mode when no flags are passed.
    /// `None` for sessions created before this field was added.
    #[serde(default)]
    pub runtime_mode: Option<kild_protocol::RuntimeMode>,

    /// Whether this session was created with `--main` (runs from project root, no linked worktree).
    ///
    /// When true, `worktree_path` points to the project root itself.
    /// `destroy_session` skips git worktree removal and directory deletion
    /// to prevent `remove_dir_all` from being called on the project root.
    #[serde(default)]
    pub use_main_worktree: bool,

    /// All agent processes opened in this kild session.
    ///
    /// Populated by `kild create` (initial agent) and `kild open` (additional agents).
    /// `kild stop` clears this vec. Each open operation appends an entry.
    /// Empty for sessions created before multi-agent tracking was added.
    #[serde(default)]
    agents: Vec<AgentProcess>,
}

impl Session {
    /// Create a new Session.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: SessionId,
        project_id: ProjectId,
        branch: BranchName,
        worktree_path: PathBuf,
        agent: String,
        status: SessionStatus,
        created_at: String,
        port_range_start: u16,
        port_range_end: u16,
        port_count: u16,
        last_activity: Option<String>,
        note: Option<String>,
        agents: Vec<AgentProcess>,
        agent_session_id: Option<String>,
        task_list_id: Option<String>,
        runtime_mode: Option<kild_protocol::RuntimeMode>,
    ) -> Self {
        Self {
            id,
            project_id,
            branch,
            worktree_path,
            agent,
            status,
            created_at,
            port_range_start,
            port_range_end,
            port_count,
            last_activity,
            note,
            agents,
            agent_session_id,
            agent_session_id_history: Vec::new(),
            task_list_id,
            runtime_mode,
            use_main_worktree: false,
        }
    }

    /// Returns true if the session's worktree path exists on disk.
    ///
    /// Sessions with missing worktrees are still valid session files
    /// (they can be loaded and listed), but cannot be operated on
    /// (open, etc.) until the worktree issue is resolved.
    ///
    /// Use this to check worktree validity before operations or to
    /// display orphaned status indicators in the UI.
    pub fn is_worktree_valid(&self) -> bool {
        self.worktree_path.exists()
    }

    /// All tracked agent processes in this session.
    pub fn agents(&self) -> &[AgentProcess] {
        &self.agents
    }

    /// Whether this session has any tracked agents.
    pub fn has_agents(&self) -> bool {
        !self.agents.is_empty()
    }

    /// Number of tracked agents.
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// The most recently opened agent (last in the vec).
    pub fn latest_agent(&self) -> Option<&AgentProcess> {
        self.agents.last()
    }

    pub fn latest_agent_mut(&mut self) -> Option<&mut AgentProcess> {
        self.agents.last_mut()
    }

    /// Append an agent to this session's tracking vec.
    pub fn add_agent(&mut self, agent: AgentProcess) {
        self.agents.push(agent);
    }

    /// Remove all tracked agents (called during stop/destroy).
    pub fn clear_agents(&mut self) {
        self.agents.clear();
    }

    /// Set the initial agents vec (called during session creation).
    pub fn set_agents(&mut self, agents: Vec<AgentProcess>) {
        self.agents = agents;
    }

    /// Rotate agent_session_id, preserving the previous ID in history.
    ///
    /// No-op on the history if the new ID is identical to the current one (resume path).
    /// Returns `true` if the previous ID was different and moved to history.
    pub fn rotate_agent_session_id(&mut self, new_id: String) -> bool {
        let rotated = if let Some(prev) = self.agent_session_id.take()
            && prev != new_id
        {
            self.agent_session_id_history.push(prev);
            true
        } else {
            false
        };
        self.agent_session_id = Some(new_id);
        rotated
    }

    /// Create a minimal Session for testing purposes.
    #[cfg(test)]
    pub fn new_for_test(branch: impl Into<BranchName>, worktree_path: PathBuf) -> Self {
        let branch = branch.into();
        Self {
            id: SessionId::new(format!("test-{}", branch)),
            project_id: ProjectId::new("test-project"),
            branch,
            worktree_path,
            agent: "test".to_string(),
            status: SessionStatus::Active,
            created_at: "2026-02-09T10:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            last_activity: None,
            note: None,
            agents: vec![],
            agent_session_id: None,
            agent_session_id_history: Vec::new(),
            task_list_id: None,
            runtime_mode: None,
            use_main_worktree: false,
        }
    }
}
