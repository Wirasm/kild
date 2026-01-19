use std::path::PathBuf;
use serde::{Deserialize, Serialize};

fn default_port_start() -> u16 { 0 }
fn default_port_end() -> u16 { 0 }
fn default_port_count() -> u16 { 0 }
fn default_command() -> String { String::default() }

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
    
    /// The full command that was executed to start the agent
    /// 
    /// This is the actual command passed to the terminal, e.g.,
    /// "kiro-cli chat --trust-all-tools" or "claude-code"
    /// 
    /// Empty string for sessions created before this field was added.
    #[serde(default = "default_command")]
    pub command: String,
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
            command: "claude-code".to_string(),
        };

        assert_eq!(session.branch, "branch");
        assert_eq!(session.status, SessionStatus::Active);
        assert_eq!(session.command, "claude-code");
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
}
