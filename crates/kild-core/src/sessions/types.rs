use crate::git::types::WorktreeStatus;
use crate::terminal::types::TerminalType;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Result of checking if a PR exists for a branch.
///
/// This is a proper enum instead of `Option<bool>` to make the semantics
/// explicit and self-documenting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrCheckResult {
    /// PR exists for this branch (open, merged, or closed).
    Exists,
    /// No PR found for this branch.
    NotFound,
    /// Could not check PR status.
    ///
    /// This happens when:
    /// - The `gh` CLI is not installed
    /// - The `gh` CLI is not authenticated
    /// - Network errors occurred
    /// - The worktree path doesn't exist
    #[default]
    Unavailable,
}

impl PrCheckResult {
    /// Returns true if a PR definitely exists.
    pub fn exists(&self) -> bool {
        matches!(self, PrCheckResult::Exists)
    }

    /// Returns true if we confirmed no PR exists.
    pub fn not_found(&self) -> bool {
        matches!(self, PrCheckResult::NotFound)
    }

    /// Returns true if we couldn't check PR status.
    pub fn is_unavailable(&self) -> bool {
        matches!(self, PrCheckResult::Unavailable)
    }
}

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
                // Build detailed message with file counts
                let mut parts = Vec::new();
                if details.staged_files > 0 {
                    parts.push(format!("{} staged", details.staged_files));
                }
                if details.modified_files > 0 {
                    parts.push(format!("{} modified", details.modified_files));
                }
                if details.untracked_files > 0 {
                    parts.push(format!("{} untracked", details.untracked_files));
                }
                format!("Uncommitted changes: {}", parts.join(", "))
            } else {
                // Fallback when details unavailable
                "Uncommitted changes detected".to_string()
            };
            messages.push(message);
        }

        // Unpushed commits (warning only)
        if self.git_status.unpushed_commit_count > 0 {
            let count = self.git_status.unpushed_commit_count;

            // Use correct singular/plural form
            let message = if count == 1 {
                format!("{} unpushed commit will be lost", count)
            } else {
                format!("{} unpushed commits will be lost", count)
            };

            messages.push(message);
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
    /// PR was not merged (or couldn't be checked), remote branch preserved for future merge
    PrNotMerged,
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
    pub agent: String,
    pub status: SessionStatus,
    pub created_at: String,
    #[serde(default = "default_port_start")]
    pub port_range_start: u16,
    #[serde(default = "default_port_end")]
    pub port_range_end: u16,
    #[serde(default = "default_port_count")]
    pub port_count: u16,

    /// Process ID of the spawned terminal/agent process.
    ///
    /// This is `None` if:
    /// - The session was created before PID tracking was implemented
    /// - The terminal spawn failed to capture the PID
    /// - The session is in a stopped state
    ///
    /// Note: PIDs can be reused by the OS, so this should be validated
    /// against process name/start time before use.
    pub process_id: Option<u32>,

    /// Process name captured at spawn time for PID reuse protection
    pub process_name: Option<String>,

    /// Process start time captured at spawn time for PID reuse protection
    pub process_start_time: Option<u64>,

    /// Terminal type used to launch this session (iTerm, Terminal.app, Ghostty)
    ///
    /// Used to close the terminal window during destroy.
    /// None for sessions created before this field was added.
    #[serde(default)]
    pub terminal_type: Option<TerminalType>,

    /// Terminal window ID for closing the correct window on destroy.
    ///
    /// For iTerm2/Terminal.app: The AppleScript window ID (e.g., "1596")
    /// For Ghostty: The unique title set via ANSI escape sequence
    /// None for: sessions created before this field, or spawn failed to capture ID
    #[serde(default)]
    pub terminal_window_id: Option<String>,

    /// The full command that was executed to start the agent
    ///
    /// This is the actual command passed to the terminal, e.g.,
    /// "kiro-cli chat --trust-all-tools" or "claude-code"
    ///
    /// Empty string for sessions created before this field was added.
    #[serde(default)]
    pub command: String,

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

    /// All agent processes opened in this kild session.
    ///
    /// Populated by `kild create` (initial agent) and `kild open` (additional agents).
    /// `kild stop` clears this vec. Each open operation appends an entry.
    /// Empty for sessions created before multi-agent tracking was added.
    ///
    /// Prefer accessor methods (`agents()`, `add_agent()`, `clear_agents()`,
    /// `latest_agent()`, `has_agents()`, `agent_count()`) over direct field access.
    #[serde(default)]
    pub agents: Vec<AgentProcess>,
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
}

impl Session {
    /// Returns true if the session's worktree path exists on disk.
    ///
    /// Sessions with missing worktrees are still valid session files
    /// (they can be loaded and listed), but cannot be operated on
    /// (open, restart, etc.) until the worktree issue is resolved.
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SessionStatus {
    Active,
    Stopped,
    Destroyed,
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
    pub agent: Option<String>,
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
}

impl CreateSessionRequest {
    pub fn new(branch: String, agent: Option<String>, note: Option<String>) -> Self {
        Self {
            branch,
            agent,
            note,
            project_path: None,
            base_branch: None,
            no_fetch: false,
        }
    }

    /// Create a request with explicit project path (for UI usage)
    pub fn with_project_path(
        branch: String,
        agent: Option<String>,
        note: Option<String>,
        project_path: PathBuf,
    ) -> Self {
        Self {
            branch,
            agent,
            note,
            project_path: Some(project_path),
            base_branch: None,
            no_fetch: false,
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

    pub fn agent(&self) -> String {
        self.agent.clone().unwrap_or_else(|| "claude".to_string())
    }

    pub fn agent_or_default(&self, default: &str) -> String {
        self.agent.clone().unwrap_or_else(|| default.to_string())
    }
}

/// Process status for a kild session.
///
/// Represents whether the agent process is currently running, stopped,
/// or in an unknown state (detection failed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        let session = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 3000,
            port_range_end: 3009,
            port_count: 10,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: "claude-code".to_string(),
            last_activity: Some("2024-01-01T00:00:00Z".to_string()),
            note: None,
            agents: vec![],
        };

        assert_eq!(session.branch, "branch");
        assert_eq!(session.status, SessionStatus::Active);
        assert_eq!(session.command, "claude-code");
    }

    #[test]
    fn test_session_backward_compatibility() {
        // Test that sessions without last_activity field can be deserialized
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
            "port_count": 10,
            "process_id": null,
            "process_name": null,
            "process_start_time": null,
            "command": "claude-code"
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
            "port_count": 10,
            "process_id": null,
            "process_name": null,
            "process_start_time": null,
            "command": "claude-code"
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
            "process_id": null,
            "process_name": null,
            "process_start_time": null,
            "command": "claude-code",
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
            Some("claude".to_string()),
            Some("OAuth2 implementation".to_string()),
        );
        assert_eq!(
            request_with_note.note,
            Some("OAuth2 implementation".to_string())
        );

        // Test request without note
        let request_without_note =
            CreateSessionRequest::new("feature-auth".to_string(), Some("claude".to_string()), None);
        assert_eq!(request_without_note.note, None);
    }

    #[test]
    fn test_create_session_request() {
        let request = CreateSessionRequest::new("test-branch".to_string(), None, None);
        assert_eq!(request.branch, "test-branch");
        assert_eq!(request.agent(), "claude");

        let request_with_agent =
            CreateSessionRequest::new("test-branch".to_string(), Some("kiro".to_string()), None);
        assert_eq!(request_with_agent.agent(), "kiro");
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
    fn test_session_with_terminal_type() {
        let session = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 3000,
            port_range_end: 3009,
            port_count: 10,
            process_id: Some(12345),
            process_name: Some("claude-code".to_string()),
            process_start_time: Some(1234567890),
            terminal_type: Some(TerminalType::ITerm),
            terminal_window_id: Some("1596".to_string()),
            command: "claude-code".to_string(),
            last_activity: Some("2024-01-01T00:00:00Z".to_string()),
            note: None,
            agents: vec![],
        };

        // Test serialization round-trip
        let json = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.terminal_type, Some(TerminalType::ITerm));
        assert_eq!(deserialized.terminal_window_id, Some("1596".to_string()));
    }

    #[test]
    fn test_session_backward_compatibility_terminal_type() {
        // Test that sessions without terminal_type field can be deserialized
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
            "port_count": 10,
            "process_id": null,
            "process_name": null,
            "process_start_time": null,
            "command": "claude-code"
        }"#;

        let session: Session = serde_json::from_str(json_without_terminal_type).unwrap();
        assert_eq!(session.terminal_type, None);
        assert_eq!(session.terminal_window_id, None);
    }

    #[test]
    fn test_session_backward_compatibility_terminal_window_id() {
        // Test that sessions without terminal_window_id field can be deserialized
        // (sessions created before window ID tracking was added)
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
            "port_count": 10,
            "process_id": null,
            "process_name": null,
            "process_start_time": null,
            "terminal_type": "ITerm",
            "command": "claude-code"
        }"#;

        let session: Session = serde_json::from_str(json_without_window_id).unwrap();
        assert_eq!(session.terminal_type, Some(TerminalType::ITerm));
        assert_eq!(session.terminal_window_id, None);
    }

    #[test]
    fn test_create_session_request_with_project_path() {
        let request = CreateSessionRequest::with_project_path(
            "test-branch".to_string(),
            Some("claude".to_string()),
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
        let request = CreateSessionRequest::new("test-branch".to_string(), None, None);
        assert!(request.project_path.is_none());
    }

    #[test]
    fn test_is_worktree_valid_with_existing_path() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_worktree_valid");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let session = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path: temp_dir.clone(),
            agent: "claude".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: "test-command".to_string(),
            last_activity: None,
            note: None,
            agents: vec![],
        };

        assert!(session.is_worktree_valid());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_is_worktree_valid_with_missing_path() {
        let session = Session {
            id: "test/orphaned".to_string(),
            project_id: "test".to_string(),
            branch: "orphaned".to_string(),
            worktree_path: PathBuf::from("/nonexistent/path/that/does/not/exist"),
            agent: "claude".to_string(),
            status: SessionStatus::Stopped,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: "test-command".to_string(),
            last_activity: None,
            note: None,
            agents: vec![],
        };

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
            "port_count": 10,
            "process_id": null,
            "process_name": null,
            "process_start_time": null,
            "command": "claude-code"
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert!(!session.has_agents());
    }

    #[test]
    fn test_session_with_multiple_agents_serialization() {
        let session = Session {
            id: "test/branch".to_string(),
            project_id: "test".to_string(),
            branch: "branch".to_string(),
            worktree_path: PathBuf::from("/tmp/test"),
            agent: "claude".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 3000,
            port_range_end: 3009,
            port_count: 10,
            process_id: Some(12345),
            process_name: Some("claude-code".to_string()),
            process_start_time: Some(1234567890),
            terminal_type: Some(TerminalType::Ghostty),
            terminal_window_id: Some("kild-test".to_string()),
            command: "claude-code".to_string(),
            last_activity: Some("2024-01-01T00:00:00Z".to_string()),
            note: None,
            agents: vec![
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
                )
                .unwrap(),
            ],
        };
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
            "process_id": null,
            "process_name": null,
            "process_start_time": null,
            "command": "claude-code",
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
}
