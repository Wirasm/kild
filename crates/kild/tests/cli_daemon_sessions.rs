//! Integration tests for CLI behavior with daemon-managed sessions.
//!
//! Tests verify that `hide`, `focus`, and removed commands handle daemon
//! sessions correctly. Uses HOME env var override to isolate session state.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Create a temporary HOME directory with `.kild/sessions/` for test isolation.
///
/// Returns the temp home path. The `dirs::home_dir()` call inside `Config::new()`
/// reads `$HOME`, so overriding it in the spawned process isolates session state.
fn setup_test_home(test_name: &str) -> PathBuf {
    let unique_id = format!(
        "{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let temp_home = std::env::temp_dir().join(format!("kild_test_{}_{}", test_name, unique_id));
    let sessions_dir = temp_home.join(".kild").join("sessions");
    fs::create_dir_all(&sessions_dir).expect("Failed to create test sessions dir");
    temp_home
}

/// Write a daemon-managed session fixture to the sessions directory.
fn write_daemon_session(sessions_dir: &PathBuf, branch: &str) {
    let id = format!("test/{}", branch);
    let filename = format!("{}.json", id.replace('/', "_"));
    let json = serde_json::json!({
        "id": id,
        "project_id": "test",
        "branch": branch,
        "worktree_path": "/tmp/kild-test-nonexistent",
        "agent": "claude",
        "status": "Active",
        "created_at": "2024-01-01T00:00:00Z",
        "port_range_start": 3000,
        "port_range_end": 3009,
        "port_count": 10,
        "runtime_mode": "Daemon",
        "agents": [{
            "agent": "claude",
            "spawn_id": format!("test_{}_0", branch),
            "process_id": null,
            "process_name": null,
            "process_start_time": null,
            "terminal_type": null,
            "terminal_window_id": null,
            "command": "claude-code",
            "opened_at": "2024-01-01T00:00:00Z",
            "daemon_session_id": "550e8400-e29b-41d4-a716-446655440000"
        }]
    });
    fs::write(
        sessions_dir.join(filename),
        serde_json::to_string_pretty(&json).unwrap(),
    )
    .expect("Failed to write daemon session fixture");
}

// =============================================================================
// hide --all: daemon sessions skipped gracefully
// =============================================================================

/// `kild hide --all` with only daemon sessions should exit 0 and report skipped count.
///
/// This is the core regression test for #355 where `hide --all` incorrectly
/// returned exit code 1 when daemon sessions were present.
#[test]
fn test_hide_all_skips_daemon_sessions_exits_zero() {
    let temp_home = setup_test_home("hide_all_daemon");
    let sessions_dir = temp_home.join(".kild").join("sessions");

    write_daemon_session(&sessions_dir, "daemon-one");
    write_daemon_session(&sessions_dir, "daemon-two");

    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .env("HOME", &temp_home)
        .args(["hide", "--all"])
        .output()
        .expect("Failed to execute 'kild hide --all'");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "hide --all should exit 0 when only daemon sessions exist.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    assert!(
        stdout.contains("Skipped 2 daemon-managed kilds"),
        "Should report 2 skipped daemon sessions, got stdout: {}",
        stdout
    );

    // Should NOT report any hidden sessions
    assert!(
        !stdout.contains("Hidden"),
        "Should not report hidden sessions when all are daemon-managed, got stdout: {}",
        stdout
    );

    let _ = fs::remove_dir_all(&temp_home);
}

/// `kild hide --all` with no active sessions should print "no windows" message.
#[test]
fn test_hide_all_no_active_sessions() {
    let temp_home = setup_test_home("hide_all_empty");
    // No session files created — empty sessions dir

    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .env("HOME", &temp_home)
        .args(["hide", "--all"])
        .output()
        .expect("Failed to execute 'kild hide --all'");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "hide --all with no sessions should exit 0"
    );

    assert!(
        stdout.contains("No kild windows to hide"),
        "Should print 'no windows' message, got stdout: {}",
        stdout
    );

    let _ = fs::remove_dir_all(&temp_home);
}

// =============================================================================
// hide <daemon-branch>: actionable error
// =============================================================================

/// `kild hide <daemon-branch>` should fail with an actionable error suggesting `kild attach`.
#[test]
fn test_hide_daemon_session_errors_with_attach_hint() {
    let temp_home = setup_test_home("hide_daemon_single");
    let sessions_dir = temp_home.join(".kild").join("sessions");

    write_daemon_session(&sessions_dir, "my-daemon-kild");

    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .env("HOME", &temp_home)
        .args(["hide", "my-daemon-kild"])
        .output()
        .expect("Failed to execute 'kild hide'");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "hide on daemon session should fail"
    );

    assert!(
        stderr.contains("Cannot hide"),
        "Should contain daemon error message, got stderr: {}",
        stderr
    );

    assert!(
        stderr.contains("kild attach my-daemon-kild"),
        "Should suggest 'kild attach' with branch name, got stderr: {}",
        stderr
    );

    let _ = fs::remove_dir_all(&temp_home);
}

// =============================================================================
// focus <daemon-branch>: actionable error
// =============================================================================

/// `kild focus <daemon-branch>` should fail with an actionable error suggesting `kild attach`.
#[test]
fn test_focus_daemon_session_errors_with_attach_hint() {
    let temp_home = setup_test_home("focus_daemon");
    let sessions_dir = temp_home.join(".kild").join("sessions");

    write_daemon_session(&sessions_dir, "my-daemon-kild");

    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .env("HOME", &temp_home)
        .args(["focus", "my-daemon-kild"])
        .output()
        .expect("Failed to execute 'kild focus'");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "focus on daemon session should fail"
    );

    assert!(
        stderr.contains("Cannot focus"),
        "Should contain daemon error message, got stderr: {}",
        stderr
    );

    assert!(
        stderr.contains("kild attach my-daemon-kild"),
        "Should suggest 'kild attach' with branch name, got stderr: {}",
        stderr
    );

    let _ = fs::remove_dir_all(&temp_home);
}

// =============================================================================
// restart command removed
// =============================================================================

/// `kild restart` should be an unrecognized subcommand after removal.
#[test]
fn test_restart_command_removed() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["restart", "some-branch"])
        .output()
        .expect("Failed to execute 'kild restart'");

    assert!(
        !output.status.success(),
        "restart should not be a recognized subcommand"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("unrecognized subcommand"),
        "Should indicate restart is not valid, got stderr: {}",
        stderr
    );
}
