pub use crate::forge::types::PrCheckResult;
use crate::git::types::WorktreeStatus;
use crate::terminal::types::TerminalType;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Safety information for a destroy operation.
///
/// Contains git status information and PR check results to help users
/// make informed decisions before destroying a kild.
///
/// # Degraded State
///
/// Check `git_status.status_check_failed` to determine if the safety info
/// is degraded. When degraded, the fallback is conservative (assumes dirty)
/// and a warning message is included.
#[derive(Debug, Clone, Default)]
pub struct DestroySafetyInfo {
    /// Git worktree status (uncommitted changes, unpushed commits, etc.)
    pub git_status: WorktreeStatus,
    /// PR check result for the kild's branch.
    pub pr_status: PrCheckResult,
}

impl DestroySafetyInfo {
    /// Returns true if the destroy should be blocked (requires --force).
    ///
    /// Blocks on:
    /// - Uncommitted changes (cannot be recovered)
    /// - Status check failure with conservative fallback (user should verify manually)
    pub fn should_block(&self) -> bool {
        self.git_status.has_uncommitted_changes
    }

    /// Returns true if there are any warnings to show the user.
    pub fn has_warnings(&self) -> bool {
        self.git_status.has_uncommitted_changes
            || self.git_status.unpushed_commit_count > 0
            || !self.git_status.has_remote_branch
            || self.pr_status.not_found()
            || self.git_status.status_check_failed
    }

    /// Generate warning messages for display.
    ///
    /// Returns a list of human-readable warning messages in severity order:
    /// 1. Status check failures (critical - user should verify manually)
    /// 2. Uncommitted changes (blocking)
    /// 3. Unpushed commits (warning)
    /// 4. Never pushed (warning)
    /// 5. No PR found (advisory)
    pub fn warning_messages(&self) -> Vec<String> {
        let mut messages = Vec::new();

        // Status check failure (critical - shown first)
        if self.git_status.status_check_failed {
            messages
                .push("Git status check failed - could not verify uncommitted changes".to_string());
        }

        // Uncommitted changes (blocking)
        // Skip if status check failed (already showed critical message)
        if self.git_status.has_uncommitted_changes && !self.git_status.status_check_failed {
            let message = if let Some(details) = &self.git_status.uncommitted_details {
                let parts: Vec<String> = [
                    (details.staged_files > 0).then(|| format!("{} staged", details.staged_files)),
                    (details.modified_files > 0)
                        .then(|| format!("{} modified", details.modified_files)),
                    (details.untracked_files > 0)
                        .then(|| format!("{} untracked", details.untracked_files)),
                ]
                .into_iter()
                .flatten()
                .collect();
                format!("Uncommitted changes: {}", parts.join(", "))
            } else {
                "Uncommitted changes detected".to_string()
            };
            messages.push(message);
        }

        // Unpushed commits (warning only)
        if self.git_status.unpushed_commit_count > 0 {
            let count = self.git_status.unpushed_commit_count;
            let commit_word = if count == 1 { "commit" } else { "commits" };
            messages.push(format!("{} unpushed {} will be lost", count, commit_word));
        }

        // Never pushed (warning only) - skip if status check failed or has unpushed commits
        if !self.git_status.has_remote_branch
            && self.git_status.unpushed_commit_count == 0
            && !self.git_status.status_check_failed
        {
            messages.push("Branch has never been pushed".to_string());
        }

        // No PR found (advisory)
        if self.pr_status.not_found() {
            messages.push("No PR found for this branch".to_string());
        }

        messages
    }
}

/// Result of the `complete_session` operation, distinguishing between different outcomes.
#[derive(Debug, Clone, PartialEq)]
pub enum CompleteResult {
    /// PR was merged and remote branch was successfully deleted
    RemoteDeleted,
    /// PR was merged but remote branch deletion failed (logged as warning, non-fatal)
    RemoteDeleteFailed,
    /// PR was not merged, remote branch preserved for future merge
    PrNotMerged,
    /// Could not verify PR merge status (no forge, CLI error, no remote)
    PrCheckUnavailable,
}

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
    pub id: String,
    pub project_id: String,
    pub branch: String,
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
    pub runtime_mode: Option<crate::state::types::RuntimeMode>,

    /// All agent processes opened in this kild session.
    ///
    /// Populated by `kild create` (initial agent) and `kild open` (additional agents).
    /// `kild stop` clears this vec. Each open operation appends an entry.
    /// Empty for sessions created before multi-agent tracking was added.
    #[serde(default)]
    agents: Vec<AgentProcess>,
}

/// Represents a single agent process spawned within a kild session.
///
/// Multiple agents can run concurrently in the same kild via `kild open`.
/// Each open operation appends an `AgentProcess` to the session's `agents` vec.
///
/// Invariant: `process_id`, `process_name`, and `process_start_time` must all
/// be `Some` or all be `None`. This ensures PID reuse protection always has
/// the metadata it needs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(into = "AgentProcessData")]
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
    /// When `Some`, process_id/process_name/process_start_time are `None`
    /// and operations route through `daemon::client` instead of PID-based tracking.
    daemon_session_id: Option<String>,
}

/// Internal serde representation that routes through [`AgentProcess::new`]
/// on deserialization to enforce the PID metadata invariant.
#[derive(Serialize, Deserialize)]
struct AgentProcessData {
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

impl From<AgentProcess> for AgentProcessData {
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

impl TryFrom<AgentProcessData> for AgentProcess {
    type Error = String;

    fn try_from(data: AgentProcessData) -> Result<Self, Self::Error> {
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
        let data = AgentProcessData::deserialize(deserializer)?;
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
    ) -> Result<Self, super::errors::SessionError> {
        // Validate: process_id, process_name, process_start_time must all be present or all absent
        let has_pid = process_id.is_some();
        let has_name = process_name.is_some();
        let has_time = process_start_time.is_some();
        if has_pid != has_name || has_pid != has_time {
            return Err(super::errors::SessionError::InvalidProcessMetadata);
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
}

impl Session {
    /// Create a new Session.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        project_id: String,
        branch: String,
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
        runtime_mode: Option<crate::state::types::RuntimeMode>,
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
            task_list_id,
            runtime_mode,
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

    /// Create a minimal Session for testing purposes.
    #[cfg(test)]
    pub fn new_for_test(branch: String, worktree_path: PathBuf) -> Self {
        Self {
            id: format!("test-{}", branch),
            project_id: "test-project".to_string(),
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
            task_list_id: None,
            runtime_mode: None,
        }
    }
}

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

#[derive(Debug, Clone)]
pub struct ValidatedRequest {
    pub name: String,
    pub command: String,
    pub agent: String,
}

#[derive(Debug, Clone)]
pub struct CreateSessionRequest {
    pub branch: String,
    /// What agent to launch (default from config, specific override, or bare shell).
    pub agent_mode: crate::state::types::AgentMode,
    pub note: Option<String>,
    /// Optional project path for UI context. When provided, this path is used
    /// instead of current working directory for project detection.
    ///
    /// See [`crate::sessions::handler::create_session`] for the branching logic.
    pub project_path: Option<PathBuf>,
    /// Override base branch for this create (CLI --base flag).
    pub base_branch: Option<String>,
    /// Skip fetching before create (CLI --no-fetch flag).
    pub no_fetch: bool,
    /// Whether to launch in an external terminal or daemon-owned PTY.
    pub runtime_mode: crate::state::types::RuntimeMode,
}

impl CreateSessionRequest {
    pub fn new(
        branch: String,
        agent_mode: crate::state::types::AgentMode,
        note: Option<String>,
    ) -> Self {
        Self {
            branch,
            agent_mode,
            note,
            project_path: None,
            base_branch: None,
            no_fetch: false,
            runtime_mode: crate::state::types::RuntimeMode::Terminal,
        }
    }

    /// Create a request with explicit project path (for UI usage)
    pub fn with_project_path(
        branch: String,
        agent_mode: crate::state::types::AgentMode,
        note: Option<String>,
        project_path: PathBuf,
    ) -> Self {
        Self {
            branch,
            agent_mode,
            note,
            project_path: Some(project_path),
            base_branch: None,
            no_fetch: false,
            runtime_mode: crate::state::types::RuntimeMode::Terminal,
        }
    }

    pub fn with_base_branch(mut self, base_branch: Option<String>) -> Self {
        self.base_branch = base_branch;
        self
    }

    pub fn with_no_fetch(mut self, no_fetch: bool) -> Self {
        self.no_fetch = no_fetch;
        self
    }

    pub fn with_runtime_mode(mut self, mode: crate::state::types::RuntimeMode) -> Self {
        self.runtime_mode = mode;
        self
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = Session::new(
            "test/branch".to_string(),
            "test".to_string(),
            "branch".to_string(),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            3000,
            3009,
            10,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        assert_eq!(session.branch, "branch");
        assert_eq!(session.status, SessionStatus::Active);
    }

    #[test]
    fn test_session_backward_compatibility() {
        // Test that sessions without last_activity field can be deserialized
        // (old JSON may contain removed fields like process_id, etc. - serde ignores unknown fields)
        let json_without_last_activity = r#"{
            "id": "test/branch",
            "project_id": "test",
            "branch": "branch",
            "worktree_path": "/tmp/test",
            "agent": "claude",
            "status": "Active",
            "created_at": "2024-01-01T00:00:00Z",
            "port_range_start": 3000,
            "port_range_end": 3009,
            "port_count": 10
        }"#;

        let session: Session = serde_json::from_str(json_without_last_activity).unwrap();
        assert_eq!(session.last_activity, None);
        assert_eq!(session.branch, "branch");
    }

    #[test]
    fn test_session_backward_compatibility_note() {
        // Test that sessions without note field can be deserialized
        let json_without_note = r#"{
            "id": "test/branch",
            "project_id": "test",
            "branch": "branch",
            "worktree_path": "/tmp/test",
            "agent": "claude",
            "status": "Active",
            "created_at": "2024-01-01T00:00:00Z",
            "port_range_start": 3000,
            "port_range_end": 3009,
            "port_count": 10
        }"#;

        let session: Session = serde_json::from_str(json_without_note).unwrap();
        assert_eq!(session.note, None);
        assert_eq!(session.branch, "branch");
    }

    #[test]
    fn test_session_with_note_serialization_roundtrip() {
        // Test that sessions WITH notes serialize and deserialize correctly
        let json_with_note = r#"{
            "id": "test/branch",
            "project_id": "test",
            "branch": "branch",
            "worktree_path": "/tmp/test",
            "agent": "claude",
            "status": "Active",
            "created_at": "2024-01-01T00:00:00Z",
            "port_range_start": 3000,
            "port_range_end": 3009,
            "port_count": 10,
            "note": "Implementing auth feature with OAuth2 support"
        }"#;

        let session: Session = serde_json::from_str(json_with_note).unwrap();
        assert_eq!(
            session.note,
            Some("Implementing auth feature with OAuth2 support".to_string())
        );

        // Verify round-trip preserves note
        let serialized = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.note, session.note);
    }

    #[test]
    fn test_create_session_request_with_note() {
        // Test CreateSessionRequest properly includes note
        let request_with_note = CreateSessionRequest::new(
            "feature-auth".to_string(),
            crate::state::types::AgentMode::Agent("claude".to_string()),
            Some("OAuth2 implementation".to_string()),
        );
        assert_eq!(
            request_with_note.note,
            Some("OAuth2 implementation".to_string())
        );

        // Test request without note
        let request_without_note = CreateSessionRequest::new(
            "feature-auth".to_string(),
            crate::state::types::AgentMode::Agent("claude".to_string()),
            None,
        );
        assert_eq!(request_without_note.note, None);
    }

    #[test]
    fn test_create_session_request_agent_mode() {
        use crate::state::types::AgentMode;

        let request =
            CreateSessionRequest::new("test-branch".to_string(), AgentMode::DefaultAgent, None);
        assert_eq!(request.branch, "test-branch");
        assert_eq!(request.agent_mode, AgentMode::DefaultAgent);

        let request_with_agent = CreateSessionRequest::new(
            "test-branch".to_string(),
            AgentMode::Agent("kiro".to_string()),
            None,
        );
        assert_eq!(
            request_with_agent.agent_mode,
            AgentMode::Agent("kiro".to_string())
        );

        let request_bare_shell =
            CreateSessionRequest::new("test-branch".to_string(), AgentMode::BareShell, None);
        assert_eq!(request_bare_shell.agent_mode, AgentMode::BareShell);
    }

    #[test]
    fn test_validated_request() {
        let validated = ValidatedRequest {
            name: "test".to_string(),
            command: "echo hello".to_string(),
            agent: "claude".to_string(),
        };

        assert_eq!(validated.name, "test");
        assert_eq!(validated.command, "echo hello");
    }

    #[test]
    fn test_session_with_terminal_type_in_agent() {
        let agent = AgentProcess::new(
            "claude".to_string(),
            "test_0".to_string(),
            Some(12345),
            Some("claude-code".to_string()),
            Some(1234567890),
            Some(TerminalType::ITerm),
            Some("1596".to_string()),
            "claude-code".to_string(),
            "2024-01-01T00:00:00Z".to_string(),
            None,
        )
        .unwrap();
        let session = Session::new(
            "test/branch".to_string(),
            "test".to_string(),
            "branch".to_string(),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            3000,
            3009,
            10,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![agent],
            None,
            None,
            None,
        );

        // Test serialization round-trip
        let json = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&json).unwrap();
        let latest = deserialized.latest_agent().unwrap();
        assert_eq!(latest.terminal_type(), Some(&TerminalType::ITerm));
        assert_eq!(latest.terminal_window_id(), Some("1596"));
    }

    #[test]
    fn test_session_backward_compatibility_terminal_type() {
        // Test that old session JSON (with removed fields) can still be deserialized
        // serde ignores unknown fields by default
        let json_without_terminal_type = r#"{
            "id": "test/branch",
            "project_id": "test",
            "branch": "branch",
            "worktree_path": "/tmp/test",
            "agent": "claude",
            "status": "Active",
            "created_at": "2024-01-01T00:00:00Z",
            "port_range_start": 3000,
            "port_range_end": 3009,
            "port_count": 10
        }"#;

        let session: Session = serde_json::from_str(json_without_terminal_type).unwrap();
        assert!(!session.has_agents());
    }

    #[test]
    fn test_session_backward_compatibility_terminal_window_id() {
        // Test that old session JSON with removed fields still deserializes
        // (serde ignores unknown fields like terminal_type, process_id, etc.)
        let json_without_window_id = r#"{
            "id": "test/branch",
            "project_id": "test",
            "branch": "branch",
            "worktree_path": "/tmp/test",
            "agent": "claude",
            "status": "Active",
            "created_at": "2024-01-01T00:00:00Z",
            "port_range_start": 3000,
            "port_range_end": 3009,
            "port_count": 10
        }"#;

        let session: Session = serde_json::from_str(json_without_window_id).unwrap();
        assert_eq!(session.branch, "branch");
        assert!(!session.has_agents());
    }

    #[test]
    fn test_create_session_request_with_project_path() {
        let request = CreateSessionRequest::with_project_path(
            "test-branch".to_string(),
            crate::state::types::AgentMode::Agent("claude".to_string()),
            None,
            PathBuf::from("/path/to/project"),
        );
        assert_eq!(request.branch, "test-branch");
        assert_eq!(
            request.project_path,
            Some(PathBuf::from("/path/to/project"))
        );
    }

    #[test]
    fn test_create_session_request_new_has_no_project_path() {
        let request = CreateSessionRequest::new(
            "test-branch".to_string(),
            crate::state::types::AgentMode::DefaultAgent,
            None,
        );
        assert!(request.project_path.is_none());
    }

    #[test]
    fn test_is_worktree_valid_with_existing_path() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_worktree_valid");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let session = Session::new(
            "test/branch".to_string(),
            "test".to_string(),
            "branch".to_string(),
            temp_dir.clone(),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            None,
            None,
            vec![],
            None,
            None,
            None,
        );

        assert!(session.is_worktree_valid());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_is_worktree_valid_with_missing_path() {
        let session = Session::new(
            "test/orphaned".to_string(),
            "test".to_string(),
            "orphaned".to_string(),
            PathBuf::from("/nonexistent/path/that/does/not/exist"),
            "claude".to_string(),
            SessionStatus::Stopped,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            None,
            None,
            vec![],
            None,
            None,
            None,
        );

        assert!(!session.is_worktree_valid());
    }

    // --- PrCheckResult tests ---

    #[test]
    fn test_pr_check_result_exists() {
        let result = PrCheckResult::Exists;
        assert!(result.exists());
        assert!(!result.not_found());
        assert!(!result.is_unavailable());
    }

    #[test]
    fn test_pr_check_result_not_found() {
        let result = PrCheckResult::NotFound;
        assert!(!result.exists());
        assert!(result.not_found());
        assert!(!result.is_unavailable());
    }

    #[test]
    fn test_pr_check_result_unavailable() {
        let result = PrCheckResult::Unavailable;
        assert!(!result.exists());
        assert!(!result.not_found());
        assert!(result.is_unavailable());
    }

    #[test]
    fn test_pr_check_result_default() {
        let result = PrCheckResult::default();
        assert!(result.is_unavailable());
    }

    // --- DestroySafetyInfo tests ---

    #[test]
    fn test_should_block_on_uncommitted_changes() {
        use crate::git::types::WorktreeStatus;

        let info = DestroySafetyInfo {
            git_status: WorktreeStatus {
                has_uncommitted_changes: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(info.should_block());
    }

    #[test]
    fn test_should_not_block_on_unpushed_only() {
        use crate::git::types::WorktreeStatus;

        let info = DestroySafetyInfo {
            git_status: WorktreeStatus {
                has_uncommitted_changes: false,
                unpushed_commit_count: 5,
                has_remote_branch: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(!info.should_block());
        assert!(info.has_warnings());
    }

    #[test]
    fn test_should_block_on_status_check_failed() {
        use crate::git::types::WorktreeStatus;

        // When status check fails, has_uncommitted_changes defaults to true (conservative)
        let info = DestroySafetyInfo {
            git_status: WorktreeStatus {
                has_uncommitted_changes: true,
                status_check_failed: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(info.should_block());
        assert!(info.has_warnings());
    }

    #[test]
    fn test_has_warnings_no_pr() {
        let info = DestroySafetyInfo {
            pr_status: PrCheckResult::NotFound,
            ..Default::default()
        };
        assert!(info.has_warnings());
    }

    #[test]
    fn test_has_warnings_pr_unavailable_no_warning() {
        use crate::git::types::WorktreeStatus;

        // When gh CLI unavailable, we shouldn't warn about PR
        let info = DestroySafetyInfo {
            pr_status: PrCheckResult::Unavailable,
            git_status: WorktreeStatus {
                has_remote_branch: true,
                ..Default::default()
            },
        };
        assert!(!info.has_warnings());
    }

    #[test]
    fn test_has_warnings_never_pushed() {
        use crate::git::types::WorktreeStatus;

        let info = DestroySafetyInfo {
            git_status: WorktreeStatus {
                has_remote_branch: false,
                unpushed_commit_count: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(info.has_warnings());
    }

    #[test]
    fn test_warning_messages_uncommitted_with_details() {
        use crate::git::types::{UncommittedDetails, WorktreeStatus};

        let info = DestroySafetyInfo {
            git_status: WorktreeStatus {
                has_uncommitted_changes: true,
                uncommitted_details: Some(UncommittedDetails {
                    staged_files: 2,
                    modified_files: 3,
                    untracked_files: 1,
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let msgs = info.warning_messages();
        assert!(!msgs.is_empty());
        assert!(msgs[0].contains("2 staged"));
        assert!(msgs[0].contains("3 modified"));
        assert!(msgs[0].contains("1 untracked"));
    }

    #[test]
    fn test_warning_messages_singular_commit() {
        use crate::git::types::WorktreeStatus;

        let info = DestroySafetyInfo {
            git_status: WorktreeStatus {
                unpushed_commit_count: 1,
                has_remote_branch: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let msgs = info.warning_messages();
        assert!(msgs.iter().any(|m| m.contains("1 unpushed commit")));
        // Ensure singular "commit" not plural "commits"
        assert!(!msgs.iter().any(|m| m.contains("1 unpushed commits")));
    }

    #[test]
    fn test_warning_messages_plural_commits() {
        use crate::git::types::WorktreeStatus;

        let info = DestroySafetyInfo {
            git_status: WorktreeStatus {
                unpushed_commit_count: 3,
                has_remote_branch: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let msgs = info.warning_messages();
        assert!(msgs.iter().any(|m| m.contains("3 unpushed commits")));
    }

    #[test]
    fn test_warning_messages_never_pushed_not_shown_with_unpushed() {
        use crate::git::types::WorktreeStatus;

        // When there are unpushed commits, "never pushed" is redundant
        let info = DestroySafetyInfo {
            git_status: WorktreeStatus {
                unpushed_commit_count: 5,
                has_remote_branch: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let msgs = info.warning_messages();
        assert!(!msgs.iter().any(|m| m.contains("never been pushed")));
        assert!(msgs.iter().any(|m| m.contains("unpushed")));
    }

    #[test]
    fn test_warning_messages_status_check_failed() {
        use crate::git::types::WorktreeStatus;

        let info = DestroySafetyInfo {
            git_status: WorktreeStatus {
                has_uncommitted_changes: true,
                status_check_failed: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let msgs = info.warning_messages();
        assert!(msgs.iter().any(|m| m.contains("Git status check failed")));
        // Should NOT show "Uncommitted changes" message when status check failed
        // (we show the failure message instead)
        assert!(!msgs.iter().any(|m| m.starts_with("Uncommitted changes:")));
    }

    #[test]
    fn test_warning_messages_no_warnings() {
        use crate::git::types::WorktreeStatus;

        let info = DestroySafetyInfo {
            git_status: WorktreeStatus {
                has_uncommitted_changes: false,
                unpushed_commit_count: 0,
                has_remote_branch: true,
                ..Default::default()
            },
            pr_status: PrCheckResult::Exists,
        };
        assert!(!info.has_warnings());
        assert!(info.warning_messages().is_empty());
    }

    // --- AgentProcess and multi-agent tests ---

    #[test]
    fn test_agent_process_rejects_inconsistent_process_metadata() {
        // pid without name/time
        let result = AgentProcess::new(
            "claude".to_string(),
            String::new(),
            Some(12345),
            None,
            None,
            None,
            None,
            "cmd".to_string(),
            "2024-01-01T00:00:00Z".to_string(),
            None,
        );
        assert!(result.is_err());

        // pid + name without time
        let result = AgentProcess::new(
            "claude".to_string(),
            String::new(),
            Some(12345),
            Some("claude-code".to_string()),
            None,
            None,
            None,
            "cmd".to_string(),
            "2024-01-01T00:00:00Z".to_string(),
            None,
        );
        assert!(result.is_err());

        // all None is valid
        let result = AgentProcess::new(
            "claude".to_string(),
            String::new(),
            None,
            None,
            None,
            None,
            None,
            "cmd".to_string(),
            "2024-01-01T00:00:00Z".to_string(),
            None,
        );
        assert!(result.is_ok());

        // all Some is valid
        let result = AgentProcess::new(
            "claude".to_string(),
            String::new(),
            Some(12345),
            Some("claude-code".to_string()),
            Some(1705318200),
            None,
            None,
            "cmd".to_string(),
            "2024-01-01T00:00:00Z".to_string(),
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_agent_process_serialization_roundtrip() {
        let agent = AgentProcess::new(
            "claude".to_string(),
            "test_0".to_string(),
            Some(12345),
            Some("claude-code".to_string()),
            Some(1705318200),
            Some(TerminalType::Ghostty),
            Some("kild-test".to_string()),
            "claude-code".to_string(),
            "2024-01-15T10:30:00Z".to_string(),
            None,
        )
        .unwrap();
        let json = serde_json::to_string(&agent).unwrap();
        let deserialized: AgentProcess = serde_json::from_str(&json).unwrap();
        assert_eq!(agent, deserialized);
    }

    #[test]
    fn test_session_with_agents_backward_compat() {
        // Old session JSON without "agents" field should deserialize with empty vec
        let json = r#"{
            "id": "test",
            "project_id": "test-project",
            "branch": "test-branch",
            "worktree_path": "/tmp/test",
            "agent": "claude",
            "status": "Active",
            "created_at": "2024-01-01T00:00:00Z",
            "port_range_start": 3000,
            "port_range_end": 3009,
            "port_count": 10
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert!(!session.has_agents());
    }

    #[test]
    fn test_session_with_multiple_agents_serialization() {
        let session = Session::new(
            "test/branch".to_string(),
            "test".to_string(),
            "branch".to_string(),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            3000,
            3009,
            10,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![
                AgentProcess::new(
                    "claude".to_string(),
                    "test_0".to_string(),
                    Some(12345),
                    Some("claude-code".to_string()),
                    Some(1234567890),
                    Some(TerminalType::Ghostty),
                    Some("kild-test".to_string()),
                    "claude-code".to_string(),
                    "2024-01-01T00:00:00Z".to_string(),
                    None,
                )
                .unwrap(),
                AgentProcess::new(
                    "kiro".to_string(),
                    "test_1".to_string(),
                    Some(67890),
                    Some("kiro-cli".to_string()),
                    Some(1234567900),
                    Some(TerminalType::Ghostty),
                    Some("kild-test-2".to_string()),
                    "kiro-cli chat".to_string(),
                    "2024-01-01T00:01:00Z".to_string(),
                    None,
                )
                .unwrap(),
            ],
            None,
            None,
            None,
        );
        let json = serde_json::to_string_pretty(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.agent_count(), 2);
        assert_eq!(deserialized.agents()[0].agent(), "claude");
        assert_eq!(deserialized.agents()[1].agent(), "kiro");
    }

    #[test]
    fn test_agent_process_deserialization_rejects_inconsistent_metadata() {
        // JSON with process_id but missing process_name/process_start_time
        let json = r#"{
            "agent": "claude",
            "process_id": 12345,
            "process_name": null,
            "process_start_time": null,
            "terminal_type": null,
            "terminal_window_id": null,
            "command": "cmd",
            "opened_at": "2024-01-01T00:00:00Z"
        }"#;
        let result: Result<AgentProcess, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Invalid process metadata"),
            "Expected InvalidProcessMetadata error, got: {}",
            err
        );
    }

    #[test]
    fn test_agent_process_deserialization_accepts_consistent_metadata() {
        // All Some
        let json = r#"{
            "agent": "claude",
            "process_id": 12345,
            "process_name": "claude-code",
            "process_start_time": 1705318200,
            "terminal_type": null,
            "terminal_window_id": null,
            "command": "cmd",
            "opened_at": "2024-01-01T00:00:00Z"
        }"#;
        let result: Result<AgentProcess, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        // All None
        let json = r#"{
            "agent": "claude",
            "process_id": null,
            "process_name": null,
            "process_start_time": null,
            "terminal_type": null,
            "terminal_window_id": null,
            "command": "cmd",
            "opened_at": "2024-01-01T00:00:00Z"
        }"#;
        let result: Result<AgentProcess, _> = serde_json::from_str(json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_with_corrupted_agent_fails_to_deserialize() {
        // Session JSON where an agent has inconsistent metadata
        let json = r#"{
            "id": "test",
            "project_id": "test-project",
            "branch": "test-branch",
            "worktree_path": "/tmp/test",
            "agent": "claude",
            "status": "Active",
            "created_at": "2024-01-01T00:00:00Z",
            "port_range_start": 3000,
            "port_range_end": 3009,
            "port_count": 10,
            "agents": [
                {
                    "agent": "claude",
                    "process_id": 12345,
                    "process_name": null,
                    "process_start_time": null,
                    "terminal_type": null,
                    "terminal_window_id": null,
                    "command": "cmd",
                    "opened_at": "2024-01-01T00:00:00Z"
                }
            ]
        }"#;
        let result: Result<Session, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_agent_status_display() {
        assert_eq!(AgentStatus::Working.to_string(), "working");
        assert_eq!(AgentStatus::Idle.to_string(), "idle");
        assert_eq!(AgentStatus::Waiting.to_string(), "waiting");
        assert_eq!(AgentStatus::Done.to_string(), "done");
        assert_eq!(AgentStatus::Error.to_string(), "error");
    }

    #[test]
    fn test_agent_status_from_str() {
        assert_eq!(
            "working".parse::<AgentStatus>().unwrap(),
            AgentStatus::Working
        );
        assert_eq!("idle".parse::<AgentStatus>().unwrap(), AgentStatus::Idle);
        assert_eq!(
            "waiting".parse::<AgentStatus>().unwrap(),
            AgentStatus::Waiting
        );
        assert_eq!("done".parse::<AgentStatus>().unwrap(), AgentStatus::Done);
        assert_eq!("error".parse::<AgentStatus>().unwrap(), AgentStatus::Error);
    }

    #[test]
    fn test_agent_status_from_str_invalid() {
        let err = "invalid".parse::<AgentStatus>().unwrap_err();
        assert!(err.contains("Invalid agent status"));
        assert!(err.contains("invalid"));
    }

    #[test]
    fn test_agent_status_serde_roundtrip() {
        for status in [
            AgentStatus::Working,
            AgentStatus::Idle,
            AgentStatus::Waiting,
            AgentStatus::Done,
            AgentStatus::Error,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: AgentStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn test_agent_status_serde_lowercase() {
        let json = serde_json::to_string(&AgentStatus::Working).unwrap();
        assert_eq!(json, r#""working""#);
    }

    #[test]
    fn test_agent_status_info_serde_roundtrip() {
        let info = AgentStatusInfo {
            status: AgentStatus::Working,
            updated_at: "2026-02-05T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: AgentStatusInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, info);
    }

    // --- agent_session_id tests ---

    #[test]
    fn test_session_backward_compatibility_agent_session_id() {
        // Old session JSON without agent_session_id should deserialize with None
        let json = r#"{
            "id": "test/branch",
            "project_id": "test",
            "branch": "branch",
            "worktree_path": "/tmp/test",
            "agent": "claude",
            "status": "Active",
            "created_at": "2024-01-01T00:00:00Z",
            "port_range_start": 3000,
            "port_range_end": 3009,
            "port_count": 10
        }"#;

        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.agent_session_id, None);
        assert_eq!(session.branch, "branch");
    }

    #[test]
    fn test_session_with_agent_session_id_roundtrip() {
        let session = Session::new(
            "test/branch".to_string(),
            "test".to_string(),
            "branch".to_string(),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            3000,
            3009,
            10,
            None,
            None,
            vec![],
            Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
            None,
            None,
        );

        assert_eq!(
            session.agent_session_id,
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );

        // Verify round-trip preserves agent_session_id
        let serialized = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.agent_session_id, session.agent_session_id);
    }

    #[test]
    fn test_session_agent_session_id_survives_clear_agents() {
        let mut session = Session::new(
            "test/branch".to_string(),
            "test".to_string(),
            "branch".to_string(),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            3000,
            3009,
            10,
            None,
            None,
            vec![],
            Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
            None,
            None,
        );

        // Simulate stop: clear agents
        session.clear_agents();
        session.status = SessionStatus::Stopped;

        // agent_session_id should survive clear_agents()
        assert_eq!(
            session.agent_session_id,
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
        assert!(!session.has_agents());
    }

    // --- runtime_mode tests ---

    #[test]
    fn test_session_backward_compatibility_runtime_mode() {
        // Old session JSON without runtime_mode should deserialize with None
        let json = r#"{
            "id": "test/branch",
            "project_id": "test",
            "branch": "branch",
            "worktree_path": "/tmp/test",
            "agent": "claude",
            "status": "Active",
            "created_at": "2024-01-01T00:00:00Z",
            "port_range_start": 3000,
            "port_range_end": 3009,
            "port_count": 10
        }"#;

        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.runtime_mode, None);
    }

    #[test]
    fn test_session_with_runtime_mode_roundtrip() {
        use crate::state::types::RuntimeMode;

        let session = Session::new(
            "test/branch".to_string(),
            "test".to_string(),
            "branch".to_string(),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            3000,
            3009,
            10,
            None,
            None,
            vec![],
            None,
            None,
            Some(RuntimeMode::Daemon),
        );

        assert_eq!(session.runtime_mode, Some(RuntimeMode::Daemon));

        // Verify round-trip preserves runtime_mode
        let serialized = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.runtime_mode, Some(RuntimeMode::Daemon));
    }

    #[test]
    fn test_session_runtime_mode_survives_clear_agents() {
        use crate::state::types::RuntimeMode;

        let mut session = Session::new(
            "test/branch".to_string(),
            "test".to_string(),
            "branch".to_string(),
            PathBuf::from("/tmp/test"),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            3000,
            3009,
            10,
            None,
            None,
            vec![],
            None,
            None,
            Some(RuntimeMode::Daemon),
        );

        // Simulate stop: clear agents
        session.clear_agents();
        session.status = SessionStatus::Stopped;

        // runtime_mode should survive clear_agents()
        assert_eq!(session.runtime_mode, Some(RuntimeMode::Daemon));
        assert!(!session.has_agents());
    }

    // --- Enum serialization normalization tests ---

    #[test]
    fn test_session_status_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&SessionStatus::Active).unwrap(),
            r#""active""#
        );
        assert_eq!(
            serde_json::to_string(&SessionStatus::Stopped).unwrap(),
            r#""stopped""#
        );
        assert_eq!(
            serde_json::to_string(&SessionStatus::Destroyed).unwrap(),
            r#""destroyed""#
        );
    }

    #[test]
    fn test_session_status_deserializes_old_pascal_case() {
        assert_eq!(
            serde_json::from_str::<SessionStatus>(r#""Active""#).unwrap(),
            SessionStatus::Active
        );
        assert_eq!(
            serde_json::from_str::<SessionStatus>(r#""Stopped""#).unwrap(),
            SessionStatus::Stopped
        );
        assert_eq!(
            serde_json::from_str::<SessionStatus>(r#""Destroyed""#).unwrap(),
            SessionStatus::Destroyed
        );
    }

    #[test]
    fn test_session_status_roundtrip_new_format() {
        for status in [
            SessionStatus::Active,
            SessionStatus::Stopped,
            SessionStatus::Destroyed,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: SessionStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn test_process_status_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&ProcessStatus::Running).unwrap(),
            r#""running""#
        );
        assert_eq!(
            serde_json::to_string(&ProcessStatus::Stopped).unwrap(),
            r#""stopped""#
        );
        assert_eq!(
            serde_json::to_string(&ProcessStatus::Unknown).unwrap(),
            r#""unknown""#
        );
    }

    #[test]
    fn test_process_status_roundtrip() {
        for status in [
            ProcessStatus::Running,
            ProcessStatus::Stopped,
            ProcessStatus::Unknown,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: ProcessStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn test_session_with_old_pascal_case_status_deserializes() {
        let json = r#"{
            "id": "test/branch",
            "project_id": "test",
            "branch": "branch",
            "worktree_path": "/tmp/test",
            "agent": "claude",
            "status": "Active",
            "created_at": "2024-01-01T00:00:00Z",
            "port_range_start": 3000,
            "port_range_end": 3009,
            "port_count": 10
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.status, SessionStatus::Active);

        // Verify re-serialization outputs snake_case
        let reserialized = serde_json::to_string(&session).unwrap();
        assert!(reserialized.contains(r#""status":"active""#));
    }
}
