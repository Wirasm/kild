use super::*;
use std::path::PathBuf;

#[test]
fn test_session_creation() {
    let session = Session::new(
        "test/branch".into(),
        "test".into(),
        "branch".into(),
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

    assert_eq!(&*session.branch, "branch");
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
    assert_eq!(&*session.branch, "branch");
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
    assert_eq!(&*session.branch, "branch");
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
    assert_eq!(&*request.branch, "test-branch");
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
        name: "test".into(),
        command: "echo hello".to_string(),
        agent: "claude".to_string(),
    };

    assert_eq!(&*validated.name, "test");
    assert_eq!(validated.command, "echo hello");
}

#[test]
fn test_session_with_terminal_type_in_agent() {
    use crate::terminal::types::TerminalType;

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
        "test/branch".into(),
        "test".into(),
        "branch".into(),
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
    assert_eq!(&*session.branch, "branch");
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
    assert_eq!(&*request.branch, "test-branch");
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
        "test/branch".into(),
        "test".into(),
        "branch".into(),
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
        "test/orphaned".into(),
        "test".into(),
        "orphaned".into(),
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
    use crate::terminal::types::TerminalType;

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
    use crate::terminal::types::TerminalType;

    let session = Session::new(
        "test/branch".into(),
        "test".into(),
        "branch".into(),
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
    assert_eq!(&*session.branch, "branch");
}

#[test]
fn test_session_with_agent_session_id_roundtrip() {
    let session = Session::new(
        "test/branch".into(),
        "test".into(),
        "branch".into(),
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
        "test/branch".into(),
        "test".into(),
        "branch".into(),
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
        "test/branch".into(),
        "test".into(),
        "branch".into(),
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
        "test/branch".into(),
        "test".into(),
        "branch".into(),
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
fn test_session_new_sets_all_fields() {
    use crate::state::types::RuntimeMode;

    let session = Session::new(
        "proj/feature".into(),
        "proj".into(),
        "feature".into(),
        PathBuf::from("/worktrees/feature"),
        "claude".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        3000,
        3009,
        10,
        Some("2024-01-01T12:00:00Z".to_string()),
        Some("Auth feature".to_string()),
        vec![],
        Some("sid-123".to_string()),
        Some("tl-456".to_string()),
        Some(RuntimeMode::Daemon),
    );

    assert_eq!(&*session.id, "proj/feature");
    assert_eq!(&*session.project_id, "proj");
    assert_eq!(&*session.branch, "feature");
    assert_eq!(session.worktree_path, PathBuf::from("/worktrees/feature"));
    assert_eq!(session.agent, "claude");
    assert_eq!(session.status, SessionStatus::Active);
    assert_eq!(session.created_at, "2024-01-01T00:00:00Z");
    assert_eq!(session.port_range_start, 3000);
    assert_eq!(session.port_range_end, 3009);
    assert_eq!(session.port_count, 10);
    assert_eq!(
        session.last_activity,
        Some("2024-01-01T12:00:00Z".to_string())
    );
    assert_eq!(session.note, Some("Auth feature".to_string()));
    assert_eq!(session.agent_session_id, Some("sid-123".to_string()));
    assert_eq!(session.task_list_id, Some("tl-456".to_string()));
    assert_eq!(session.runtime_mode, Some(RuntimeMode::Daemon));
    assert!(!session.has_agents());
    assert_eq!(session.agent_count(), 0);
}

#[test]
fn test_agent_process_accessor_methods() {
    use crate::terminal::types::TerminalType;

    let agent = AgentProcess::new(
        "claude".to_string(),
        "proj_feat_0".to_string(),
        Some(42),
        Some("claude-code".to_string()),
        Some(1705318200),
        Some(TerminalType::Ghostty),
        Some("kild-win".to_string()),
        "claude --print".to_string(),
        "2024-01-15T10:30:00Z".to_string(),
        Some("daemon-sid-abc".to_string()),
    )
    .unwrap();

    assert_eq!(agent.agent(), "claude");
    assert_eq!(agent.spawn_id(), "proj_feat_0");
    assert_eq!(agent.process_id(), Some(42));
    assert_eq!(agent.process_name(), Some("claude-code"));
    assert_eq!(agent.process_start_time(), Some(1705318200));
    assert_eq!(agent.terminal_type(), Some(&TerminalType::Ghostty));
    assert_eq!(agent.terminal_window_id(), Some("kild-win"));
    assert_eq!(agent.command(), "claude --print");
    assert_eq!(agent.opened_at(), "2024-01-15T10:30:00Z");
    assert_eq!(agent.daemon_session_id(), Some("daemon-sid-abc"));
}

#[test]
fn test_agent_process_daemon_only_no_pid() {
    let agent = AgentProcess::new(
        "claude".to_string(),
        "proj_feat_0".to_string(),
        None,
        None,
        None,
        None,
        None,
        "claude --print".to_string(),
        "2024-01-01T00:00:00Z".to_string(),
        Some("daemon-session-1".to_string()),
    )
    .unwrap();

    assert!(agent.process_id().is_none());
    assert!(agent.process_name().is_none());
    assert!(agent.process_start_time().is_none());
    assert!(agent.terminal_type().is_none());
    assert!(agent.terminal_window_id().is_none());
    assert_eq!(agent.daemon_session_id(), Some("daemon-session-1"));
}

#[test]
fn test_session_agent_methods() {
    let agents = vec![
        AgentProcess::new(
            "claude".to_string(),
            "test_0".to_string(),
            None,
            None,
            None,
            None,
            None,
            "cmd1".to_string(),
            "2024-01-01T00:00:00Z".to_string(),
            None,
        )
        .unwrap(),
        AgentProcess::new(
            "kiro".to_string(),
            "test_1".to_string(),
            None,
            None,
            None,
            None,
            None,
            "cmd2".to_string(),
            "2024-01-01T00:01:00Z".to_string(),
            None,
        )
        .unwrap(),
    ];

    let session = Session::new(
        "test/branch".into(),
        "test".into(),
        "branch".into(),
        PathBuf::from("/tmp/test"),
        "kiro".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        0,
        0,
        0,
        None,
        None,
        agents,
        None,
        None,
        None,
    );

    assert!(session.has_agents());
    assert_eq!(session.agent_count(), 2);
    assert_eq!(session.agents().len(), 2);
    assert_eq!(session.agents()[0].agent(), "claude");
    assert_eq!(session.agents()[1].agent(), "kiro");
    assert_eq!(session.latest_agent().unwrap().agent(), "kiro");
}

#[test]
fn test_session_add_and_clear_agents() {
    let mut session = Session::new_for_test("test".to_string(), PathBuf::from("/tmp/test"));

    assert!(!session.has_agents());
    assert_eq!(session.agent_count(), 0);
    assert!(session.latest_agent().is_none());

    let agent = AgentProcess::new(
        "claude".to_string(),
        "test_0".to_string(),
        None,
        None,
        None,
        None,
        None,
        "cmd".to_string(),
        "2024-01-01T00:00:00Z".to_string(),
        None,
    )
    .unwrap();

    session.add_agent(agent);
    assert!(session.has_agents());
    assert_eq!(session.agent_count(), 1);

    session.clear_agents();
    assert!(!session.has_agents());
    assert_eq!(session.agent_count(), 0);
}

#[test]
fn test_session_new_for_test_helper() {
    let session = Session::new_for_test("my-branch".to_string(), PathBuf::from("/tmp/wt"));
    assert_eq!(&*session.id, "test-my-branch");
    assert_eq!(&*session.project_id, "test-project");
    assert_eq!(&*session.branch, "my-branch");
    assert_eq!(session.agent, "test");
    assert_eq!(session.status, SessionStatus::Active);
    assert!(!session.has_agents());
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

#[test]
fn test_use_main_worktree_serde_roundtrip() {
    let mut session = Session::new_for_test("honryu", PathBuf::from("/tmp/project"));
    session.use_main_worktree = true;

    let json = serde_json::to_string(&session).unwrap();
    let reloaded: Session = serde_json::from_str(&json).unwrap();
    assert!(
        reloaded.use_main_worktree,
        "use_main_worktree must survive serde roundtrip — this guards against \
         remove_dir_all on the project root during destroy"
    );
}

#[test]
fn test_use_main_worktree_defaults_false_on_old_json() {
    let json = r#"{
        "id": "test/honryu",
        "project_id": "test",
        "branch": "honryu",
        "worktree_path": "/tmp/project",
        "agent": "claude",
        "status": "Active",
        "created_at": "2024-01-01T00:00:00Z",
        "port_range_start": 3000,
        "port_range_end": 3009,
        "port_count": 10
    }"#;

    let session: Session = serde_json::from_str(json).unwrap();
    assert!(
        !session.use_main_worktree,
        "use_main_worktree must default to false for old sessions without the field"
    );
}
