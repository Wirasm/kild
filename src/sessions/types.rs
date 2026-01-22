use crate::terminal::types::TerminalType;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn default_port_start() -> u16 {
    0
}
fn default_port_end() -> u16 {
    0
}
fn default_port_count() -> u16 {
    0
}
fn default_command() -> String {
    String::default()
}
fn default_last_activity() -> Option<String> {
    None
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
    #[serde(default = "default_command")]
    pub command: String,

    /// Timestamp of last detected activity for health monitoring.
    ///
    /// This tracks when the session was last active for health status calculation.
    /// Used by the health monitoring system to distinguish between Idle, Stuck, and Crashed states.
    /// Initially set to session creation time, updated by activity monitoring.
    ///
    /// Format: RFC3339 timestamp string (e.g., "2024-01-01T12:00:00Z")
    #[serde(default = "default_last_activity")]
    pub last_activity: Option<String>,
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
}

impl CreateSessionRequest {
    pub fn new(branch: String, agent: Option<String>) -> Self {
        Self { branch, agent }
    }

    pub fn agent(&self) -> String {
        self.agent.clone().unwrap_or_else(|| "claude".to_string())
    }

    pub fn agent_or_default(&self, default: &str) -> String {
        self.agent.clone().unwrap_or_else(|| default.to_string())
    }
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
    fn test_create_session_request() {
        let request = CreateSessionRequest::new("test-branch".to_string(), None);
        assert_eq!(request.branch, "test-branch");
        assert_eq!(request.agent(), "claude");

        let request_with_agent =
            CreateSessionRequest::new("test-branch".to_string(), Some("kiro".to_string()));
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
}
