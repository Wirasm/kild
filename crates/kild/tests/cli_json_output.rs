//! Integration tests for CLI JSON output behavior
//!
//! These tests verify that --json flag produces valid, parseable JSON output
//! for automation and scripting workflows.

use std::process::Command;

/// Execute 'kild list --json' and return the output
fn run_kild_list_json() -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["list", "--json"])
        .output()
        .expect("Failed to execute 'kild list --json'")
}

/// Verify that 'kild list --json' outputs valid JSON array
#[test]
fn test_list_json_outputs_valid_json_array() {
    let output = run_kild_list_json();

    assert!(
        output.status.success(),
        "kild list --json failed with exit code {:?}. stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify output is valid JSON
    let sessions: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    // Must be an array (even if empty)
    assert!(
        sessions.is_array(),
        "JSON output should be an array, got: {}",
        stdout
    );
}

/// Verify that empty list returns empty JSON array '[]'
#[test]
fn test_list_json_empty_returns_empty_array() {
    let output = run_kild_list_json();

    if !output.status.success() {
        // If command fails (e.g., not in a git repo), skip this test
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    let sessions: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    // If array is empty, verify it's actually [] not null or something else
    if let Some(arr) = sessions.as_array() {
        if arr.is_empty() {
            // Verify the raw output is actually an empty array
            assert!(
                stdout.trim() == "[]",
                "Empty list should output '[]', got: {}",
                stdout
            );
        }
    }
}

/// Verify that JSON output contains expected Session fields when sessions exist
#[test]
fn test_list_json_session_fields() {
    let output = run_kild_list_json();

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sessions: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    // If there are sessions, verify the structure has expected fields
    if let Some(arr) = sessions.as_array() {
        if let Some(first) = arr.first() {
            // Core required fields from Session struct
            assert!(first.get("id").is_some(), "Session should have 'id' field");
            assert!(
                first.get("branch").is_some(),
                "Session should have 'branch' field"
            );
            assert!(
                first.get("status").is_some(),
                "Session should have 'status' field"
            );
            assert!(
                first.get("worktree_path").is_some(),
                "Session should have 'worktree_path' field"
            );
            assert!(
                first.get("agent").is_some(),
                "Session should have 'agent' field"
            );
            assert!(
                first.get("created_at").is_some(),
                "Session should have 'created_at' field"
            );
        }
    }
}

/// Verify that logs go to stderr, not stdout in JSON mode
#[test]
fn test_list_json_logs_to_stderr_not_stdout() {
    let output = run_kild_list_json();

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // stdout should ONLY contain the JSON array, not log lines
    assert!(
        !stdout.contains(r#""event":"#),
        "JSON mode: logs should go to stderr, not stdout. Got: {}",
        stdout
    );

    // stdout should not contain timestamp fields from logs
    assert!(
        !stdout.contains(r#""timestamp":"#),
        "JSON mode: log timestamps should go to stderr, not stdout. Got: {}",
        stdout
    );
}

/// Verify that 'kild status <branch> --json' outputs valid JSON object
#[test]
fn test_status_json_outputs_valid_json_object() {
    // First get a branch name from list
    let list_output = run_kild_list_json();

    if !list_output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let sessions: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("list output should be valid JSON");

    // Skip if no sessions exist
    if sessions.is_empty() {
        return;
    }

    let branch = sessions[0]["branch"]
        .as_str()
        .expect("Session should have branch field");

    // Now test status --json
    let status_output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["status", branch, "--json"])
        .output()
        .expect("Failed to execute 'kild status --json'");

    assert!(
        status_output.status.success(),
        "kild status --json failed: {:?}",
        status_output.status
    );

    let status_stdout = String::from_utf8_lossy(&status_output.stdout);

    // Verify output is valid JSON object (not array)
    let session: serde_json::Value =
        serde_json::from_str(&status_stdout).expect("status stdout should be valid JSON");

    assert!(
        session.is_object(),
        "status --json should output an object, got: {}",
        status_stdout
    );

    // Verify the branch matches
    assert_eq!(
        session.get("branch").and_then(|v| v.as_str()),
        Some(branch),
        "Status session branch should match requested branch"
    );
}

/// Verify status --json has expected Session fields
#[test]
fn test_status_json_session_fields() {
    // First get a branch name from list
    let list_output = run_kild_list_json();

    if !list_output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let sessions: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("list output should be valid JSON");

    if sessions.is_empty() {
        return;
    }

    let branch = sessions[0]["branch"]
        .as_str()
        .expect("Session should have branch field");

    let status_output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["status", branch, "--json"])
        .output()
        .expect("Failed to execute 'kild status --json'");

    if !status_output.status.success() {
        return;
    }

    let status_stdout = String::from_utf8_lossy(&status_output.stdout);
    let session: serde_json::Value =
        serde_json::from_str(&status_stdout).expect("status stdout should be valid JSON");

    // Verify all expected fields exist
    assert!(session.get("id").is_some(), "Should have 'id' field");
    assert!(
        session.get("branch").is_some(),
        "Should have 'branch' field"
    );
    assert!(
        session.get("status").is_some(),
        "Should have 'status' field"
    );
    assert!(
        session.get("worktree_path").is_some(),
        "Should have 'worktree_path' field"
    );
    assert!(session.get("agent").is_some(), "Should have 'agent' field");
    assert!(
        session.get("created_at").is_some(),
        "Should have 'created_at' field"
    );
}

/// Verify status --json logs go to stderr
#[test]
fn test_status_json_logs_to_stderr_not_stdout() {
    let list_output = run_kild_list_json();

    if !list_output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let sessions: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("list output should be valid JSON");

    if sessions.is_empty() {
        return;
    }

    let branch = sessions[0]["branch"]
        .as_str()
        .expect("Session should have branch field");

    let status_output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["status", branch, "--json"])
        .output()
        .expect("Failed to execute 'kild status --json'");

    if !status_output.status.success() {
        return;
    }

    let status_stdout = String::from_utf8_lossy(&status_output.stdout);

    assert!(
        !status_stdout.contains(r#""event":"#),
        "status --json: logs should go to stderr, not stdout. Got: {}",
        status_stdout
    );
}

/// Verify JSON output is parseable (simulates jq usage without requiring jq)
#[test]
fn test_list_json_is_parseable_for_scripting() {
    let output = run_kild_list_json();

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse as generic JSON Value first
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("Should parse as JSON Value");

    // Then verify we can iterate through array (common jq pattern)
    if let Some(arr) = value.as_array() {
        for session in arr {
            // Simulate: jq '.[] | .branch' - extracting branch from each session
            let _branch = session
                .get("branch")
                .and_then(|v| v.as_str())
                .expect("Each session should have string 'branch' field");

            // Simulate: jq '.[] | select(.status == "Active")'
            let _status = session
                .get("status")
                .and_then(|v| v.as_str())
                .expect("Each session should have string 'status' field");
        }
    }
}

/// Verify that 'kild list --json' includes git_stats per session
#[test]
fn test_list_json_includes_git_stats() {
    let output = run_kild_list_json();

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sessions: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("list output should be valid JSON");

    // If there are sessions, verify git_stats key exists
    if let Some(first) = sessions.first() {
        assert!(
            first.get("git_stats").is_some(),
            "Each session in list --json should have 'git_stats' field. Got: {}",
            serde_json::to_string_pretty(first).unwrap()
        );
    }
}

/// Verify that 'kild status <branch> --json' includes git_stats
#[test]
fn test_status_json_includes_git_stats() {
    let list_output = run_kild_list_json();

    if !list_output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let sessions: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("list output should be valid JSON");

    if sessions.is_empty() {
        return;
    }

    let branch = sessions[0]["branch"]
        .as_str()
        .expect("Session should have branch field");

    let status_output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["status", branch, "--json"])
        .output()
        .expect("Failed to execute 'kild status --json'");

    if !status_output.status.success() {
        return;
    }

    let status_stdout = String::from_utf8_lossy(&status_output.stdout);
    let session: serde_json::Value =
        serde_json::from_str(&status_stdout).expect("status stdout should be valid JSON");

    assert!(
        session.get("git_stats").is_some(),
        "status --json should include 'git_stats' field. Got: {}",
        status_stdout
    );
}
