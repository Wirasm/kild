use super::*;
use kild_core::sessions::types::SessionStatus;
use kild_core::{BranchName, Event, GitStatus, ProcessStatus, Session, SessionSnapshot};
use std::path::PathBuf;

use crate::state::dialog::DialogState;
use crate::state::errors::OperationError;

#[test]
fn test_close_dialog_clears_confirm_state() {
    let mut state = AppState::test_new();
    state.set_dialog(DialogState::Confirm {
        branch: "feature-branch".to_string(),
        safety_info: None,
        error: Some("Some error".to_string()),
    });

    state.close_dialog();

    assert!(matches!(state.dialog(), DialogState::None));
}

#[test]
fn test_set_dialog_error_sets_error_on_create() {
    let mut state = AppState::test_new();
    state.set_dialog(DialogState::open_create());

    state.set_dialog_error("Test error".to_string());

    if let DialogState::Create { error, .. } = state.dialog() {
        assert_eq!(error.as_deref(), Some("Test error"));
    } else {
        panic!("Expected Create dialog");
    }
}

#[test]
fn test_set_dialog_error_sets_error_on_confirm() {
    let mut state = AppState::test_new();
    state.open_confirm_dialog("test-branch".to_string());

    state.set_dialog_error("Destroy failed".to_string());

    if let DialogState::Confirm { error, .. } = state.dialog() {
        assert_eq!(error.as_deref(), Some("Destroy failed"));
    } else {
        panic!("Expected Confirm dialog");
    }
}

#[test]
fn test_clear_error() {
    let mut state = AppState::test_new();
    state.set_error(
        "branch",
        OperationError {
            message: "error".to_string(),
        },
    );

    state.clear_error("branch");

    assert!(state.get_error("branch").is_none());
}

#[test]
fn test_close_dialog_clears_add_project_state() {
    let mut state = AppState::test_new();
    state.set_dialog(DialogState::AddProject {
        error: Some("Error".to_string()),
    });

    state.close_dialog();

    assert!(matches!(state.dialog(), DialogState::None));
}

#[test]
fn test_active_project_id() {
    let mut state = AppState::test_new();

    // No active project
    assert!(state.active_project_id().is_none());

    // With active project - should return a hash, not directory name
    let project = kild_core::projects::types::test_helpers::make_test_project(
        PathBuf::from("/Users/test/Projects/my-project"),
        "My Project".to_string(),
    );
    state.projects.add(project).unwrap();
    // First project is automatically selected
    let project_id = state.active_project_id();
    assert!(project_id.is_some());
    // Should be a hex hash, not the directory name
    let id = project_id.unwrap();
    assert!(!id.is_empty());
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_filtered_displays_no_active_project() {
    let make_session = |id: &str, project_id: &str| {
        Session::new(
            id.into(),
            project_id.into(),
            BranchName::new(format!("branch-{}", id)),
            PathBuf::from("/tmp/test"),
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
        )
    };

    let mut state = AppState::test_new();
    state.sessions.set_displays(vec![
        SessionSnapshot {
            session: make_session("1", "project-a"),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            uncommitted_diff: None,
        },
        SessionSnapshot {
            session: make_session("2", "project-b"),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            uncommitted_diff: None,
        },
    ]);

    // No active project - should return all
    let filtered = state.filtered_displays();
    assert_eq!(filtered.len(), 2);
}

#[test]
fn test_filtered_displays_with_active_project() {
    // Use the actual hash for the project path
    let project_path = PathBuf::from("/Users/test/Projects/project-a");
    let project_id_a = kild_core::projects::generate_project_id(&project_path);
    let project_id_b = kild_core::projects::generate_project_id(&PathBuf::from("/other/project"));

    let make_session = |id: &str, project_id: &str| {
        Session::new(
            id.into(),
            project_id.into(),
            BranchName::new(format!("branch-{}", id)),
            PathBuf::from("/tmp/test"),
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
        )
    };

    let mut state = AppState::test_new();
    state.sessions.set_displays(vec![
        SessionSnapshot {
            session: make_session("1", &project_id_a),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            uncommitted_diff: None,
        },
        SessionSnapshot {
            session: make_session("2", &project_id_b),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            uncommitted_diff: None,
        },
        SessionSnapshot {
            session: make_session("3", &project_id_a),
            process_status: ProcessStatus::Running,
            git_status: GitStatus::Unknown,
            uncommitted_diff: None,
        },
    ]);

    // Active project set - should filter
    // Add project and select it
    let project = kild_core::projects::types::test_helpers::make_test_project(
        project_path.clone(),
        "Project A".to_string(),
    );
    state.projects.add(project).unwrap();
    // First project is auto-selected, so this should filter
    let filtered = state.filtered_displays();
    assert_eq!(filtered.len(), 2);
    assert!(
        filtered
            .iter()
            .all(|d| d.session.project_id == project_id_a)
    );
}

#[test]
fn test_filtered_displays_returns_empty_when_no_matching_project() {
    let make_session = |id: &str, project_id: &str| {
        Session::new(
            id.into(),
            project_id.into(),
            BranchName::new(format!("branch-{}", id)),
            PathBuf::from("/tmp/test"),
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
        )
    };

    let mut state = AppState::test_new();
    state.sessions.set_displays(vec![SessionSnapshot {
        session: make_session("1", "other-project-hash"),
        process_status: ProcessStatus::Stopped,
        git_status: GitStatus::Unknown,
        uncommitted_diff: None,
    }]);

    // Active project set to a different path - should return empty
    let project = kild_core::projects::types::test_helpers::make_test_project(
        PathBuf::from("/different/project/path"),
        "Different Project".to_string(),
    );
    state.projects.add(project).unwrap();
    let filtered = state.filtered_displays();
    assert!(
        filtered.is_empty(),
        "Should return empty when no kilds match active project"
    );
}

#[test]
fn test_selected_kild_returns_none_when_kild_removed_after_refresh() {
    let make_session = |id: &str| {
        Session::new(
            id.into(),
            "test-project".into(),
            BranchName::new(format!("branch-{}", id)),
            PathBuf::from("/tmp/test"),
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
        )
    };

    let mut state = AppState::test_new();
    state.sessions.set_displays(vec![SessionSnapshot {
        session: make_session("test-id"),
        process_status: ProcessStatus::Stopped,
        git_status: GitStatus::Unknown,
        uncommitted_diff: None,
    }]);
    state.selection.select("test-id".to_string());

    // Verify selection works initially
    assert!(state.selected_kild().is_some());

    // Simulate refresh that removes the kild (e.g., destroyed via CLI)
    state.sessions.set_displays(vec![]);

    // Selection ID still set, but selected_kild() should return None gracefully
    assert!(state.selection.has_selection());
    assert!(
        state.selected_kild().is_none(),
        "Should return None when selected kild no longer exists"
    );
}

#[test]
fn test_selected_kild_persists_after_refresh_when_kild_still_exists() {
    let make_session = |id: &str| {
        Session::new(
            id.into(),
            "test-project".into(),
            BranchName::new(format!("branch-{}", id)),
            PathBuf::from("/tmp/test"),
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
        )
    };

    let mut state = AppState::test_new();
    state.sessions.set_displays(vec![SessionSnapshot {
        session: make_session("test-id"),
        process_status: ProcessStatus::Stopped,
        git_status: GitStatus::Unknown,
        uncommitted_diff: None,
    }]);
    state.selection.select("test-id".to_string());

    // Verify initial selection
    assert!(state.selected_kild().is_some());

    // Simulate refresh that keeps the same kild (new display list with same ID)
    state.sessions.set_displays(vec![SessionSnapshot {
        session: make_session("test-id"),
        process_status: ProcessStatus::Running, // Status may change
        git_status: GitStatus::Dirty,           // Git status may change
        uncommitted_diff: None,
    }]);

    // Selection should persist
    let selected = state.selected_kild();
    assert!(selected.is_some());
    assert_eq!(&*selected.unwrap().session.id, "test-id");
}

#[test]
fn test_clear_selection_clears_selection() {
    let mut state = AppState::test_new();
    state.selection.select("test-id".to_string());

    assert!(state.selection.has_selection());

    state.clear_selection();

    assert!(
        !state.selection.has_selection(),
        "clear_selection should clear the selection"
    );
}

#[test]
fn test_destroy_should_clear_selection_when_selected_kild_destroyed() {
    let make_session = |id: &str, branch: &str| {
        Session::new(
            id.into(),
            "test-project".into(),
            branch.into(),
            PathBuf::from("/tmp/test"),
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
        )
    };

    let mut state = AppState::test_new();
    state.sessions.set_displays(vec![
        SessionSnapshot {
            session: make_session("id-1", "branch-1"),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            uncommitted_diff: None,
        },
        SessionSnapshot {
            session: make_session("id-2", "branch-2"),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            uncommitted_diff: None,
        },
    ]);
    state.selection.select("id-1".to_string());

    // Simulate destroy of selected kild - the destroy handler logic:
    // if selected_kild().session.branch == destroyed_branch { clear_selection() }
    let destroyed_branch = "branch-1";
    if state
        .selected_kild()
        .is_some_and(|s| &*s.session.branch == destroyed_branch)
    {
        state.clear_selection();
    }
    state
        .sessions
        .displays_mut()
        .retain(|d| &*d.session.branch != destroyed_branch);

    // Selection should be cleared
    assert!(
        !state.selection.has_selection(),
        "Selection should be cleared when selected kild is destroyed"
    );
}

#[test]
fn test_destroy_preserves_selection_when_different_kild_destroyed() {
    let make_session = |id: &str, branch: &str| {
        Session::new(
            id.into(),
            "test-project".into(),
            branch.into(),
            PathBuf::from("/tmp/test"),
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
        )
    };

    let mut state = AppState::test_new();
    state.sessions.set_displays(vec![
        SessionSnapshot {
            session: make_session("id-1", "branch-1"),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            uncommitted_diff: None,
        },
        SessionSnapshot {
            session: make_session("id-2", "branch-2"),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            uncommitted_diff: None,
        },
    ]);
    state.selection.select("id-1".to_string());

    // Destroy branch-2 (not selected)
    let destroyed_branch = "branch-2";
    if state
        .selected_kild()
        .is_some_and(|s| &*s.session.branch == destroyed_branch)
    {
        state.clear_selection();
    }
    state
        .sessions
        .displays_mut()
        .retain(|d| &*d.session.branch != destroyed_branch);

    // Selection of branch-1 should persist
    assert_eq!(
        state.selection.id(),
        Some("id-1"),
        "Selection should persist when a different kild is destroyed"
    );
    assert!(state.selected_kild().is_some());
}

// --- apply_events tests ---

fn make_session_for_event_test(id: &str, branch: &str) -> Session {
    Session::new(
        id.into(),
        "test-project".into(),
        branch.into(),
        PathBuf::from("/tmp/test"),
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
    )
}

#[test]
fn test_apply_events_handles_empty_vec() {
    let mut state = AppState::test_new();
    state.set_dialog(DialogState::open_create());

    state.apply_events(&[]);

    // Dialog should still be open — no events means no mutations
    assert!(state.dialog().is_create());
}

#[test]
fn test_apply_kild_created_closes_dialog_and_refreshes() {
    let mut state = AppState::test_new();
    state.set_dialog(DialogState::open_create());

    state.apply_events(&[Event::KildCreated {
        branch: "test-branch".into(),
        session_id: "test-id".into(),
    }]);

    assert!(matches!(state.dialog(), DialogState::None));
}

#[test]
fn test_apply_kild_destroyed_clears_selection_when_selected() {
    let mut state = AppState::test_new();
    state.sessions.set_displays(vec![SessionSnapshot {
        session: make_session_for_event_test("id-1", "branch-1"),
        process_status: ProcessStatus::Stopped,
        git_status: GitStatus::Unknown,
        uncommitted_diff: None,
    }]);
    state.selection.select("id-1".to_string());
    state.set_dialog(DialogState::open_confirm("branch-1".to_string(), None));

    state.apply_events(&[Event::KildDestroyed {
        branch: "branch-1".into(),
    }]);

    assert!(!state.has_selection());
    assert!(matches!(state.dialog(), DialogState::None));
}

#[test]
fn test_apply_kild_destroyed_preserves_selection_when_other() {
    let mut state = AppState::test_new();
    state.sessions.set_displays(vec![
        SessionSnapshot {
            session: make_session_for_event_test("id-1", "branch-1"),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            uncommitted_diff: None,
        },
        SessionSnapshot {
            session: make_session_for_event_test("id-2", "branch-2"),
            process_status: ProcessStatus::Stopped,
            git_status: GitStatus::Unknown,
            uncommitted_diff: None,
        },
    ]);
    state.selection.select("id-1".to_string());

    state.apply_events(&[Event::KildDestroyed {
        branch: "branch-2".into(),
    }]);

    // Selection of branch-1 should be preserved
    assert!(state.has_selection());
    assert_eq!(state.selected_id(), Some("id-1"));
}

#[test]
fn test_apply_kild_opened_preserves_selection_and_dialog() {
    let mut state = AppState::test_new();
    state.sessions.set_displays(vec![SessionSnapshot {
        session: make_session_for_event_test("id-1", "branch-1"),
        process_status: ProcessStatus::Stopped,
        git_status: GitStatus::Unknown,
        uncommitted_diff: None,
    }]);
    state.selection.select("id-1".to_string());
    state.set_dialog(DialogState::open_create());

    state.apply_events(&[Event::KildOpened {
        branch: "branch-1".into(),
        agent: "claude".to_string(),
    }]);

    assert!(state.dialog().is_create());
    assert!(state.has_selection());
    assert_eq!(state.selected_id(), Some("id-1"));
}

#[test]
fn test_apply_kild_stopped_preserves_selection_and_dialog() {
    let mut state = AppState::test_new();
    state.sessions.set_displays(vec![SessionSnapshot {
        session: make_session_for_event_test("id-1", "branch-1"),
        process_status: ProcessStatus::Running,
        git_status: GitStatus::Unknown,
        uncommitted_diff: None,
    }]);
    state.selection.select("id-1".to_string());
    state.set_dialog(DialogState::open_create());

    state.apply_events(&[Event::KildStopped {
        branch: "branch-1".into(),
    }]);

    assert!(state.dialog().is_create());
    assert!(state.has_selection());
    assert_eq!(state.selected_id(), Some("id-1"));
}

#[test]
fn test_apply_kild_completed_clears_selection_when_selected() {
    let mut state = AppState::test_new();
    state.sessions.set_displays(vec![SessionSnapshot {
        session: make_session_for_event_test("id-1", "branch-1"),
        process_status: ProcessStatus::Stopped,
        git_status: GitStatus::Unknown,
        uncommitted_diff: None,
    }]);
    state.selection.select("id-1".to_string());

    state.apply_events(&[Event::KildCompleted {
        branch: "branch-1".into(),
    }]);

    assert!(!state.has_selection());
}

// --- apply_events project tests ---

#[test]
fn test_apply_project_added_closes_dialog() {
    let mut state = AppState::test_new();
    state.set_dialog(DialogState::open_add_project());

    state.apply_events(&[Event::ProjectAdded {
        path: PathBuf::from("/tmp/project"),
        name: "Project".to_string(),
    }]);

    assert!(matches!(state.dialog(), DialogState::None));
}

#[test]
fn test_apply_project_removed_does_not_close_dialog() {
    let mut state = AppState::test_new();
    state.set_dialog(DialogState::open_create());

    state.apply_events(&[Event::ProjectRemoved {
        path: PathBuf::from("/tmp/project"),
    }]);

    // Remove project should not close dialogs
    assert!(state.dialog().is_create());
}

#[test]
fn test_apply_active_project_changed() {
    let mut state = AppState::test_new();

    // Should not panic on empty project list
    state.apply_events(&[Event::ActiveProjectChanged {
        path: Some(PathBuf::from("/tmp/project")),
    }]);
}

#[test]
fn test_apply_active_project_changed_to_none() {
    let mut state = AppState::test_new();

    // Should not panic
    state.apply_events(&[Event::ActiveProjectChanged { path: None }]);
}

// --- Project error boundary tests ---

#[test]
fn test_add_project_error_preserves_dialog() {
    let mut state = AppState::test_new();
    state.set_dialog(DialogState::open_add_project());

    // Simulate dispatch failure (invalid path)
    let error = "Path does not exist".to_string();
    state.set_dialog_error(error.clone());

    // Dialog should remain open with error
    assert!(state.dialog().is_add_project());
    if let DialogState::AddProject { error: e, .. } = state.dialog() {
        assert_eq!(e.as_deref(), Some("Path does not exist"));
    } else {
        panic!("Expected AddProject dialog");
    }
}

#[test]
fn test_add_project_success_closes_dialog() {
    let mut state = AppState::test_new();
    state.set_dialog(DialogState::open_add_project());

    // Simulate successful dispatch
    state.apply_events(&[Event::ProjectAdded {
        path: PathBuf::from("/tmp/project"),
        name: "Project".to_string(),
    }]);

    // Dialog should be closed
    assert!(matches!(state.dialog(), DialogState::None));
}

#[test]
fn test_add_project_error_then_success_clears_error_and_closes() {
    let mut state = AppState::test_new();
    state.set_dialog(DialogState::open_add_project());

    // First attempt fails
    state.set_dialog_error("Not a git repo".to_string());
    assert!(state.dialog().is_add_project());

    // Second attempt succeeds
    state.apply_events(&[Event::ProjectAdded {
        path: PathBuf::from("/tmp/project"),
        name: "Project".to_string(),
    }]);
    assert!(matches!(state.dialog(), DialogState::None));
}

#[test]
fn test_select_project_error_surfaces_in_banner() {
    let mut state = AppState::test_new();
    assert!(!state.has_banner_errors());

    // Simulate select project failure
    state.push_error("Failed to select project: not found".to_string());

    assert!(state.has_banner_errors());
    assert_eq!(state.banner_errors().len(), 1);
    assert_eq!(
        state.banner_errors()[0],
        "Failed to select project: not found"
    );
}

#[test]
fn test_remove_project_error_surfaces_in_banner() {
    let mut state = AppState::test_new();

    state.push_error("Failed to remove project: permission denied".to_string());

    assert!(state.has_banner_errors());
    assert_eq!(
        state.banner_errors()[0],
        "Failed to remove project: permission denied"
    );
}

// --- Loading facade tests ---

#[test]
fn test_loading_facade_branch() {
    let mut state = AppState::test_new();

    assert!(!state.is_loading("branch-1"));
    state.set_loading("branch-1");
    assert!(state.is_loading("branch-1"));
    assert!(!state.is_loading("branch-2"));
    state.clear_loading("branch-1");
    assert!(!state.is_loading("branch-1"));
}

#[test]
fn test_loading_facade_dialog() {
    let mut state = AppState::test_new();

    assert!(!state.is_dialog_loading());
    state.set_dialog_loading();
    assert!(state.is_dialog_loading());
    state.clear_dialog_loading();
    assert!(!state.is_dialog_loading());
}

#[test]
fn test_multiple_branches_load_independently() {
    let mut state = AppState::test_new();

    state.set_loading("branch-1");
    state.set_loading("branch-2");

    assert!(state.is_loading("branch-1"));
    assert!(state.is_loading("branch-2"));

    state.clear_loading("branch-1");
    assert!(!state.is_loading("branch-1"));
    assert!(state.is_loading("branch-2"));
}

#[test]
fn test_loading_dimensions_independent() {
    let mut state = AppState::test_new();

    state.set_loading("branch-1");
    assert!(!state.is_dialog_loading());

    state.set_dialog_loading();
    assert!(state.is_loading("branch-1"));
    assert!(state.is_dialog_loading());
}

#[test]
fn test_set_loading_does_not_clear_existing_error() {
    let mut state = AppState::test_new();
    state.set_error(
        "branch-1",
        OperationError {
            message: "Previous error".to_string(),
        },
    );

    state.set_loading("branch-1");

    assert!(state.get_error("branch-1").is_some());
    assert_eq!(
        state.get_error("branch-1").unwrap().message,
        "Previous error"
    );
}

#[test]
fn test_error_persists_through_loading_lifecycle() {
    let mut state = AppState::test_new();
    state.set_error(
        "branch-1",
        OperationError {
            message: "Error message".to_string(),
        },
    );

    // Error persists through loading set/clear cycle
    state.set_loading("branch-1");
    assert!(state.get_error("branch-1").is_some());

    state.clear_loading("branch-1");
    assert!(state.get_error("branch-1").is_some());
}
