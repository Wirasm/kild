use serde::{Deserialize, Serialize};

/// PTY session status as reported by the daemon.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Creating,
    Running,
    Stopped,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Creating => write!(f, "creating"),
            SessionStatus::Running => write!(f, "running"),
            SessionStatus::Stopped => write!(f, "stopped"),
        }
    }
}

/// Summary of a daemon session as returned via IPC.
///
/// This is a PTY-centric wire type for the protocol, not the internal
/// `DaemonSession`. The daemon knows about PTYs and processes, not about
/// git worktrees or agents — those concepts live in kild-core.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub working_directory: String,
    pub command: String,
    pub status: SessionStatus,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pty_pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_info_serde() {
        let info = SessionInfo {
            id: "myapp_feature-auth".to_string(),
            working_directory: "/tmp/worktrees/feature-auth".to_string(),
            command: "claude".to_string(),
            status: SessionStatus::Running,
            created_at: "2026-02-09T14:30:00Z".to_string(),
            client_count: Some(2),
            pty_pid: Some(12345),
            exit_code: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains(r#""status":"running""#));
        let parsed: SessionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, info.id);
        assert_eq!(parsed.command, "claude");
        assert_eq!(parsed.status, SessionStatus::Running);
        assert_eq!(parsed.client_count, Some(2));
    }

    #[test]
    fn test_session_info_optional_fields_omitted() {
        let info = SessionInfo {
            id: "test".to_string(),
            working_directory: "/tmp".to_string(),
            command: "bash".to_string(),
            status: SessionStatus::Stopped,
            created_at: "2026-02-09T14:30:00Z".to_string(),
            client_count: None,
            pty_pid: None,
            exit_code: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(!json.contains("client_count"));
        assert!(!json.contains("pty_pid"));
        assert!(!json.contains("exit_code"));
    }

    #[test]
    fn test_session_info_with_exit_code() {
        let info = SessionInfo {
            id: "test".to_string(),
            working_directory: "/tmp".to_string(),
            command: "bash".to_string(),
            status: SessionStatus::Stopped,
            created_at: "2026-02-09T14:30:00Z".to_string(),
            client_count: None,
            pty_pid: None,
            exit_code: Some(1),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"exit_code\":1"));
    }

    #[test]
    fn test_session_info_exit_code_roundtrip() {
        let info = SessionInfo {
            id: "test".to_string(),
            working_directory: "/tmp".to_string(),
            command: "bash".to_string(),
            status: SessionStatus::Stopped,
            created_at: "2026-02-09T14:30:00Z".to_string(),
            client_count: None,
            pty_pid: None,
            exit_code: Some(127),
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: SessionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.exit_code, Some(127));
    }

    #[test]
    fn test_session_status_display() {
        assert_eq!(SessionStatus::Creating.to_string(), "creating");
        assert_eq!(SessionStatus::Running.to_string(), "running");
        assert_eq!(SessionStatus::Stopped.to_string(), "stopped");
    }

    #[test]
    fn test_session_status_roundtrip() {
        for status in [
            SessionStatus::Creating,
            SessionStatus::Running,
            SessionStatus::Stopped,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: SessionStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn test_session_status_wire_format() {
        assert_eq!(
            serde_json::to_string(&SessionStatus::Running).unwrap(),
            r#""running""#
        );
        assert_eq!(
            serde_json::to_string(&SessionStatus::Stopped).unwrap(),
            r#""stopped""#
        );
        assert_eq!(
            serde_json::to_string(&SessionStatus::Creating).unwrap(),
            r#""creating""#
        );
    }
}
