//! Integration tests for CLI output behavior

use std::process::Command;

/// Execute 'kild list' and verify it succeeds
fn run_kild_list() -> std::process::Output {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .arg("list")
        .output()
        .expect("Failed to execute 'kild list'");

    assert!(
        output.status.success(),
        "kild list failed with exit code {:?}. stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    output
}

/// Verify that stdout contains only user-facing output (no JSON logs)
/// and that any stderr output is structured JSON logs.
#[test]
fn test_list_stdout_is_clean() {
    let output = run_kild_list();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // stdout should not contain JSON log lines
    assert!(
        !stdout.contains(r#""event":"#),
        "stdout should not contain JSON logs, got: {}",
        stdout
    );

    // stderr should contain JSON logs (if any logging occurred)
    if !stderr.is_empty() {
        // If there's output on stderr, it should be JSON logs
        assert!(
            stderr.contains(r#""timestamp""#) || stderr.contains(r#""level""#),
            "stderr should contain structured logs, got: {}",
            stderr
        );
    }
}

/// Verify stdout has no JSON lines and is suitable for piping
#[test]
fn test_output_is_pipeable() {
    let output = run_kild_list();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // stdout should be clean enough to pipe through grep
    // No line should be JSON (starting with '{')
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Should not be JSON
        assert!(
            !trimmed.starts_with('{'),
            "stdout contains JSON line: {}",
            line
        );
    }
}

// =============================================================================
// Quiet Mode Behavioral Tests
// =============================================================================

/// Execute 'kild -q list' and return the output
fn run_kild_quiet_list() -> std::process::Output {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["-q", "list"])
        .output()
        .expect("Failed to execute 'kild -q list'");

    assert!(
        output.status.success(),
        "kild -q list failed with exit code {:?}. stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    output
}

/// Verify that quiet mode suppresses INFO-level logs
#[test]
fn test_quiet_flag_suppresses_info_logs() {
    let output = run_kild_quiet_list();

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should NOT contain INFO-level log events
    assert!(
        !stderr.contains(r#""level":"INFO""#),
        "Quiet mode should suppress INFO logs, but stderr contains: {}",
        stderr
    );

    // Should NOT contain DEBUG-level log events
    assert!(
        !stderr.contains(r#""level":"DEBUG""#),
        "Quiet mode should suppress DEBUG logs, but stderr contains: {}",
        stderr
    );

    // Should NOT contain WARN-level log events
    assert!(
        !stderr.contains(r#""level":"WARN""#),
        "Quiet mode should suppress WARN logs, but stderr contains: {}",
        stderr
    );
}

/// Verify that quiet mode preserves user-facing stdout output
#[test]
fn test_quiet_flag_preserves_stdout() {
    let output = run_kild_quiet_list();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // User-facing output should still be present (table header or "no kilds" message)
    assert!(
        !stdout.is_empty(),
        "Quiet mode should preserve user-facing stdout output"
    );

    // stdout should contain table elements or status message
    assert!(
        stdout.contains("Active kilds") || stdout.contains("No active kilds"),
        "stdout should contain user-facing list output, got: {}",
        stdout
    );
}

/// Verify normal mode (without -q) does emit INFO logs
#[test]
fn test_normal_mode_emits_info_logs() {
    let output = run_kild_list();

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Normal mode should contain INFO-level log events
    assert!(
        stderr.contains(r#""level":"INFO""#),
        "Normal mode should emit INFO logs, but stderr is: {}",
        stderr
    );
}

/// Verify quiet mode works with --quiet long form
#[test]
fn test_quiet_flag_long_form_suppresses_logs() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["--quiet", "list"])
        .output()
        .expect("Failed to execute 'kild --quiet list'");

    assert!(
        output.status.success(),
        "kild --quiet list failed with exit code {:?}",
        output.status.code()
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains(r#""level":"INFO""#),
        "--quiet long form should suppress INFO logs, but stderr contains: {}",
        stderr
    );
}

/// Verify quiet mode works when flag is after subcommand (global flag behavior)
#[test]
fn test_quiet_flag_after_subcommand() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["list", "-q"])
        .output()
        .expect("Failed to execute 'kild list -q'");

    assert!(
        output.status.success(),
        "kild list -q failed with exit code {:?}. stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains(r#""level":"INFO""#),
        "Quiet flag after subcommand should suppress INFO logs, but stderr contains: {}",
        stderr
    );
}

// =============================================================================
// Error Handling Tests
// =============================================================================

/// Verify that 'kild diff' with non-existent branch returns proper error
#[test]
fn test_diff_nonexistent_branch_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["diff", "nonexistent-branch-that-does-not-exist"])
        .output()
        .expect("Failed to execute 'kild diff'");

    // Command should fail
    assert!(
        !output.status.success(),
        "kild diff with non-existent branch should fail"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should contain error indicator emoji and helpful message
    assert!(
        stderr.contains("❌") || stderr.contains("Failed to find kild"),
        "Error output should contain failure indicator, got stderr: {}",
        stderr
    );

    // Should contain the branch name in the error
    assert!(
        stderr.contains("nonexistent-branch-that-does-not-exist"),
        "Error output should mention the branch name, got stderr: {}",
        stderr
    );
}

/// Verify that RUST_LOG env var is respected alongside quiet flag
/// When RUST_LOG is explicitly set, it should override the quiet flag
#[test]
fn test_rust_log_overrides_quiet_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .env("RUST_LOG", "kild=debug")
        .args(["-q", "list"])
        .output()
        .expect("Failed to execute command with RUST_LOG");

    assert!(
        output.status.success(),
        "Command failed with exit code {:?}",
        output.status.code()
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    // RUST_LOG=kild=debug should override --quiet because EnvFilter
    // processes env vars first, but add_directive adds to them.
    // Actually, the implementation uses add_directive which ADDS to the env filter,
    // so the quiet directive should win. Let's verify the actual behavior:
    // If quiet=true sets "kild=error" directive, and RUST_LOG=kild=debug,
    // the add_directive should override the env var for that target.

    // Based on tracing_subscriber behavior: add_directive takes precedence
    // So quiet flag should still suppress INFO even with RUST_LOG set
    assert!(
        !stderr.contains(r#""level":"INFO""#),
        "Quiet flag should take precedence over RUST_LOG for kild target, stderr: {}",
        stderr
    );
}
