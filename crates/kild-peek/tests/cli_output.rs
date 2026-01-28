//! Integration tests for kild-peek CLI output behavior
//!
//! The default behavior is quiet (no logs). Use -v/--verbose to enable logs.

use std::process::Command;

/// Execute 'kild-peek list windows' and verify it succeeds
fn run_peek_list_windows() -> std::process::Output {
    let output = Command::new(env!("CARGO_BIN_EXE_kild-peek"))
        .args(["list", "windows"])
        .output()
        .expect("Failed to execute 'kild-peek list windows'");

    assert!(
        output.status.success(),
        "kild-peek list windows failed with exit code {:?}. stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    output
}

/// Execute 'kild-peek -v list windows' (verbose mode) and return the output
fn run_peek_verbose_list_windows() -> std::process::Output {
    let output = Command::new(env!("CARGO_BIN_EXE_kild-peek"))
        .args(["-v", "list", "windows"])
        .output()
        .expect("Failed to execute 'kild-peek -v list windows'");

    assert!(
        output.status.success(),
        "kild-peek -v list windows failed with exit code {:?}. stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    output
}

// =============================================================================
// Default Mode (Quiet) Behavioral Tests
// =============================================================================

/// Verify that default mode (no flags) suppresses INFO-level logs
#[test]
fn test_default_mode_suppresses_info_logs() {
    let output = run_peek_list_windows();

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

/// Verify that stdout contains only user-facing output (no JSON logs)
#[test]
fn test_stdout_is_clean() {
    let output = run_peek_list_windows();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // stdout should not contain JSON log lines
    assert!(
        !stdout.contains(r#""event":"#),
        "stdout should not contain JSON logs, got: {}",
        stdout
    );
}

// =============================================================================
// Verbose Mode Behavioral Tests
// =============================================================================

/// Verify verbose mode (-v) emits INFO logs
#[test]
fn test_verbose_flag_emits_info_logs() {
    let output = run_peek_verbose_list_windows();

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
    let output = Command::new(env!("CARGO_BIN_EXE_kild-peek"))
        .args(["--verbose", "list", "windows"])
        .output()
        .expect("Failed to execute 'kild-peek --verbose list windows'");

    assert!(
        output.status.success(),
        "kild-peek --verbose list windows failed with exit code {:?}",
        output.status.code()
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains(r#""level":"INFO""#),
        "--verbose long form should emit INFO logs, but stderr is: {}",
        stderr
    );
}
