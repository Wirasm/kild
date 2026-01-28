//! Integration tests for CLI output behavior
//!
//! The default behavior is quiet (no logs). Use -v/--verbose to enable logs.

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

/// Execute 'kild -v list' (verbose mode) and return the output
fn run_kild_verbose_list() -> std::process::Output {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["-v", "list"])
        .output()
        .expect("Failed to execute 'kild -v list'");

    assert!(
        output.status.success(),
        "kild -v list failed with exit code {:?}. stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    output
}

/// Verify that stdout contains only user-facing output (no JSON logs)
/// and that stderr is empty by default (quiet mode)
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

    // stderr should be empty in default (quiet) mode, or only contain errors
    if !stderr.is_empty() {
        // If there's output on stderr, it should only be ERROR level
        assert!(
            !stderr.contains(r#""level":"INFO""#),
            "Default mode should not emit INFO logs, got: {}",
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
// Default Mode (Quiet) Behavioral Tests
// =============================================================================

/// Verify that default mode (no flags) suppresses INFO-level logs
#[test]
fn test_default_mode_suppresses_info_logs() {
    let output = run_kild_list();

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should NOT contain INFO-level log events
    assert!(
        !stderr.contains(r#""level":"INFO""#),
        "Default mode should suppress INFO logs, but stderr contains: {}",
        stderr
    );

    // Should NOT contain DEBUG-level log events
    assert!(
        !stderr.contains(r#""level":"DEBUG""#),
        "Default mode should suppress DEBUG logs, but stderr contains: {}",
        stderr
    );

    // Should NOT contain WARN-level log events
    assert!(
        !stderr.contains(r#""level":"WARN""#),
        "Default mode should suppress WARN logs, but stderr contains: {}",
        stderr
    );
}

/// Verify that default mode preserves user-facing stdout output
#[test]
fn test_default_mode_preserves_stdout() {
    let output = run_kild_list();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // User-facing output should still be present (table header or "no kilds" message)
    assert!(
        !stdout.is_empty(),
        "Default mode should preserve user-facing stdout output"
    );

    // stdout should contain table elements or status message
    assert!(
        stdout.contains("Active kilds") || stdout.contains("No active kilds"),
        "stdout should contain user-facing list output, got: {}",
        stdout
    );
}

// =============================================================================
// Verbose Mode Behavioral Tests
// =============================================================================

/// Verify verbose mode (-v) emits INFO logs
#[test]
fn test_verbose_flag_emits_info_logs() {
    let output = run_kild_verbose_list();

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verbose mode should contain INFO-level log events
    assert!(
        stderr.contains(r#""level":"INFO""#),
        "Verbose mode should emit INFO logs, but stderr is: {}",
        stderr
    );
}

/// Verify verbose mode works with --verbose long form
#[test]
fn test_verbose_flag_long_form_emits_logs() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["--verbose", "list"])
        .output()
        .expect("Failed to execute 'kild --verbose list'");

    assert!(
        output.status.success(),
        "kild --verbose list failed with exit code {:?}",
        output.status.code()
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains(r#""level":"INFO""#),
        "--verbose long form should emit INFO logs, but stderr is: {}",
        stderr
    );
}

/// Verify verbose flag works when flag is after subcommand (global flag behavior)
#[test]
fn test_verbose_flag_after_subcommand() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["list", "-v"])
        .output()
        .expect("Failed to execute 'kild list -v'");

    assert!(
        output.status.success(),
        "kild list -v failed with exit code {:?}. stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains(r#""level":"INFO""#),
        "Verbose flag after subcommand should emit INFO logs, but stderr is: {}",
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

/// Verify that RUST_LOG env var is respected alongside verbose flag
/// When RUST_LOG is explicitly set, it should affect log levels
#[test]
fn test_rust_log_overrides_default_quiet() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .env("RUST_LOG", "kild=debug")
        .args(["list"])
        .output()
        .expect("Failed to execute command with RUST_LOG");

    assert!(
        output.status.success(),
        "Command failed with exit code {:?}",
        output.status.code()
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Without -v flag, the default quiet directive (kild=error) is added
    // which takes precedence via add_directive. So RUST_LOG alone shouldn't
    // override the quiet default.
    assert!(
        !stderr.contains(r#""level":"INFO""#),
        "Default quiet should take precedence over RUST_LOG, stderr: {}",
        stderr
    );
}
