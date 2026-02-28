use super::session_files::*;
use super::*;
use crate::sessions::types::*;

#[test]
fn test_ensure_sessions_directory() {
    use std::env;

    let temp_dir = env::temp_dir().join("kild_test_sessions");

    // Clean up if exists
    let _ = std::fs::remove_dir_all(&temp_dir);

    // Should create directory
    assert!(ensure_sessions_directory(&temp_dir).is_ok());
    assert!(temp_dir.exists());

    // Should not error if directory already exists
    assert!(ensure_sessions_directory(&temp_dir).is_ok());

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_save_session_to_file() {
    use std::env;

    let temp_dir = env::temp_dir().join("kild_test_save_session");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let worktree_path = temp_dir.join("worktree");
    std::fs::create_dir_all(&worktree_path).unwrap();

    let session = Session::new(
        "test/branch".into(),
        "test".into(),
        "branch".into(),
        worktree_path,
        "claude".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        0,
        0,
        0,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );

    assert!(save_session_to_file(&session, &temp_dir).is_ok());

    let sess_dir = temp_dir.join("test_branch");
    let sess_file = sess_dir.join("kild.json");
    assert!(sess_dir.is_dir());
    assert!(sess_file.exists());

    let content = std::fs::read_to_string(&sess_file).unwrap();
    let loaded_session: Session = serde_json::from_str(&content).unwrap();
    assert_eq!(loaded_session, session);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_save_session_atomic_write_temp_cleanup() {
    use std::env;

    let temp_dir = env::temp_dir().join("kild_test_atomic_write");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let worktree_path = temp_dir.join("worktree");
    std::fs::create_dir_all(&worktree_path).unwrap();

    let session = Session::new(
        "test/atomic".into(),
        "test".into(),
        "atomic".into(),
        worktree_path,
        "claude".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        0,
        0,
        0,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );

    assert!(save_session_to_file(&session, &temp_dir).is_ok());

    let sess_dir = temp_dir.join("test_atomic");
    let temp_file = sess_dir.join("kild.json.tmp");
    assert!(
        !temp_file.exists(),
        "Temp file should be cleaned up after successful write"
    );

    let sess_file = sess_dir.join("kild.json");
    assert!(sess_file.exists(), "Final session file should exist");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_save_session_atomic_behavior() {
    use std::env;

    let temp_dir = env::temp_dir().join("kild_test_atomic_behavior");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let worktree_path = temp_dir.join("worktree");
    std::fs::create_dir_all(&worktree_path).unwrap();

    let session = Session::new(
        "test/atomic-behavior".into(),
        "test".into(),
        "atomic-behavior".into(),
        worktree_path,
        "claude".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        0,
        0,
        0,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );

    let sess_dir = temp_dir.join("test_atomic-behavior");
    std::fs::create_dir_all(&sess_dir).unwrap();
    let sess_file = sess_dir.join("kild.json");
    std::fs::write(&sess_file, "old content").unwrap();

    assert!(save_session_to_file(&session, &temp_dir).is_ok());

    let content = std::fs::read_to_string(&sess_file).unwrap();
    assert!(content.contains("test/atomic-behavior"));
    assert!(!content.contains("old content"));

    let loaded_session: Session = serde_json::from_str(&content).unwrap();
    assert_eq!(loaded_session, session);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_save_session_temp_file_cleanup_on_failure() {
    use std::env;

    let temp_dir = env::temp_dir().join("kild_test_temp_cleanup");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let worktree_path = temp_dir.join("worktree");
    std::fs::create_dir_all(&worktree_path).unwrap();

    let session = Session::new(
        "test/cleanup".into(),
        "test".into(),
        "cleanup".into(),
        worktree_path,
        "claude".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        0,
        0,
        0,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );

    let sess_dir = temp_dir.join("test_cleanup");
    std::fs::create_dir_all(&sess_dir).unwrap();
    let sess_file = sess_dir.join("kild.json");
    std::fs::create_dir_all(&sess_file).unwrap(); // Create as directory to force rename failure

    let result = save_session_to_file(&session, &temp_dir);
    assert!(result.is_err(), "Save should fail when rename fails");

    let temp_file = sess_dir.join("kild.json.tmp");
    assert!(
        !temp_file.exists(),
        "Temp file should be cleaned up after rename failure"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_load_sessions_from_files() {
    use std::env;

    let temp_dir = env::temp_dir().join("kild_test_load_sessions");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let (sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();
    assert_eq!(sessions.len(), 0);
    assert_eq!(skipped, 0);

    let worktree1 = temp_dir.join("worktree1");
    let worktree2 = temp_dir.join("worktree2");
    std::fs::create_dir_all(&worktree1).unwrap();
    std::fs::create_dir_all(&worktree2).unwrap();

    let session1 = Session::new(
        "test/branch1".into(),
        "test".into(),
        "branch1".into(),
        worktree1,
        "claude".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        0,
        0,
        0,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );
    let session2 = Session::new(
        "test/branch2".into(),
        "test".into(),
        "branch2".into(),
        worktree2,
        "kiro".to_string(),
        SessionStatus::Stopped,
        "2024-01-02T00:00:00Z".to_string(),
        0,
        0,
        0,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );

    save_session_to_file(&session1, &temp_dir).unwrap();
    save_session_to_file(&session2, &temp_dir).unwrap();

    let (loaded_sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();
    assert_eq!(loaded_sessions.len(), 2);
    assert_eq!(skipped, 0);

    let ids: Vec<String> = loaded_sessions.iter().map(|s| s.id.to_string()).collect();
    assert!(ids.contains(&"test/branch1".to_string()));
    assert!(ids.contains(&"test/branch2".to_string()));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_load_sessions_nonexistent_directory() {
    use std::env;
    let nonexistent_dir = env::temp_dir().join("kild_test_nonexistent");
    let _ = std::fs::remove_dir_all(&nonexistent_dir);
    let (sessions, skipped) = load_sessions_from_files(&nonexistent_dir).unwrap();
    assert_eq!(sessions.len(), 0);
    assert_eq!(skipped, 0);
}

#[test]
fn test_find_session_by_name() {
    use std::env;

    let temp_dir = env::temp_dir().join("kild_test_find_session");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let worktree_path = temp_dir.join("worktree");
    std::fs::create_dir_all(&worktree_path).unwrap();

    let session = Session::new(
        "test/feature-branch".into(),
        "test".into(),
        "feature-branch".into(),
        worktree_path,
        "claude".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        0,
        0,
        0,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );

    save_session_to_file(&session, &temp_dir).unwrap();

    let found = find_session_by_name(&temp_dir, "feature-branch").unwrap();
    assert!(found.is_some());
    assert_eq!(&*found.unwrap().id, "test/feature-branch");

    let not_found = find_session_by_name(&temp_dir, "non-existent").unwrap();
    assert!(not_found.is_none());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_remove_session_file() {
    use std::env;

    let temp_dir = env::temp_dir().join("kild_test_remove_session");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let worktree_path = temp_dir.join("worktree");
    std::fs::create_dir_all(&worktree_path).unwrap();

    let session = Session::new(
        "test/branch".into(),
        "test".into(),
        "branch".into(),
        worktree_path,
        "claude".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        0,
        0,
        0,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );

    save_session_to_file(&session, &temp_dir).unwrap();

    let sess_dir = temp_dir.join("test_branch");
    assert!(sess_dir.is_dir());

    remove_session_file(&temp_dir, &session.id).unwrap();
    assert!(!sess_dir.exists());

    assert!(remove_session_file(&temp_dir, "non-existent").is_ok());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_load_sessions_with_invalid_files() {
    use std::env;

    let temp_dir = env::temp_dir().join("kild_test_invalid_files");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let worktree_path = temp_dir.join("valid_worktree");
    std::fs::create_dir_all(&worktree_path).unwrap();

    let valid_session = Session::new(
        "test/valid".into(),
        "test".into(),
        "valid".into(),
        worktree_path,
        "claude".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        0,
        0,
        0,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );
    save_session_to_file(&valid_session, &temp_dir).unwrap();

    let invalid_dir = temp_dir.join("invalid");
    std::fs::create_dir_all(&invalid_dir).unwrap();
    std::fs::write(invalid_dir.join("kild.json"), "{ invalid json }").unwrap();

    let invalid_structure_dir = temp_dir.join("invalid_structure");
    std::fs::create_dir_all(&invalid_structure_dir).unwrap();
    std::fs::write(
        invalid_structure_dir.join("kild.json"),
        r#"{"id": "", "project_id": "test"}"#,
    )
    .unwrap();

    let (sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(&*sessions[0].id, "test/valid");
    assert_eq!(skipped, 2);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_load_sessions_includes_missing_worktree() {
    use std::env;

    let temp_dir = env::temp_dir().join("kild_test_missing_worktree");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let nonexistent_worktree = temp_dir.join("worktree_that_does_not_exist");

    let session_missing_worktree = Session::new(
        "test/orphaned".into(),
        "test".into(),
        "orphaned".into(),
        nonexistent_worktree.clone(),
        "claude".to_string(),
        SessionStatus::Stopped,
        "2024-01-01T00:00:00Z".to_string(),
        0,
        0,
        0,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );

    let sess_dir = temp_dir.join("test_orphaned");
    std::fs::create_dir_all(&sess_dir).unwrap();
    let sess_file = sess_dir.join("kild.json");
    std::fs::write(
        &sess_file,
        serde_json::to_string_pretty(&session_missing_worktree).unwrap(),
    )
    .unwrap();

    assert!(sess_file.exists());
    assert!(!nonexistent_worktree.exists());

    let (sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(skipped, 0);
    assert_eq!(&*sessions[0].id, "test/orphaned");
    assert!(!sessions[0].is_worktree_valid());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_load_sessions_mixed_valid_and_missing_worktrees() {
    use std::env;

    let temp_dir = env::temp_dir().join("kild_test_mixed_worktrees");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let valid_worktree = temp_dir.join("valid_worktree");
    std::fs::create_dir_all(&valid_worktree).unwrap();
    let missing_worktree = temp_dir.join("missing_worktree");

    let session_valid = Session::new(
        "test/valid-session".into(),
        "test".into(),
        "valid-session".into(),
        valid_worktree.clone(),
        "claude".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        3000,
        3009,
        10,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );
    let session_missing = Session::new(
        "test/missing-session".into(),
        "test".into(),
        "missing-session".into(),
        missing_worktree.clone(),
        "claude".to_string(),
        SessionStatus::Stopped,
        "2024-01-01T00:00:00Z".to_string(),
        3010,
        3019,
        10,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );

    let valid_dir = temp_dir.join("test_valid-session");
    std::fs::create_dir_all(&valid_dir).unwrap();
    std::fs::write(
        valid_dir.join("kild.json"),
        serde_json::to_string_pretty(&session_valid).unwrap(),
    )
    .unwrap();
    let missing_dir = temp_dir.join("test_missing-session");
    std::fs::create_dir_all(&missing_dir).unwrap();
    std::fs::write(
        missing_dir.join("kild.json"),
        serde_json::to_string_pretty(&session_missing).unwrap(),
    )
    .unwrap();

    let (sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();
    assert_eq!(sessions.len(), 2);
    assert_eq!(skipped, 0);

    let valid = sessions
        .iter()
        .find(|s| &*s.branch == "valid-session")
        .unwrap();
    let missing = sessions
        .iter()
        .find(|s| &*s.branch == "missing-session")
        .unwrap();
    assert!(valid.is_worktree_valid());
    assert!(!missing.is_worktree_valid());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_patch_session_json_field_preserves_unknown_fields() {
    use std::env;

    let temp_dir = env::temp_dir().join("kild_test_patch_preserves_fields");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let json = serde_json::json!({
        "id": "proj/my-branch", "project_id": "proj", "branch": "my-branch",
        "worktree_path": "/tmp/test", "agent": "claude", "status": "Active",
        "created_at": "2024-01-01T00:00:00Z", "port_range_start": 3000,
        "port_range_end": 3009, "port_count": 10,
        "last_activity": "2024-01-01T00:00:00Z", "agents": [],
        "future_field": "must_survive"
    });
    let sess_dir = temp_dir.join("proj_my-branch");
    std::fs::create_dir_all(&sess_dir).unwrap();
    let sess_file = sess_dir.join("kild.json");
    std::fs::write(&sess_file, serde_json::to_string_pretty(&json).unwrap()).unwrap();

    patch_session_json_field(
        &temp_dir,
        "proj/my-branch",
        "last_activity",
        serde_json::Value::String("2024-06-15T12:00:00Z".to_string()),
    )
    .unwrap();

    let content = std::fs::read_to_string(&sess_file).unwrap();
    let patched: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(patched["last_activity"], "2024-06-15T12:00:00Z");
    assert_eq!(patched["future_field"], "must_survive");
    assert_eq!(patched["branch"], "my-branch");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_patch_session_json_field_fails_on_non_object() {
    use std::env;

    let temp_dir = env::temp_dir().join("kild_test_patch_non_object");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let sess_dir = temp_dir.join("proj_branch");
    std::fs::create_dir_all(&sess_dir).unwrap();
    std::fs::write(sess_dir.join("kild.json"), "[]").unwrap();

    let result = patch_session_json_field(
        &temp_dir,
        "proj/branch",
        "last_activity",
        serde_json::Value::String("2024-06-15T12:00:00Z".to_string()),
    );
    assert!(result.is_err());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_patch_session_json_fields_preserves_unknown_fields() {
    use std::env;

    let temp_dir = env::temp_dir().join("kild_test_patch_multi_preserves_fields");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let json = serde_json::json!({
        "id": "proj/my-branch", "project_id": "proj", "branch": "my-branch",
        "worktree_path": "/tmp/test", "agent": "claude", "status": "Active",
        "created_at": "2024-01-01T00:00:00Z", "port_range_start": 3000,
        "port_range_end": 3009, "port_count": 10,
        "last_activity": "2024-01-01T00:00:00Z", "agents": [],
        "future_field": "must_survive"
    });
    let sess_dir = temp_dir.join("proj_my-branch");
    std::fs::create_dir_all(&sess_dir).unwrap();
    let sess_file = sess_dir.join("kild.json");
    std::fs::write(&sess_file, serde_json::to_string_pretty(&json).unwrap()).unwrap();

    patch_session_json_fields(
        &temp_dir,
        "proj/my-branch",
        &[
            ("status", serde_json::json!("Stopped")),
            (
                "last_activity",
                serde_json::Value::String("2024-06-15T12:00:00Z".to_string()),
            ),
        ],
    )
    .unwrap();

    let content = std::fs::read_to_string(&sess_file).unwrap();
    let patched: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(patched["status"], "Stopped");
    assert_eq!(patched["last_activity"], "2024-06-15T12:00:00Z");
    assert_eq!(patched["future_field"], "must_survive");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_write_and_read_agent_status() {
    let tmp = tempfile::TempDir::new().unwrap();
    let info = AgentStatusInfo {
        status: AgentStatus::Working,
        updated_at: "2026-02-05T12:00:00Z".to_string(),
    };
    write_agent_status(tmp.path(), "test/branch", &info).unwrap();
    assert!(tmp.path().join("test_branch").join("status").exists());
    assert_eq!(read_agent_status(tmp.path(), "test/branch"), Some(info));
}

#[test]
fn test_read_agent_status_missing_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    assert_eq!(read_agent_status(tmp.path(), "nonexistent"), None);
}

#[test]
fn test_read_agent_status_corrupt_json() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sess_dir = tmp.path().join("bad_session");
    std::fs::create_dir_all(&sess_dir).unwrap();
    std::fs::write(sess_dir.join("status"), "not json").unwrap();
    assert_eq!(read_agent_status(tmp.path(), "bad_session"), None);
}

#[test]
fn test_remove_agent_status_file_exists() {
    let tmp = tempfile::TempDir::new().unwrap();
    let info = AgentStatusInfo {
        status: AgentStatus::Idle,
        updated_at: "2026-02-05T12:00:00Z".to_string(),
    };
    write_agent_status(tmp.path(), "test/rm", &info).unwrap();
    let sidecar = tmp.path().join("test_rm").join("status");
    assert!(sidecar.exists());
    remove_agent_status_file(tmp.path(), "test/rm");
    assert!(!sidecar.exists());
}

#[test]
fn test_remove_agent_status_file_missing_is_noop() {
    let tmp = tempfile::TempDir::new().unwrap();
    remove_agent_status_file(tmp.path(), "nonexistent");
}

#[test]
fn test_write_and_read_pr_info() {
    use crate::forge::types::{CiStatus, PrState, PullRequest, ReviewStatus};
    let tmp = tempfile::TempDir::new().unwrap();
    let info = PullRequest {
        number: 42,
        url: "https://github.com/org/repo/pull/42".to_string(),
        state: PrState::Open,
        ci_status: CiStatus::Passing,
        ci_summary: Some("3/3 passing".to_string()),
        review_status: ReviewStatus::Approved,
        review_summary: Some("1 approved".to_string()),
        updated_at: "2026-02-05T12:00:00Z".to_string(),
    };
    write_pr_info(tmp.path(), "test/branch", &info).unwrap();
    assert!(tmp.path().join("test_branch").join("pr").exists());
    assert_eq!(read_pr_info(tmp.path(), "test/branch"), Some(info));
}

#[test]
fn test_read_pr_info_missing_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    assert_eq!(read_pr_info(tmp.path(), "nonexistent"), None);
}

#[test]
fn test_read_pr_info_corrupt_json() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sess_dir = tmp.path().join("bad_session");
    std::fs::create_dir_all(&sess_dir).unwrap();
    std::fs::write(sess_dir.join("pr"), "not json").unwrap();
    assert_eq!(read_pr_info(tmp.path(), "bad_session"), None);
}

#[test]
fn test_remove_pr_info_file_exists() {
    use crate::forge::types::{CiStatus, PrState, PullRequest, ReviewStatus};
    let tmp = tempfile::TempDir::new().unwrap();
    let info = PullRequest {
        number: 1,
        url: "https://github.com/org/repo/pull/1".to_string(),
        state: PrState::Open,
        ci_status: CiStatus::Unknown,
        ci_summary: None,
        review_status: ReviewStatus::Unknown,
        review_summary: None,
        updated_at: "2026-02-05T12:00:00Z".to_string(),
    };
    write_pr_info(tmp.path(), "test/rm", &info).unwrap();
    let sidecar = tmp.path().join("test_rm").join("pr");
    assert!(sidecar.exists());
    remove_pr_info_file(tmp.path(), "test/rm");
    assert!(!sidecar.exists());
}

#[test]
fn test_remove_pr_info_file_missing_is_noop() {
    let tmp = tempfile::TempDir::new().unwrap();
    remove_pr_info_file(tmp.path(), "nonexistent");
}

#[test]
fn test_save_load_roundtrip_all_fields() {
    use crate::terminal::types::TerminalType;
    use kild_protocol::RuntimeMode;

    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_dir = tmp.path();
    let worktree_dir = sessions_dir.join("worktree");
    std::fs::create_dir_all(&worktree_dir).unwrap();

    let agent = AgentProcess::new(
        "claude".to_string(),
        "proj_feat_0".to_string(),
        Some(12345),
        Some("claude-code".to_string()),
        Some(1705318200),
        Some(TerminalType::Ghostty),
        Some("kild-feat".to_string()),
        "claude --session-id abc".to_string(),
        "2024-01-15T10:30:00Z".to_string(),
        None,
    )
    .unwrap();

    let session = Session::new(
        "proj/feat".into(),
        "proj".into(),
        "feat".into(),
        worktree_dir,
        "claude".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        3000,
        3009,
        10,
        Some("2024-01-15T10:30:00Z".to_string()),
        Some("Implementing auth".to_string()),
        Some(456),
        vec![agent],
        Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
        Some("tl_proj_feat".to_string()),
        Some(RuntimeMode::Daemon),
    );

    save_session_to_file(&session, sessions_dir).unwrap();
    let loaded = find_session_by_name(sessions_dir, "feat")
        .unwrap()
        .expect("session should be found");

    assert_eq!(loaded.id, session.id);
    assert_eq!(loaded.project_id, session.project_id);
    assert_eq!(loaded.branch, session.branch);
    assert_eq!(loaded.worktree_path, session.worktree_path);
    assert_eq!(loaded.agent, session.agent);
    assert_eq!(loaded.status, session.status);
    assert_eq!(loaded.runtime_mode, session.runtime_mode);
    assert_eq!(loaded.agent_count(), 1);
    let agent = loaded.latest_agent().unwrap();
    assert_eq!(agent.agent(), "claude");
    assert_eq!(agent.spawn_id(), "proj_feat_0");
    assert_eq!(loaded.issue, session.issue);
}

#[test]
fn test_session_id_filename_mapping() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_dir = tmp.path();
    let worktree_dir = sessions_dir.join("wt");
    std::fs::create_dir_all(&worktree_dir).unwrap();

    let session = Session::new(
        "my-project/deep/nested".into(),
        "my-project".into(),
        "deep-nested".into(),
        worktree_dir,
        "claude".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        0,
        0,
        0,
        None,
        None,
        None,
        vec![],
        None,
        None,
        None,
    );

    save_session_to_file(&session, sessions_dir).unwrap();

    let expected_dir = sessions_dir.join("my-project_deep_nested");
    let expected_file = expected_dir.join("kild.json");
    assert!(expected_dir.is_dir());
    assert!(expected_file.exists());

    let loaded = load_session_from_file("deep-nested", sessions_dir).unwrap();
    assert_eq!(&*loaded.id, "my-project/deep/nested");
}

// --- Migration tests ---

#[test]
fn test_migrate_session_with_all_sidecars() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_dir = tmp.path();

    std::fs::write(
        sessions_dir.join("proj_branch.json"),
        r#"{"id":"proj/branch"}"#,
    )
    .unwrap();
    std::fs::write(
        sessions_dir.join("proj_branch.status"),
        r#"{"status":"idle"}"#,
    )
    .unwrap();
    std::fs::write(sessions_dir.join("proj_branch.pr"), r#"{"number":42}"#).unwrap();

    let result = migrate_session_if_needed(sessions_dir, "proj_branch").unwrap();
    assert!(result);

    assert!(!sessions_dir.join("proj_branch.json").exists());
    assert!(!sessions_dir.join("proj_branch.status").exists());
    assert!(!sessions_dir.join("proj_branch.pr").exists());

    let sess_dir = sessions_dir.join("proj_branch");
    assert!(sess_dir.join("kild.json").exists());
    assert!(sess_dir.join("status").exists());
    assert!(sess_dir.join("pr").exists());
}

#[test]
fn test_migrate_session_json_only() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join("proj_branch.json"),
        r#"{"id":"proj/branch"}"#,
    )
    .unwrap();

    let result = migrate_session_if_needed(tmp.path(), "proj_branch").unwrap();
    assert!(result);
    assert!(tmp.path().join("proj_branch").join("kild.json").exists());
    assert!(!tmp.path().join("proj_branch").join("status").exists());
}

#[test]
fn test_migrate_idempotent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sess_dir = tmp.path().join("proj_branch");
    std::fs::create_dir_all(&sess_dir).unwrap();
    std::fs::write(sess_dir.join("kild.json"), r#"{"id":"proj/branch"}"#).unwrap();

    let result = migrate_session_if_needed(tmp.path(), "proj_branch").unwrap();
    assert!(!result);
}

#[test]
fn test_migrate_nonexistent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let result = migrate_session_if_needed(tmp.path(), "nonexistent").unwrap();
    assert!(!result);
}

#[test]
fn test_load_sessions_auto_migrates_old_format() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_dir = tmp.path();
    let worktree_path = sessions_dir.join("worktree");
    std::fs::create_dir_all(&worktree_path).unwrap();

    let session = Session::new(
        "test/migrate-me".into(),
        "test".into(),
        "migrate-me".into(),
        worktree_path,
        "claude".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        0,
        0,
        0,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );

    let old_file = sessions_dir.join("test_migrate-me.json");
    std::fs::write(&old_file, serde_json::to_string_pretty(&session).unwrap()).unwrap();

    let (sessions, skipped) = load_sessions_from_files(sessions_dir).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(skipped, 0);
    assert_eq!(&*sessions[0].id, "test/migrate-me");
    assert!(!old_file.exists());
    assert!(
        sessions_dir
            .join("test_migrate-me")
            .join("kild.json")
            .exists()
    );
}

#[test]
fn test_load_sessions_mixed_old_and_new() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_dir = tmp.path();

    let wt1 = sessions_dir.join("wt1");
    let wt2 = sessions_dir.join("wt2");
    std::fs::create_dir_all(&wt1).unwrap();
    std::fs::create_dir_all(&wt2).unwrap();

    let session1 = Session::new(
        "test/new-format".into(),
        "test".into(),
        "new-format".into(),
        wt1,
        "claude".to_string(),
        SessionStatus::Active,
        "2024-01-01T00:00:00Z".to_string(),
        0,
        0,
        0,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );
    let session2 = Session::new(
        "test/old-format".into(),
        "test".into(),
        "old-format".into(),
        wt2,
        "claude".to_string(),
        SessionStatus::Stopped,
        "2024-01-01T00:00:00Z".to_string(),
        0,
        0,
        0,
        Some("2024-01-01T00:00:00Z".to_string()),
        None,
        None,
        vec![],
        None,
        None,
        None,
    );

    save_session_to_file(&session1, sessions_dir).unwrap();
    std::fs::write(
        sessions_dir.join("test_old-format.json"),
        serde_json::to_string_pretty(&session2).unwrap(),
    )
    .unwrap();

    let (sessions, skipped) = load_sessions_from_files(sessions_dir).unwrap();
    assert_eq!(sessions.len(), 2);
    assert_eq!(skipped, 0);
}

#[test]
fn test_migrate_cleans_up_temp_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join("proj_branch.json"),
        r#"{"id":"proj/branch"}"#,
    )
    .unwrap();
    std::fs::write(tmp.path().join("proj_branch.json.tmp"), "temp data").unwrap();
    std::fs::write(tmp.path().join("proj_branch.status.tmp"), "temp data").unwrap();

    migrate_session_if_needed(tmp.path(), "proj_branch").unwrap();

    assert!(!tmp.path().join("proj_branch.json.tmp").exists());
    assert!(!tmp.path().join("proj_branch.status.tmp").exists());
}

#[test]
fn test_concurrent_migration() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_dir = tmp.path().to_path_buf();
    std::fs::write(
        sessions_dir.join("proj_branch.json"),
        r#"{"id":"proj/branch"}"#,
    )
    .unwrap();

    let dir1 = sessions_dir.clone();
    let dir2 = sessions_dir.clone();

    let t1 = std::thread::spawn(move || migrate_session_if_needed(&dir1, "proj_branch"));
    let t2 = std::thread::spawn(move || migrate_session_if_needed(&dir2, "proj_branch"));

    let r1 = t1.join().unwrap();
    let r2 = t2.join().unwrap();
    assert!(r1.is_ok());
    assert!(r2.is_ok());

    assert!(sessions_dir.join("proj_branch").join("kild.json").exists());
    assert!(!sessions_dir.join("proj_branch.json").exists());
}

#[test]
fn test_session_dir_and_session_file_helpers() {
    let base = std::path::Path::new("/tmp/sessions");
    assert_eq!(
        session_dir(base, "proj/branch"),
        std::path::PathBuf::from("/tmp/sessions/proj_branch")
    );
    assert_eq!(
        session_file(base, "proj/branch"),
        std::path::PathBuf::from("/tmp/sessions/proj_branch/kild.json")
    );
    assert_eq!(
        session_dir(base, "deep/nested/id"),
        std::path::PathBuf::from("/tmp/sessions/deep_nested_id")
    );
}

#[test]
fn test_remove_session_file_warns_unexpected_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("proj_branch");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kild.json"), r#"{"id":"proj/branch"}"#).unwrap();
    std::fs::write(dir.join("status"), r#"{"status":"idle"}"#).unwrap();
    std::fs::write(dir.join("unexpected.log"), "should warn").unwrap();

    remove_session_file(tmp.path(), "proj/branch").unwrap();
    assert!(!dir.exists());
}

// --- Branch index tests ---

#[test]
fn test_branch_index_save_and_lookup() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_dir = tmp.path();
    let worktree = tmp.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();

    let session = Session::new_for_test("feature-auth".to_string(), worktree);
    save_session_to_file(&session, sessions_dir).unwrap();

    // Index should be populated — fast path returns the session
    let found = find_session_by_name(sessions_dir, "feature-auth").unwrap();
    assert!(found.is_some());
    assert_eq!(&*found.unwrap().branch, "feature-auth");

    // branch_index.json must exist
    assert!(sessions_dir.join("branch_index.json").exists());
}

#[test]
fn test_branch_index_remove_on_session_remove() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_dir = tmp.path();
    let worktree = tmp.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();

    let session = Session::new_for_test("feature-auth".to_string(), worktree);
    save_session_to_file(&session, sessions_dir).unwrap();

    // Confirm it's findable before removal
    assert!(
        find_session_by_name(sessions_dir, "feature-auth")
            .unwrap()
            .is_some()
    );

    remove_session_file(sessions_dir, &session.id).unwrap();

    // Should be gone after removal
    let found = find_session_by_name(sessions_dir, "feature-auth").unwrap();
    assert!(found.is_none());
}

#[test]
fn test_find_session_by_name_fallback_without_index() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_dir = tmp.path();
    let worktree = tmp.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();

    let session = Session::new_for_test("feature-auth".to_string(), worktree);

    // Write session file directly without going through save_session_to_file
    // so no index entry is created.
    let safe_id = session.id.replace('/', "_");
    let dir = sessions_dir.join(&safe_id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("kild.json"),
        serde_json::to_string(&session).unwrap(),
    )
    .unwrap();

    // Should still find it via full scan fallback
    let found = find_session_by_name(sessions_dir, "feature-auth").unwrap();
    assert!(found.is_some());
    assert_eq!(&*found.unwrap().branch, "feature-auth");

    // Index should be repaired after the fallback scan
    assert!(sessions_dir.join("branch_index.json").exists());
}

#[test]
fn test_save_session_compact_json() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_dir = tmp.path();
    let worktree = tmp.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();

    let session = Session::new_for_test("feat".to_string(), worktree);
    save_session_to_file(&session, sessions_dir).unwrap();

    let safe_id = session.id.replace('/', "_");
    let content = std::fs::read_to_string(sessions_dir.join(safe_id).join("kild.json")).unwrap();

    // Compact JSON has no newlines (single line)
    assert_eq!(
        content.lines().count(),
        1,
        "kild.json should be compact (single line)"
    );
}
