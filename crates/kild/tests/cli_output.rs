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

    // stderr should be empty in default (quiet) mode — all log levels suppressed
    assert!(
        stderr.is_empty(),
        "Default quiet mode should have empty stderr, got: {}",
        stderr
    );
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

/// Verify that default mode (no flags) suppresses all log levels
#[test]
fn test_default_mode_suppresses_all_logs() {
    let output = run_kild_list();

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should NOT contain any log level — quiet mode is OFF, not ERROR
    for level in &["INFO", "DEBUG", "WARN", "ERROR", "TRACE"] {
        let pattern = format!(r#""level":"{}""#, level);
        assert!(
            !stderr.contains(&pattern),
            "Default mode should suppress {} logs, but stderr contains: {}",
            level,
            stderr
        );
    }
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

    // Should contain a clear error message
    assert!(
        stderr.contains("No kild found"),
        "Error output should contain failure indicator, got stderr: {}",
        stderr
    );

    // Should contain the branch name in the error
    assert!(
        stderr.contains("nonexistent-branch-that-does-not-exist"),
        "Error output should mention the branch name, got stderr: {}",
        stderr
    );

    // Should NOT contain JSON log lines
    assert!(
        !stderr.contains(r#""level":"ERROR""#),
        "Default mode should suppress ERROR JSON logs in error output, got: {}",
        stderr
    );

    // Should NOT contain raw Rust Debug error representation
    assert!(
        !stderr.contains("Error: "),
        "Should not show Rust Debug error representation, got: {}",
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

    // Without -v flag, the default quiet directive (kild=off) is added
    // which takes precedence via add_directive. So RUST_LOG alone shouldn't
    // override the quiet default.
    assert!(
        !stderr.contains(r#""level":"INFO""#),
        "Default quiet should take precedence over RUST_LOG, stderr: {}",
        stderr
    );
}

// =============================================================================
// Clean Error Output Tests
// =============================================================================

/// Verify that error output in default mode contains ONLY the user-facing message
#[test]
fn test_error_output_is_clean_in_default_mode() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["status", "nonexistent-branch-that-does-not-exist"])
        .output()
        .expect("Failed to execute 'kild status'");

    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should contain the user-facing error message
    assert!(
        stderr.contains("No kild found"),
        "Should contain user-facing error indicator, got: {}",
        stderr
    );

    // Should NOT contain JSON log lines
    assert!(
        !stderr.contains(r#""level":"ERROR""#),
        "Default mode should suppress ERROR JSON logs, got: {}",
        stderr
    );

    // Should NOT contain raw Rust Debug representation
    assert!(
        !stderr.contains("Error: NotFound"),
        "Should not show Rust Debug error representation, got: {}",
        stderr
    );
}

/// Verify that verbose mode shows JSON logs on error
#[test]
fn test_error_output_verbose_shows_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_kild"))
        .args(["-v", "status", "nonexistent-branch-that-does-not-exist"])
        .output()
        .expect("Failed to execute 'kild -v status'");

    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should contain user-facing error
    assert!(
        stderr.contains("No kild found"),
        "Should contain user-facing error, got: {}",
        stderr
    );

    // Should contain JSON error logs in verbose mode
    assert!(
        stderr.contains(r#""level":"ERROR""#),
        "Verbose mode should show ERROR JSON logs, got: {}",
        stderr
    );
}
