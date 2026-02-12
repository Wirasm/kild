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

/// Parse 'kild list --json' output and extract sessions array
fn parse_list_sessions(stdout: &str) -> Vec<serde_json::Value> {
    let value: serde_json::Value =
        serde_json::from_str(stdout).expect("list output should be valid JSON");
    value["sessions"]
        .as_array()
        .expect("list output should have 'sessions' array")
        .clone()
}

/// Verify that 'kild list --json' outputs valid JSON object with sessions and fleet_summary
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

    // Verify output is valid JSON object
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert!(
        value.is_object(),
        "JSON output should be an object, got: {}",
        stdout
    );

    // Must have sessions array
    assert!(
        value.get("sessions").and_then(|v| v.as_array()).is_some(),
        "JSON output should have 'sessions' array"
    );

    // Must have fleet_summary object
    let summary = value
        .get("fleet_summary")
        .expect("JSON output should have 'fleet_summary'");
    assert!(summary.is_object(), "fleet_summary should be an object");
    assert!(
        summary.get("total").is_some(),
        "fleet_summary should have 'total'"
    );
    assert!(
        summary.get("active").is_some(),
        "fleet_summary should have 'active'"
    );
    assert!(
        summary.get("stopped").is_some(),
        "fleet_summary should have 'stopped'"
    );
    assert!(
        summary.get("conflicts").is_some(),
        "fleet_summary should have 'conflicts'"
    );
    assert!(
        summary.get("needs_push").is_some(),
        "fleet_summary should have 'needs_push'"
    );
}

/// Verify that empty list returns object with empty sessions array
#[test]
fn test_list_json_empty_returns_empty_array() {
    let output = run_kild_list_json();

    if !output.status.success() {
        // If command fails (e.g., not in a git repo), skip this test
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sessions = parse_list_sessions(&stdout);

    // If sessions is empty, verify fleet_summary has zeros for all fields
    if sessions.is_empty() {
        let value: serde_json::Value =
            serde_json::from_str(&stdout).expect("stdout should be valid JSON");
        let summary = value
            .get("fleet_summary")
            .expect("should have fleet_summary");
        assert_eq!(summary["total"], 0, "empty fleet should have total=0");
        assert_eq!(summary["active"], 0, "empty fleet should have active=0");
        assert_eq!(summary["stopped"], 0, "empty fleet should have stopped=0");
        assert_eq!(
            summary["conflicts"], 0,
            "empty fleet should have conflicts=0"
        );
        assert_eq!(
            summary["needs_push"], 0,
            "empty fleet should have needs_push=0"
        );
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
    let sessions = parse_list_sessions(&stdout);

    if let Some(first) = sessions.first() {
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
    let sessions = parse_list_sessions(&stdout);

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
    let sessions = parse_list_sessions(&stdout);

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
    let sessions = parse_list_sessions(&stdout);

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
    let sessions = parse_list_sessions(&stdout);

    // Simulate: jq '.sessions[] | .branch' - extracting branch from each session
    for session in &sessions {
        let _branch = session
            .get("branch")
            .and_then(|v| v.as_str())
            .expect("Each session should have string 'branch' field");

        // Simulate: jq '.sessions[] | select(.status == "Active")'
        let _status = session
            .get("status")
            .and_then(|v| v.as_str())
            .expect("Each session should have string 'status' field");
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
    let sessions = parse_list_sessions(&stdout);

    // If there are sessions, verify git_stats key exists
    if let Some(first) = sessions.first() {
        assert!(
            first.get("git_stats").is_some(),
            "Each session in list --json should have 'git_stats' field. Got: {}",
            serde_json::to_string_pretty(first).unwrap()
        );
    }
}

/// Verify that 'kild stats --all --json' always returns valid JSON (even empty)
#[test]
fn test_stats_all_json_outputs_valid_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["stats", "--all", "--json"])
        .output()
        .expect("Failed to execute 'kild stats --all --json'");

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let _value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stats --all --json stdout should be valid JSON");
}

/// Verify that 'kild overlaps --json' always returns valid JSON (even empty)
#[test]
fn test_overlaps_json_outputs_valid_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["overlaps", "--json"])
        .output()
        .expect("Failed to execute 'kild overlaps --json'");

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let _value: serde_json::Value =
        serde_json::from_str(&stdout).expect("overlaps --json stdout should be valid JSON");
}

/// Verify that 'kild status <branch> --json' includes git_stats
#[test]
fn test_status_json_includes_git_stats() {
    let list_output = run_kild_list_json();

    if !list_output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let sessions = parse_list_sessions(&stdout);

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

/// Verify that JSON output contains all enriched fields when sessions exist
#[test]
fn test_list_json_enriched_field_completeness() {
    let output = run_kild_list_json();

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sessions = parse_list_sessions(&stdout);

    if let Some(first) = sessions.first() {
        // All enriched fields must be present (even if null)
        let required_fields = [
            "git_stats",
            "agent_status",
            "agent_status_updated_at",
            "terminal_window_title",
            "terminal_type",
            "pr_info",
            "process_status",
            "branch_health",
            "merge_readiness",
            "overlapping_files",
        ];
        for field in &required_fields {
            assert!(
                first.get(field).is_some(),
                "list --json should have '{}' field (even if null). Got: {}",
                field,
                serde_json::to_string_pretty(first).unwrap()
            );
        }
    }
}

/// Verify that enum values in JSON output use consistent snake_case
#[test]
fn test_list_json_enum_consistency() {
    let output = run_kild_list_json();

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sessions = parse_list_sessions(&stdout);

    for session in &sessions {
        // SessionStatus must be snake_case
        if let Some(status) = session.get("status").and_then(|v| v.as_str()) {
            assert!(
                ["active", "stopped", "destroyed"].contains(&status),
                "status should be snake_case, got: '{}'",
                status
            );
        }

        // RuntimeMode must be snake_case (if present)
        if let Some(mode) = session.get("runtime_mode").and_then(|v| v.as_str()) {
            assert!(
                ["terminal", "daemon"].contains(&mode),
                "runtime_mode should be snake_case, got: '{}'",
                mode
            );
        }

        // process_status must be snake_case
        if let Some(ps) = session.get("process_status").and_then(|v| v.as_str()) {
            assert!(
                ["running", "stopped", "unknown"].contains(&ps),
                "process_status should be snake_case, got: '{}'",
                ps
            );
        }
    }
}

/// Verify optional fields serialize as null (not omitted)
#[test]
fn test_list_json_null_consistency() {
    let output = run_kild_list_json();

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sessions = parse_list_sessions(&stdout);

    if let Some(first) = sessions.first() {
        // These fields must always be present in the JSON object (even if value is null)
        let nullable_fields = [
            "git_stats",
            "agent_status",
            "agent_status_updated_at",
            "terminal_window_title",
            "terminal_type",
            "pr_info",
            "branch_health",
            "merge_readiness",
            "overlapping_files",
        ];
        for field in &nullable_fields {
            assert!(
                first.get(field).is_some(),
                "'{}' should be present in JSON (as value or null), not omitted",
                field
            );
        }
    }
}

/// Verify jq-style deep access on nested fields
#[test]
fn test_list_json_jq_deep_access() {
    let output = run_kild_list_json();

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sessions = parse_list_sessions(&stdout);

    if let Some(first) = sessions.first() {
        // Deep access: git_stats.worktree_status.unpushed_commit_count
        if let Some(git_stats) = first.get("git_stats") {
            if !git_stats.is_null() {
                // worktree_status may be null but the key should exist in git_stats
                assert!(
                    git_stats.get("worktree_status").is_some(),
                    "git_stats should have worktree_status key"
                );
            }
        }

        // Deep access: git_stats.diff_vs_base.insertions
        if let Some(git_stats) = first.get("git_stats") {
            if let Some(dvb) = git_stats.get("diff_vs_base") {
                if !dvb.is_null() {
                    assert!(
                        dvb.get("insertions").is_some(),
                        "diff_vs_base should have insertions"
                    );
                }
            }
        }
    }
}

/// Verify status --json has all enriched fields
#[test]
fn test_status_json_enriched_field_completeness() {
    let list_output = run_kild_list_json();

    if !list_output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let sessions = parse_list_sessions(&stdout);

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

    let required_fields = [
        "git_stats",
        "agent_status",
        "agent_status_updated_at",
        "terminal_window_title",
        "terminal_type",
        "pr_info",
        "process_status",
        "branch_health",
        "merge_readiness",
        "overlapping_files",
    ];
    for field in &required_fields {
        assert!(
            session.get(field).is_some(),
            "status --json should have '{}' field (even if null). Got: {}",
            field,
            serde_json::to_string_pretty(&session).unwrap()
        );
    }
}

/// Verify process_status values are valid
#[test]
fn test_list_json_process_status_values() {
    let output = run_kild_list_json();

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sessions = parse_list_sessions(&stdout);

    for session in &sessions {
        let ps = session
            .get("process_status")
            .and_then(|v| v.as_str())
            .expect("process_status should always be a string");
        assert!(
            ["running", "stopped", "unknown"].contains(&ps),
            "process_status must be one of running/stopped/unknown, got: '{}'",
            ps
        );
    }
}

/// Verify fleet_summary totals are consistent with sessions array
#[test]
fn test_fleet_summary_consistency_with_sessions() {
    let output = run_kild_list_json();

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    let sessions = value["sessions"]
        .as_array()
        .expect("should have sessions array");
    let summary = value
        .get("fleet_summary")
        .expect("should have fleet_summary");

    let total = summary["total"].as_u64().expect("total should be a number");
    let active = summary["active"]
        .as_u64()
        .expect("active should be a number");
    let stopped = summary["stopped"]
        .as_u64()
        .expect("stopped should be a number");

    // total must match sessions.len()
    assert_eq!(
        total,
        sessions.len() as u64,
        "fleet_summary.total ({}) must equal sessions.len() ({})",
        total,
        sessions.len()
    );

    // active + stopped <= total (destroyed sessions may account for the rest)
    assert!(
        active + stopped <= total,
        "active ({}) + stopped ({}) should be <= total ({})",
        active,
        stopped,
        total
    );

    // Verify counts match actual session statuses
    let actual_active = sessions
        .iter()
        .filter(|s| s.get("status").and_then(|v| v.as_str()) == Some("active"))
        .count() as u64;
    let actual_stopped = sessions
        .iter()
        .filter(|s| s.get("status").and_then(|v| v.as_str()) == Some("stopped"))
        .count() as u64;

    assert_eq!(
        active, actual_active,
        "fleet_summary.active ({}) should match actual active sessions ({})",
        active, actual_active
    );
    assert_eq!(
        stopped, actual_stopped,
        "fleet_summary.stopped ({}) should match actual stopped sessions ({})",
        stopped, actual_stopped
    );

    // needs_push must match sessions with unpushed commits or no remote branch
    let needs_push = summary["needs_push"]
        .as_u64()
        .expect("needs_push should be a number");
    let actual_needs_push = sessions
        .iter()
        .filter(|s| {
            s.get("git_stats")
                .and_then(|gs| gs.get("worktree_status"))
                .is_some_and(|ws| {
                    let unpushed = ws
                        .get("unpushed_commit_count")
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0);
                    let has_remote = ws
                        .get("has_remote_branch")
                        .and_then(|h| h.as_bool())
                        .unwrap_or(true);
                    unpushed > 0 || !has_remote
                })
        })
        .count() as u64;
    assert_eq!(
        needs_push, actual_needs_push,
        "fleet_summary.needs_push ({}) should match sessions needing push ({})",
        needs_push, actual_needs_push
    );
}

// =============================================================================
// JSON error output tests (issue #397)
// =============================================================================

/// Verify that 'kild status nonexistent --json' returns a JSON error object
#[test]
fn test_status_json_nonexistent_returns_json_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["status", "nonexistent-branch-xyz-12345", "--json"])
        .output()
        .expect("Failed to execute 'kild status --json'");

    // Command should fail (non-zero exit code)
    assert!(
        !output.status.success(),
        "kild status nonexistent --json should fail"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // stdout should contain valid JSON error object
    let error_obj: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON even on error");

    assert!(
        error_obj.is_object(),
        "JSON error should be an object, got: {}",
        stdout
    );

    // Must have "error" and "code" fields
    assert!(
        error_obj.get("error").and_then(|v| v.as_str()).is_some(),
        "JSON error should have 'error' string field"
    );
    assert!(
        error_obj.get("code").and_then(|v| v.as_str()).is_some(),
        "JSON error should have 'code' string field"
    );

    // Error code should be SESSION_NOT_FOUND
    assert_eq!(
        error_obj.get("code").and_then(|v| v.as_str()),
        Some("SESSION_NOT_FOUND"),
        "Error code should be SESSION_NOT_FOUND"
    );
}

/// Verify that 'kild pr nonexistent --json' returns a JSON error object
#[test]
fn test_pr_json_nonexistent_returns_json_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["pr", "nonexistent-branch-xyz-12345", "--json"])
        .output()
        .expect("Failed to execute 'kild pr --json'");

    assert!(
        !output.status.success(),
        "kild pr nonexistent --json should fail"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let error_obj: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON even on error");

    assert!(
        error_obj.get("error").is_some(),
        "Should have 'error' field"
    );
    assert!(error_obj.get("code").is_some(), "Should have 'code' field");
}

/// Verify that 'kild stats nonexistent --json' returns a JSON error object
#[test]
fn test_stats_json_nonexistent_returns_json_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["stats", "nonexistent-branch-xyz-12345", "--json"])
        .output()
        .expect("Failed to execute 'kild stats --json'");

    assert!(
        !output.status.success(),
        "kild stats nonexistent --json should fail"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let error_obj: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON even on error");

    assert!(
        error_obj.get("error").is_some(),
        "Should have 'error' field"
    );
    assert!(error_obj.get("code").is_some(), "Should have 'code' field");
}

/// Verify that 'kild health nonexistent --json' returns a JSON error object
#[test]
fn test_health_json_nonexistent_returns_json_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["health", "nonexistent-branch-xyz-12345", "--json"])
        .output()
        .expect("Failed to execute 'kild health --json'");

    assert!(
        !output.status.success(),
        "kild health nonexistent --json should fail"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let error_obj: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON even on error");

    assert!(
        error_obj.get("error").is_some(),
        "Should have 'error' field"
    );
    assert!(error_obj.get("code").is_some(), "Should have 'code' field");
}

/// Verify that non-JSON error mode still works (plain text to stderr)
#[test]
fn test_status_nonexistent_without_json_uses_stderr() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["status", "nonexistent-branch-xyz-12345"])
        .output()
        .expect("Failed to execute 'kild status'");

    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // stdout should be empty (no JSON)
    assert!(
        stdout.trim().is_empty(),
        "Without --json, stdout should be empty on error. Got: {}",
        stdout
    );

    // stderr should contain the error message
    assert!(
        stderr.contains("No kild found"),
        "Without --json, error should go to stderr. Got: {}",
        stderr
    );
}

// =============================================================================
// agent-status JSON output tests (issue #361)
// =============================================================================

/// Verify that 'kild agent-status nonexistent working --json' returns a JSON error object
#[test]
fn test_agent_status_json_nonexistent_returns_json_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args([
            "agent-status",
            "nonexistent-branch-xyz-12345",
            "working",
            "--json",
        ])
        .output()
        .expect("Failed to execute 'kild agent-status --json'");

    // Command should fail (non-zero exit code)
    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let error_obj: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON even on error");

    assert!(error_obj.is_object());
    assert_eq!(
        error_obj.get("code").and_then(|v| v.as_str()),
        Some("SESSION_NOT_FOUND")
    );
}

/// Verify that 'kild agent-status <branch> bogus --json' returns an error
#[test]
fn test_agent_status_json_invalid_status_returns_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["agent-status", "some-branch", "bogus", "--json"])
        .output()
        .expect("Failed to execute 'kild agent-status --json'");

    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let error_obj: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON even on error");

    assert!(error_obj.is_object());
    assert!(error_obj.get("error").is_some());
    assert!(error_obj.get("code").is_some());
}

/// Verify text output includes fleet summary line when sessions exist
#[test]
fn test_list_text_output_includes_fleet_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .arg("list")
        .output()
        .expect("Failed to execute 'kild list'");

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Skip if no sessions (empty state shows different message)
    if stdout.contains("No active kilds found") {
        return;
    }

    // Verify summary line exists with expected format:
    // "3 kilds: 2 active, 1 stopped | 0 conflicts | 1 needs push"
    let has_summary_line = stdout.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.contains("kilds:")
            && trimmed.contains("active,")
            && trimmed.contains("stopped")
            && trimmed.contains("conflicts")
            && trimmed.contains("needs push")
    });

    assert!(
        has_summary_line,
        "text output should include fleet summary line. Got:\n{}",
        stdout
    );
}
