use std::sync::LazyLock;
use std::sync::Mutex;
use sysinfo::{Pid as SysinfoPid, ProcessesToUpdate, System};
use tracing::debug;

use crate::agents;
use crate::process::errors::ProcessError;
use crate::process::types::{Pid, ProcessInfo, ProcessMetrics, ProcessStatus};

// Shared system instance to prevent memory leaks
static SYSTEM: LazyLock<Mutex<System>> = LazyLock::new(|| Mutex::new(System::new()));

/// Check if a process with the given PID is currently running
pub fn is_process_running(pid: u32) -> Result<bool, ProcessError> {
    let mut system = System::new();
    let pid_obj = SysinfoPid::from_u32(pid);
    system.refresh_processes(ProcessesToUpdate::Some(&[pid_obj]), true);
    Ok(system.process(pid_obj).is_some())
}

/// Minimum length required for prefix matching to prevent false positives
/// with short names like "sh", "vi", "go"
const MIN_PREFIX_MATCH_LENGTH: usize = 5;

/// Extract the base name from a path, handling both Unix (/) and Windows (\) separators
fn extract_base_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// Check if a process name matches an expected name
///
/// Uses strict matching to prevent PID reuse attacks:
/// 1. Exact match (most secure)
/// 2. Base name match after stripping paths
/// 3. Prefix match only for names >= 5 characters (to avoid "sh" matching "bash")
///
/// Returns false rather than risk killing the wrong process.
fn process_name_matches(actual_name: &str, expected_name: &str) -> bool {
    // Exact match - most secure
    if actual_name == expected_name {
        return true;
    }

    // Extract base names (strip paths) for comparison
    let actual_base = extract_base_name(actual_name);
    let expected_base = extract_base_name(expected_name);

    // Base name exact match
    if actual_base == expected_base {
        return true;
    }

    // Prefix match: only allow if expected name is long enough to be safe
    // This handles cases like "kiro-cli-chat" matching expected "kiro-cli"
    if expected_base.len() >= MIN_PREFIX_MATCH_LENGTH && actual_base.starts_with(expected_base) {
        debug!(
            "process_name_matches: prefix match - actual='{}', expected='{}'",
            actual_name, expected_name
        );
        return true;
    }

    // Reject rather than risk killing the wrong process
    false
}

/// Kill a process with the given PID, validating it matches expected metadata
pub fn kill_process(
    pid: u32,
    expected_name: Option<&str>,
    expected_start_time: Option<u64>,
) -> Result<(), ProcessError> {
    let mut system = System::new();
    let pid_obj = SysinfoPid::from_u32(pid);
    system.refresh_processes(ProcessesToUpdate::Some(&[pid_obj]), true);

    match system.process(pid_obj) {
        Some(process) => {
            // Validate process identity to prevent PID reuse attacks
            if let Some(name) = expected_name {
                let actual_name = process.name().to_string_lossy().to_string();
                if !process_name_matches(&actual_name, name) {
                    return Err(ProcessError::PidReused {
                        pid,
                        expected: name.to_string(),
                        actual: actual_name,
                    });
                }
            }

            if let Some(start_time) = expected_start_time
                && process.start_time() != start_time
            {
                return Err(ProcessError::PidReused {
                    pid,
                    expected: format!("start_time={}", start_time),
                    actual: format!("start_time={}", process.start_time()),
                });
            }

            if process.kill() {
                Ok(())
            } else {
                Err(ProcessError::KillFailed {
                    pid,
                    message: "Process kill signal failed".to_string(),
                })
            }
        }
        None => Err(ProcessError::NotFound { pid }),
    }
}

/// Get basic information about a process
pub fn get_process_info(pid: u32) -> Result<ProcessInfo, ProcessError> {
    let mut system = System::new();
    let pid_obj = SysinfoPid::from_u32(pid);
    system.refresh_processes(ProcessesToUpdate::Some(&[pid_obj]), true);

    match system.process(pid_obj) {
        Some(process) => Ok(ProcessInfo {
            pid: Pid::from_raw(pid),
            name: process.name().to_string_lossy().to_string(),
            status: ProcessStatus::from(process.status()),
            start_time: process.start_time(),
        }),
        None => Err(ProcessError::NotFound { pid }),
    }
}

/// Get CPU and memory usage metrics for a process
pub fn get_process_metrics(pid: u32) -> Result<ProcessMetrics, ProcessError> {
    let pid_obj = SysinfoPid::from_u32(pid);

    // Use shared system instance to prevent memory leaks
    let mut system = SYSTEM.lock().unwrap();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid_obj]), true);

    match system.process(pid_obj) {
        Some(process) => {
            let memory_bytes = process.memory();

            Ok(ProcessMetrics {
                cpu_usage_percent: process.cpu_usage(),
                memory_usage_bytes: memory_bytes,
            })
        }
        None => Err(ProcessError::NotFound { pid }),
    }
}

/// Generate multiple search patterns for better process matching
///
/// Creates a deduplicated list of search patterns to improve process detection reliability.
/// Includes the original pattern, partial matches (before first dash), and known agent variations.
///
/// # Examples
///
/// ```ignore
/// let patterns = generate_search_patterns("kiro-cli");
/// // Returns: ["kiro-cli", "kiro"] (deduplicated)
///
/// let patterns = generate_search_patterns("simple");
/// // Returns: ["simple"]
/// ```
fn generate_search_patterns(name_pattern: &str) -> Vec<String> {
    let mut patterns = std::collections::HashSet::new();
    patterns.insert(name_pattern.to_string());

    // Add partial matches
    if name_pattern.contains('-') {
        patterns.insert(
            name_pattern
                .split('-')
                .next()
                .unwrap_or(name_pattern)
                .to_string(),
        );
    }

    // Add agent-specific patterns if this is a known agent name or pattern
    if let Some(agent_patterns) = agents::get_process_patterns(name_pattern) {
        debug!(
            event = "core.process.agent_patterns_found",
            name_pattern = name_pattern,
            pattern_count = agent_patterns.len(),
            "Found agent-specific process patterns"
        );
        for pattern in agent_patterns {
            patterns.insert(pattern);
        }
    } else {
        debug!(
            event = "core.process.agent_patterns_not_found",
            name_pattern = name_pattern,
            "No agent-specific patterns found, using generic matching"
        );
    }

    // Also check if this pattern matches known agent process patterns and add the agent name
    for agent_name in agents::valid_agent_names() {
        if let Some(agent_patterns) = agents::get_process_patterns(agent_name)
            && agent_patterns.iter().any(|p| p == name_pattern)
        {
            // The name_pattern is a known process pattern for this agent,
            // so add all patterns for this agent
            for pattern in agent_patterns {
                patterns.insert(pattern);
            }
        }
    }

    patterns.into_iter().collect()
}

/// Check if a command line matches a command pattern
///
/// Uses flexible matching to handle cases where:
/// - The binary path differs (e.g., `/usr/bin/kiro-cli-chat` vs `kiro-cli`)
/// - Arguments may vary
///
/// Returns true if all significant words from the pattern appear in the command line.
/// Returns false for empty or flag-only patterns to prevent matching any command.
fn command_matches(cmd_line: &str, cmd_pattern: &str) -> bool {
    // Extract significant words from the pattern (skip common flags)
    let pattern_words: Vec<&str> = cmd_pattern
        .split_whitespace()
        .filter(|w| !w.starts_with('-') && !w.starts_with("--"))
        .collect();

    // Empty or flag-only patterns should not match anything
    if pattern_words.is_empty() {
        debug!(
            "command_matches: rejecting empty/flag-only pattern '{}' for cmd '{}'",
            cmd_pattern, cmd_line
        );
        return false;
    }

    let cmd_line_lower = cmd_line.to_lowercase();

    // Check if all pattern words appear in the command line
    // Use contains for substring matching to handle path differences
    pattern_words.iter().all(|word| {
        let word_lower = word.to_lowercase();
        // Check for exact word or as part of a path/compound name
        cmd_line_lower.contains(&word_lower)
    })
}

/// Find a process by name, optionally filtering by command line pattern
pub fn find_process_by_name(
    name_pattern: &str,
    command_pattern: Option<&str>,
) -> Result<Option<ProcessInfo>, ProcessError> {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    // Try multiple search strategies
    let search_patterns = generate_search_patterns(name_pattern);

    for (pid, process) in system.processes() {
        let process_name = process.name().to_string_lossy();
        let cmd_line = process
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");

        // Try each search pattern
        let name_matches = search_patterns
            .iter()
            .any(|pattern| process_name.contains(pattern) || cmd_line.contains(pattern));

        if !name_matches {
            continue;
        }

        // If command pattern specified, use flexible matching
        // But if cmd_line is empty (macOS often can't read process args), skip command check
        if let Some(cmd_pattern) = command_pattern {
            if !cmd_line.is_empty() && !command_matches(&cmd_line, cmd_pattern) {
                continue;
            }
            // When cmd_line is empty, we can't verify the command pattern,
            // so we rely on name matching only (already passed above)
            if cmd_line.is_empty() {
                debug!(
                    "find_process_by_name: cmd_line unavailable for PID {}, relying on name match only",
                    pid
                );
            }
        }

        return Ok(Some(ProcessInfo {
            pid: Pid::from_raw(pid.as_u32()),
            name: process_name.to_string(),
            status: ProcessStatus::from(process.status()),
            start_time: process.start_time(),
        }));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn test_is_process_running_with_invalid_pid() {
        let result = is_process_running(999999);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_get_process_info_with_invalid_pid() {
        let result = get_process_info(999999);
        assert!(matches!(
            result,
            Err(ProcessError::NotFound { pid: 999999 })
        ));
    }

    #[test]
    fn test_kill_process_with_invalid_pid() {
        let result = kill_process(999999, None, None);
        assert!(matches!(
            result,
            Err(ProcessError::NotFound { pid: 999999 })
        ));
    }

    #[test]
    fn test_process_lifecycle() {
        let mut child = Command::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn test process");

        let pid = child.id();

        let is_running = is_process_running(pid).expect("Failed to check process");
        assert!(is_running);

        let info = get_process_info(pid).expect("Failed to get process info");
        assert_eq!(info.pid.as_u32(), pid);
        assert!(info.name.contains("sleep"));

        let kill_result = kill_process(pid, Some(&info.name), Some(info.start_time));
        assert!(kill_result.is_ok());

        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn test_find_process_by_name() {
        use std::process::{Command, Stdio};

        // Spawn a test process
        let mut child = Command::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn test process");

        // Give it a moment to start
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Should find it by name
        let result = find_process_by_name("sleep", None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());

        // Clean up
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn test_find_process_by_name_not_found() {
        let result = find_process_by_name("nonexistent-process-xyz", None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_generate_search_patterns() {
        // kiro-cli is a known agent process pattern, so it should include kiro patterns
        let patterns = generate_search_patterns("kiro-cli");
        assert!(patterns.contains(&"kiro-cli".to_string()));
        assert!(patterns.contains(&"kiro".to_string()));

        // claude-code is a known agent process pattern
        let patterns = generate_search_patterns("claude-code");
        assert!(patterns.contains(&"claude-code".to_string()));
        assert!(patterns.contains(&"claude".to_string()));

        // "claude" is a known agent, so it should include all claude patterns
        let patterns = generate_search_patterns("claude");
        assert!(patterns.contains(&"claude".to_string()));
        assert!(patterns.contains(&"claude-code".to_string()));

        // "kiro" is a known agent
        let patterns = generate_search_patterns("kiro");
        assert!(patterns.contains(&"kiro".to_string()));
        assert!(patterns.contains(&"kiro-cli".to_string()));

        let patterns = generate_search_patterns("simple");
        assert_eq!(patterns.len(), 1);
        assert!(patterns.contains(&"simple".to_string()));

        // Edge cases
        let patterns = generate_search_patterns("");
        assert!(patterns.contains(&"".to_string()));

        let patterns = generate_search_patterns("no-match-agent");
        assert!(patterns.contains(&"no-match-agent".to_string()));
        assert!(patterns.contains(&"no".to_string()));
        assert_eq!(patterns.len(), 2);

        let patterns = generate_search_patterns("very-long-agent-name-with-many-dashes");
        assert!(patterns.contains(&"very-long-agent-name-with-many-dashes".to_string()));
        assert!(patterns.contains(&"very".to_string()));
        assert_eq!(patterns.len(), 2);
    }

    #[test]
    fn test_find_process_by_name_with_partial_match() {
        // This would need a running process to test properly
        // For now, just ensure the function doesn't panic
        let result = find_process_by_name("nonexistent", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_command_matches() {
        // Exact match
        assert!(command_matches("kiro-cli chat", "kiro-cli chat"));

        // Match with full path
        assert!(command_matches(
            "/Users/rasmus/.local/bin/kiro-cli-chat chat",
            "kiro-cli chat"
        ));
        assert!(command_matches(
            "/usr/bin/kiro-cli-chat chat --flag",
            "kiro-cli chat"
        ));

        // Match with different binary name containing the pattern
        assert!(command_matches("kiro-cli-chat chat", "kiro chat"));

        // Flags should be ignored in pattern
        assert!(command_matches("claude --verbose", "claude --yolo"));
        assert!(command_matches("claude", "claude --trust-all"));

        // Case insensitive
        assert!(command_matches("Claude Chat", "claude chat"));

        // Non-match
        assert!(!command_matches("gemini chat", "kiro chat"));
        assert!(!command_matches("/usr/bin/vim", "kiro-cli chat"));
    }

    #[test]
    fn test_command_matches_empty_patterns() {
        // Empty patterns should NOT match anything (security fix)
        assert!(!command_matches("any command", ""));
        assert!(!command_matches("claude chat", "   "));

        // Flag-only patterns should NOT match anything
        assert!(!command_matches("any command", "--flag"));
        assert!(!command_matches("kiro-cli chat", "--verbose --debug"));
        assert!(!command_matches("claude", "-v -x"));
    }

    #[test]
    fn test_process_name_matches() {
        // Exact match
        assert!(process_name_matches("kiro-cli-chat", "kiro-cli-chat"));

        // Prefix match (actual starts with expected, expected >= 5 chars)
        assert!(process_name_matches("kiro-cli-chat", "kiro-cli"));
        assert!(process_name_matches("claude-code-agent", "claude-code"));

        // Base name comparison with paths
        assert!(process_name_matches("/usr/bin/sleep", "sleep"));
        assert!(process_name_matches("sleep", "/usr/bin/sleep"));

        // Non-match
        assert!(!process_name_matches("gemini", "kiro"));
        assert!(!process_name_matches("claude-code", "kiro-cli"));
    }

    #[test]
    fn test_process_name_matches_security() {
        // Short patterns should NOT match via prefix (prevents "sh" matching "bash")
        // "kiro" is 4 chars, less than MIN_PREFIX_MATCH_LENGTH (5)
        assert!(!process_name_matches("kiro-cli-chat", "kiro"));

        // Short names that could match many processes
        assert!(!process_name_matches("bash", "sh"));
        assert!(!process_name_matches("fish", "fi"));
        assert!(!process_name_matches("vim", "vi"));

        // Reverse direction (expected contains actual) is NOT supported
        assert!(!process_name_matches("kiro", "kiro-cli-chat"));
        assert!(!process_name_matches("sh", "bash"));

        // Arbitrary substring matching is NOT supported
        assert!(!process_name_matches("my-kiro-daemon", "kiro"));
    }

    #[test]
    fn test_process_name_matches_windows_paths() {
        // Windows-style paths should also work
        assert!(process_name_matches(
            "C:\\Program Files\\app\\sleep.exe",
            "sleep.exe"
        ));
        assert!(process_name_matches(
            "sleep.exe",
            "C:\\Windows\\System32\\sleep.exe"
        ));

        // Mixed path separators
        assert!(process_name_matches("C:\\bin/sleep", "sleep"));
    }

    #[test]
    fn test_extract_base_name() {
        // Unix paths
        assert_eq!(extract_base_name("/usr/bin/sleep"), "sleep");
        assert_eq!(
            extract_base_name("/home/user/.local/bin/kiro-cli"),
            "kiro-cli"
        );

        // Windows paths
        assert_eq!(
            extract_base_name("C:\\Program Files\\app\\test.exe"),
            "test.exe"
        );
        assert_eq!(extract_base_name("D:\\bin\\tool.exe"), "tool.exe");

        // No path
        assert_eq!(extract_base_name("simple"), "simple");

        // Empty string
        assert_eq!(extract_base_name(""), "");
    }

    #[test]
    fn test_kill_process_rejects_mismatched_name() {
        // Spawn a test process
        let mut child = Command::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn test process");

        let pid = child.id();

        // Trying to kill with wrong expected name should fail
        let result = kill_process(pid, Some("definitely-not-sleep"), None);
        assert!(matches!(
            result,
            Err(ProcessError::PidReused {
                pid: _,
                expected: _,
                actual: _
            })
        ));

        // Clean up
        let _ = child.kill();
        let _ = child.wait();
    }
}
