use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: String,
    pub project_id: String,
    pub branch: String,
    pub worktree_path: PathBuf,
    pub agent: String,
    pub status: SessionStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
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
        };

        assert_eq!(session.branch, "branch");
        assert_eq!(session.status, SessionStatus::Active);
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
