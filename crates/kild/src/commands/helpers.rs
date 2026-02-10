use tracing::warn;

use kild_core::Session;
use kild_core::config::KildConfig;
use kild_core::terminal::types::TerminalType;

/// Branch name and agent name for a successfully opened kild
pub type OpenedKild = (String, String);

/// Branch name and error message for a failed operation
pub type FailedOperation = (String, String);

/// Extract terminal type and window ID from a session's latest agent.
///
/// Returns a tuple of (TerminalType, window_id) or an error message if either is missing.
pub fn get_terminal_info(session: &Session) -> Result<(TerminalType, String), String> {
    let latest = session
        .latest_agent()
        .ok_or_else(|| "No agent recorded for this kild".to_string())?;

    let terminal_type = latest
        .terminal_type()
        .cloned()
        .ok_or_else(|| "No terminal type recorded".to_string())?;

    let window_id = latest
        .terminal_window_id()
        .ok_or_else(|| "No window ID recorded".to_string())?
        .to_string();

    Ok((terminal_type, window_id))
}

/// Load configuration with warning on errors.
///
/// Falls back to defaults if config loading fails, but notifies the user via:
/// - stderr message for immediate visibility
/// - structured log event `cli.config.load_failed` for debugging
pub fn load_config_with_warning() -> KildConfig {
    match KildConfig::load_hierarchy() {
        Ok(config) => config,
        Err(e) => {
            eprintln!(
                "Warning: Could not load config: {}. Using defaults.\n\
                 Tip: Check ~/.kild/config.toml and ./.kild/config.toml for syntax errors.",
                e
            );
            warn!(
                event = "cli.config.load_failed",
                error = %e,
                "Config load failed, using defaults"
            );
            KildConfig::default()
        }
    }
}

/// Validate branch name to prevent injection attacks
pub fn is_valid_branch_name(name: &str) -> bool {
    // Allow alphanumeric, hyphens, underscores, and forward slashes
    // Prevent path traversal and special characters
    !name.is_empty()
        && !name.contains("..")
        && !name.starts_with('/')
        && !name.ends_with('/')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/')
        && name.len() <= 255
}

/// Check if user confirmation input indicates acceptance.
/// Accepts "y" or "yes" (case-insensitive).
pub fn is_confirmation_accepted(input: &str) -> bool {
    let normalized = input.trim().to_lowercase();
    normalized == "y" || normalized == "yes"
}

/// Format partial failure error message for bulk operations.
pub fn format_partial_failure_error(operation: &str, failed: usize, total: usize) -> String {
    format!(
        "Partial failure: {} of {} kild(s) failed to {}",
        failed, total, operation
    )
}

/// Ensure the daemon is running. If not running and auto_start is enabled,
/// spawns the daemon in the background and waits for it to become ready.
pub fn ensure_daemon_running(config: &KildConfig) -> Result<(), Box<dyn std::error::Error>> {
    if kild_core::daemon::client::ping_daemon().unwrap_or(false) {
        return Ok(());
    }

    if !config.daemon_auto_start() {
        return Err("Daemon is not running. To fix this, either:\n  \
             - Start it manually: kild daemon start\n  \
             - Enable auto-start in config: [daemon] auto_start = true\n  \
             - Use --no-daemon to launch in an external terminal instead"
            .into());
    }

    eprintln!("Starting daemon...");

    let daemon_binary = std::env::current_exe()?;
    std::process::Command::new(&daemon_binary)
        .args(["daemon", "start", "--foreground"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start daemon: {}", e))?;

    let socket_path = kild_core::daemon::socket_path();
    let timeout = std::time::Duration::from_secs(5);
    let start = std::time::Instant::now();

    loop {
        if socket_path.exists() && kild_core::daemon::client::ping_daemon().unwrap_or(false) {
            eprintln!("Daemon started.");
            return Ok(());
        }
        if start.elapsed() > timeout {
            if socket_path.exists() {
                return Err(
                    "Daemon socket exists but not responding to ping after 5s.\n\
                     Try: kild daemon stop && kild daemon start"
                        .into(),
                );
            } else {
                return Err("Daemon process spawned but socket not created after 5s.\n\
                     Check daemon logs: kild daemon start --foreground"
                    .into());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Resolve runtime mode from CLI flags and config.
///
/// Priority: --daemon / --no-daemon flags > [daemon] enabled config > Terminal default
pub fn resolve_runtime_mode(
    daemon_flag: bool,
    no_daemon_flag: bool,
    config: &KildConfig,
) -> kild_core::RuntimeMode {
    if daemon_flag {
        kild_core::RuntimeMode::Daemon
    } else if no_daemon_flag {
        kild_core::RuntimeMode::Terminal
    } else if config.is_daemon_enabled() {
        kild_core::RuntimeMode::Daemon
    } else {
        kild_core::RuntimeMode::Terminal
    }
}

/// Convert CLI args into an OpenMode.
pub fn resolve_open_mode(matches: &clap::ArgMatches) -> kild_core::OpenMode {
    if matches.get_flag("no-agent") {
        return kild_core::OpenMode::BareShell;
    }

    if let Some(agent) = matches.get_one::<String>("agent").cloned() {
        return kild_core::OpenMode::Agent(agent);
    }

    kild_core::OpenMode::DefaultAgent
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::truncate;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short     ");
        assert_eq!(truncate("this-is-a-very-long-string", 10), "this-is...");
        assert_eq!(truncate("exact", 5), "exact");
    }

    #[test]
    fn test_truncate_edge_cases() {
        assert_eq!(truncate("", 5), "     ");
        assert_eq!(truncate("abc", 3), "abc");
        assert_eq!(truncate("abcd", 3), "...");
    }

    #[test]
    fn test_truncate_utf8_safety() {
        // Test that truncation handles multi-byte UTF-8 characters safely
        // without panicking at byte boundaries

        // Emoji are 4 bytes each
        let emoji_note = "Test 🚀 rockets";
        let result = truncate(emoji_note, 10);
        assert_eq!(result.chars().count(), 10);
        assert!(result.ends_with("..."));

        // Multiple emoji
        let multi_emoji = "🎉🎊🎁🎈🎆";
        let result = truncate(multi_emoji, 4);
        assert_eq!(result.chars().count(), 4);
        assert!(result.ends_with("..."));

        // Mixed ASCII and emoji
        let mixed = "Hello 世界 🌍";
        let result = truncate(mixed, 8);
        assert_eq!(result.chars().count(), 8);

        // CJK characters (3 bytes each)
        let cjk = "日本語テスト";
        let result = truncate(cjk, 5);
        assert_eq!(result.chars().count(), 5);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_note_display() {
        // Test truncation at the note column width (30 chars)
        let long_note = "This is a very long note that exceeds thirty characters";
        let result = truncate(long_note, 30);
        assert_eq!(result.chars().count(), 30);
        assert!(result.contains("..."));

        // Short note should be padded
        let short_note = "Short";
        let result = truncate(short_note, 30);
        assert_eq!(result.chars().count(), 30);
        assert!(!result.contains("..."));
    }

    #[test]
    fn test_load_config_with_warning_returns_valid_config() {
        // When config loads (successfully or with fallback), should return a valid config
        let config = load_config_with_warning();
        // Should not panic and return a config with non-empty default agent
        assert!(!config.agent.default.is_empty());
    }

    #[test]
    fn test_is_valid_branch_name_accepts_valid_names() {
        // Simple alphanumeric names
        assert!(is_valid_branch_name("feature-auth"));
        assert!(is_valid_branch_name("my_branch"));
        assert!(is_valid_branch_name("branch123"));

        // Names with forward slashes (git feature branches)
        assert!(is_valid_branch_name("feat/login"));
        assert!(is_valid_branch_name("feature/user/auth"));

        // Mixed valid characters
        assert!(is_valid_branch_name("fix-123_test/branch"));
    }

    #[test]
    fn test_is_valid_branch_name_rejects_empty() {
        assert!(!is_valid_branch_name(""));
    }

    #[test]
    fn test_is_valid_branch_name_rejects_path_traversal() {
        // Path traversal attempts
        assert!(!is_valid_branch_name(".."));
        assert!(!is_valid_branch_name("foo/../bar"));
        assert!(!is_valid_branch_name("../etc/passwd"));
        assert!(!is_valid_branch_name("branch/.."));
    }

    #[test]
    fn test_is_valid_branch_name_rejects_absolute_paths() {
        assert!(!is_valid_branch_name("/absolute"));
        assert!(!is_valid_branch_name("/etc/passwd"));
    }

    #[test]
    fn test_is_valid_branch_name_rejects_trailing_slash() {
        assert!(!is_valid_branch_name("branch/"));
        assert!(!is_valid_branch_name("feature/test/"));
    }

    #[test]
    fn test_is_valid_branch_name_rejects_special_characters() {
        // Spaces
        assert!(!is_valid_branch_name("has spaces"));

        // Shell injection characters
        assert!(!is_valid_branch_name("branch;rm -rf"));
        assert!(!is_valid_branch_name("branch|cat"));
        assert!(!is_valid_branch_name("branch&echo"));
        assert!(!is_valid_branch_name("branch`whoami`"));
        assert!(!is_valid_branch_name("branch$(pwd)"));

        // Other special characters
        assert!(!is_valid_branch_name("branch*"));
        assert!(!is_valid_branch_name("branch?"));
        assert!(!is_valid_branch_name("branch<file"));
        assert!(!is_valid_branch_name("branch>file"));
    }

    #[test]
    fn test_is_valid_branch_name_rejects_too_long() {
        let long_name = "a".repeat(256);
        assert!(!is_valid_branch_name(&long_name));

        // 255 is valid
        let max_name = "a".repeat(255);
        assert!(is_valid_branch_name(&max_name));
    }

    #[test]
    fn test_is_confirmation_accepted_yes() {
        assert!(is_confirmation_accepted("y"));
        assert!(is_confirmation_accepted("Y"));
        assert!(is_confirmation_accepted("yes"));
        assert!(is_confirmation_accepted("YES"));
        assert!(is_confirmation_accepted("Yes"));
        assert!(is_confirmation_accepted("yEs"));
    }

    #[test]
    fn test_is_confirmation_accepted_no() {
        assert!(!is_confirmation_accepted("n"));
        assert!(!is_confirmation_accepted("N"));
        assert!(!is_confirmation_accepted("no"));
        assert!(!is_confirmation_accepted("NO"));
        assert!(!is_confirmation_accepted(""));
        assert!(!is_confirmation_accepted("yess"));
        assert!(!is_confirmation_accepted("yeah"));
        assert!(!is_confirmation_accepted("nope"));
    }

    #[test]
    fn test_is_confirmation_accepted_with_whitespace() {
        assert!(is_confirmation_accepted("  y  "));
        assert!(is_confirmation_accepted("\ty\n"));
        assert!(is_confirmation_accepted("  yes  "));
        assert!(is_confirmation_accepted("\n\nyes\n"));
        assert!(!is_confirmation_accepted("  n  "));
        assert!(!is_confirmation_accepted("  "));
    }

    #[test]
    fn test_format_partial_failure_error_destroy() {
        let error = format_partial_failure_error("destroy", 2, 5);
        assert_eq!(error, "Partial failure: 2 of 5 kild(s) failed to destroy");
    }

    #[test]
    fn test_format_partial_failure_error_all_failed() {
        let error = format_partial_failure_error("destroy", 3, 3);
        assert_eq!(error, "Partial failure: 3 of 3 kild(s) failed to destroy");
    }

    #[test]
    fn test_format_partial_failure_error_one_failed() {
        let error = format_partial_failure_error("destroy", 1, 10);
        assert_eq!(error, "Partial failure: 1 of 10 kild(s) failed to destroy");
    }

    #[test]
    fn test_format_partial_failure_error_other_operations() {
        // Verify the helper works for other operations too
        let stop_error = format_partial_failure_error("stop", 1, 3);
        assert_eq!(stop_error, "Partial failure: 1 of 3 kild(s) failed to stop");

        let open_error = format_partial_failure_error("open", 2, 4);
        assert_eq!(open_error, "Partial failure: 2 of 4 kild(s) failed to open");
    }

    fn config_with_daemon_enabled() -> KildConfig {
        let mut value = serde_json::to_value(KildConfig::default()).unwrap();
        value["daemon"]["enabled"] = serde_json::Value::Bool(true);
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn test_resolve_runtime_mode_daemon_flag_wins() {
        let config = KildConfig::default();
        let mode = resolve_runtime_mode(true, false, &config);
        assert!(matches!(mode, kild_core::RuntimeMode::Daemon));
    }

    #[test]
    fn test_resolve_runtime_mode_no_daemon_flag_wins() {
        let config = config_with_daemon_enabled();
        let mode = resolve_runtime_mode(false, true, &config);
        assert!(matches!(mode, kild_core::RuntimeMode::Terminal));
    }

    #[test]
    fn test_resolve_runtime_mode_config_enabled() {
        let config = config_with_daemon_enabled();
        let mode = resolve_runtime_mode(false, false, &config);
        assert!(matches!(mode, kild_core::RuntimeMode::Daemon));
    }

    #[test]
    fn test_resolve_runtime_mode_default_terminal() {
        let config = KildConfig::default();
        let mode = resolve_runtime_mode(false, false, &config);
        assert!(matches!(mode, kild_core::RuntimeMode::Terminal));
    }

    #[test]
    fn test_resolve_runtime_mode_both_flags_daemon_wins() {
        let config = KildConfig::default();
        let mode = resolve_runtime_mode(true, true, &config);
        assert!(matches!(mode, kild_core::RuntimeMode::Daemon));
    }
}
