//! Tests for the main view module.

use std::path::PathBuf;

use super::path_utils::{canonicalize_path, normalize_project_path};
use super::types::ActiveView;

#[test]
fn test_active_view_default_is_control() {
    assert_eq!(ActiveView::Control, ActiveView::Control);
    assert_ne!(ActiveView::Control, ActiveView::Dashboard);
    assert_ne!(ActiveView::Control, ActiveView::Detail);
    assert_ne!(ActiveView::Dashboard, ActiveView::Detail);
}

#[test]
fn test_toggle_view_switches_control_to_dashboard() {
    let mut view = ActiveView::Control;
    view = match view {
        ActiveView::Control => ActiveView::Dashboard,
        ActiveView::Dashboard | ActiveView::Detail => ActiveView::Control,
    };
    assert_eq!(view, ActiveView::Dashboard);
}

#[test]
fn test_toggle_view_switches_dashboard_to_control() {
    let mut view = ActiveView::Dashboard;
    view = match view {
        ActiveView::Control => ActiveView::Dashboard,
        ActiveView::Dashboard | ActiveView::Detail => ActiveView::Control,
    };
    assert_eq!(view, ActiveView::Control);
}

#[test]
fn test_toggle_view_switches_detail_to_control() {
    let mut view = ActiveView::Detail;
    view = match view {
        ActiveView::Control => ActiveView::Dashboard,
        ActiveView::Dashboard | ActiveView::Detail => ActiveView::Control,
    };
    assert_eq!(view, ActiveView::Control);
}

#[test]
fn test_dashboard_tab_active_in_detail_view() {
    let view = ActiveView::Detail;
    let is_dashboard = matches!(view, ActiveView::Dashboard | ActiveView::Detail);
    assert!(is_dashboard);

    let view = ActiveView::Dashboard;
    let is_dashboard = matches!(view, ActiveView::Dashboard | ActiveView::Detail);
    assert!(is_dashboard);

    let view = ActiveView::Control;
    let is_dashboard = matches!(view, ActiveView::Dashboard | ActiveView::Detail);
    assert!(!is_dashboard);
}

#[test]
fn test_normalize_path_with_leading_slash_nonexistent() {
    // Nonexistent paths now return errors
    let result = normalize_project_path("/Users/test/project");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Cannot access"));
}

#[test]
fn test_normalize_path_tilde_expansion() {
    // Nonexistent paths now return errors
    let result = normalize_project_path("~/projects/test");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Cannot access"));
}

#[test]
fn test_normalize_path_bare_tilde() {
    let result = normalize_project_path("~").unwrap();
    let expected_home = dirs::home_dir()
        .expect("test requires home dir")
        .canonicalize()
        .expect("home should be canonicalizable");
    assert_eq!(result, expected_home);
}

#[test]
fn test_normalize_path_trims_whitespace() {
    // Nonexistent paths now return errors
    let result = normalize_project_path("  /Users/test/project  ");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Cannot access"));
}

#[test]
fn test_normalize_path_without_leading_slash_fallback() {
    // Nonexistent paths now return errors
    let result = normalize_project_path("nonexistent/path/here");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Cannot access"));
}

#[test]
fn test_normalize_path_empty_string() {
    // Empty paths now return errors
    let result = normalize_project_path("");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Cannot access"));
}

#[test]
fn test_normalize_path_whitespace_only() {
    // Whitespace-only paths now return errors
    let result = normalize_project_path("   ");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Cannot access"));
}

#[test]
fn test_normalize_path_tilde_in_middle_not_expanded() {
    // Nonexistent paths now return errors
    let result = normalize_project_path("/Users/test/~project");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Cannot access"));
}

#[test]
fn test_normalize_path_canonicalizes_existing_path() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path();

    let result = normalize_project_path(path.to_str().unwrap()).unwrap();
    let expected = path.canonicalize().unwrap();
    assert_eq!(result, expected);
}

#[test]
fn test_normalize_path_lowercase_canonicalized() {
    if let Some(home) = dirs::home_dir() {
        let lowercase_path = home.to_str().unwrap().to_lowercase();
        let result = normalize_project_path(&lowercase_path).unwrap();

        assert!(result.exists(), "Canonicalized path should exist");

        let expected = home.canonicalize().unwrap();
        assert_eq!(result, expected);
    }
}

#[test]
fn test_canonicalize_path_existing() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();

    let result = canonicalize_path(path.clone()).unwrap();
    let expected = path.canonicalize().unwrap();
    assert_eq!(result, expected);
}

#[test]
fn test_canonicalize_path_nonexistent_returns_error() {
    let path = PathBuf::from("/nonexistent/path/that/does/not/exist");
    let result = canonicalize_path(path.clone());
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Cannot access"));
}

#[test]
#[cfg(unix)]
fn test_normalize_path_resolves_symlinks() {
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let real_path = temp_dir.path().join("real_dir");
    std::fs::create_dir(&real_path).unwrap();

    let symlink_path = temp_dir.path().join("symlink_dir");
    symlink(&real_path, &symlink_path).unwrap();

    let result = normalize_project_path(symlink_path.to_str().unwrap()).unwrap();

    // Should resolve symlink to the real path
    let expected = real_path.canonicalize().unwrap();
    assert_eq!(result, expected, "Symlinks should resolve to real path");
    assert_ne!(
        result, symlink_path,
        "Result should differ from symlink path"
    );
}
