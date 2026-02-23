use std::fs;
use std::fs::OpenOptions;

use chrono::Utc;
use clap::ArgMatches;
use nix::fcntl::{Flock, FlockArg};
use serde_json::json;
use tracing::{error, info};

use kild_core::agents::{InjectMethod, get_inject_method};

use super::helpers;

/// Default team name for fleet mode.
const DEFAULT_TEAM: &str = "honryu";

pub(crate) fn handle_inject_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let text = matches
        .get_one::<String>("text")
        .ok_or("Text argument is required")?;
    let force_inbox = matches.get_flag("inbox");

    // Reject empty text — it produces a no-op inbox message or blank PTY input.
    if text.trim().is_empty() {
        eprintln!("{}", crate::color::error("Inject text cannot be empty."));
        return Err("Inject text cannot be empty".into());
    }

    info!(event = "cli.inject_started", branch = branch);

    let session = helpers::require_session(branch, "cli.inject_failed")?;

    // Determine inject method: --inbox forces inbox protocol; otherwise use agent default.
    let method = if force_inbox {
        if get_inject_method(&session.agent) != InjectMethod::ClaudeInbox {
            eprintln!(
                "Warning: --inbox is only meaningful for claude sessions; \
                 session '{}' uses agent '{}'. Forcing inbox anyway.",
                branch, session.agent
            );
        }
        InjectMethod::ClaudeInbox
    } else {
        get_inject_method(&session.agent)
    };

    // Block inject to non-active sessions. Inbox writes would queue with nobody polling;
    // PTY writes would fail with a confusing "no daemon PTY" error. Provide a clear,
    // actionable message for both paths.
    if session.status != kild_core::SessionStatus::Active {
        let msg = format!(
            "Session '{}' is {:?} — cannot inject. \
             Start the session first with `kild open {}`.",
            branch, session.status, branch
        );
        eprintln!("{}", crate::color::error(&msg));
        error!(
            event = "cli.inject_failed",
            branch = branch,
            reason = "session_not_active"
        );
        return Err(msg.into());
    }

    let result = match method {
        InjectMethod::Pty => write_to_pty(&session, text),
        InjectMethod::ClaudeInbox => write_to_inbox(DEFAULT_TEAM, branch, text),
    };

    if let Err(e) = result {
        eprintln!("{}", crate::color::error(&format!("Inject failed: {}", e)));
        error!(event = "cli.inject_failed", branch = branch, error = %e);
        return Err(e);
    }

    let via = match method {
        InjectMethod::ClaudeInbox => "inbox",
        InjectMethod::Pty => "pty",
    };
    println!(
        "{} {} (via {})",
        crate::color::muted("Sent to"),
        crate::color::ice(branch),
        via
    );
    info!(event = "cli.inject_completed", branch = branch, via = via);
    Ok(())
}

/// Write text to the agent's PTY stdin via the daemon WriteStdin IPC.
///
/// Works for all agents. Text is written first, then Enter (\r) after a 50ms pause.
/// PTY stdin is kernel-buffered — the agent reads it when its input handler is ready.
/// This is the universal inject path and works on cold start.
fn write_to_pty(
    session: &kild_core::Session,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let daemon_session_id = session
        .latest_agent()
        .and_then(|a| a.daemon_session_id())
        .ok_or_else(|| {
            format!(
                "Session '{}' has no active daemon PTY. Is it a daemon session? \
                 Use `kild create --daemon` or `kild open --daemon`.",
                session.branch
            )
        })?;

    // Two separate writes: text then Enter (\r), with a brief pause between.
    // TUI agents need the text and Enter in separate read() cycles to correctly
    // submit the input rather than treating \r as a literal character.
    kild_core::daemon::client::write_stdin(daemon_session_id, text.as_bytes())
        .map_err(|e| format!("PTY write failed (text): {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(50));

    kild_core::daemon::client::write_stdin(daemon_session_id, b"\r")
        .map_err(|e| format!("PTY write failed (enter): {}", e).into())
}

/// Write a message to a Claude Code inbox file.
///
/// Claude Code polls `~/.claude/teams/<team>/inboxes/<agent>.json` every 1 second
/// and delivers unread messages as user turns. The session must have been started
/// with `--agent-id <agent>@<team> --agent-name <agent> --team-name <team>`.
///
/// Uses an exclusive flock on `<agent>.lock` to prevent concurrent writes from
/// the hook script (which fires on every worker Stop event) from racing and
/// overwriting each other's messages.
pub(crate) fn write_to_inbox(
    team: &str,
    agent: &str,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let base = std::env::var("CLAUDE_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
        .ok_or("HOME directory not found — cannot locate Claude config directory")?;

    let inbox_dir = base.join("teams").join(team).join("inboxes");
    fs::create_dir_all(&inbox_dir)?;

    let inbox_path = inbox_dir.join(format!("{}.json", agent));
    let lock_path = inbox_dir.join(format!("{}.lock", agent));

    // Acquire exclusive file lock to prevent concurrent hook invocations from
    // racing on the read-modify-write below. Held until _lock drops at end of fn.
    let lock_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;
    let _lock = Flock::lock(lock_file, FlockArg::LockExclusive)
        .map_err(|(_, e)| format!("failed to lock inbox: {}", e))?;

    // Read existing messages (preserving history for the session).
    let mut messages: Vec<serde_json::Value> = if inbox_path.exists() {
        let raw = fs::read_to_string(&inbox_path)
            .map_err(|e| format!("failed to read inbox {}: {}", inbox_path.display(), e))?;
        serde_json::from_str(&raw).map_err(|e| {
            format!(
                "inbox at {} is corrupt ({}). Delete it and retry.",
                inbox_path.display(),
                e
            )
        })?
    } else {
        Vec::new()
    };

    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    messages.push(json!({
        "from": team,
        "text": text,
        "timestamp": timestamp,
        "read": false
    }));

    fs::write(&inbox_path, serde_json::to_string_pretty(&messages)?)
        .map_err(|e| format!("failed to write inbox {}: {}", inbox_path.display(), e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Serialize tests that mutate CLAUDE_CONFIG_DIR — env vars are process-global.
    static INJECT_ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_claude_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("kild_inject_test_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn write_to_inbox_creates_valid_json_message() {
        let _lock = INJECT_ENV_LOCK.lock().unwrap();
        let base = temp_claude_dir("write_basic");
        // SAFETY: INJECT_ENV_LOCK serializes all CLAUDE_CONFIG_DIR mutations in this module.
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &base) };

        write_to_inbox("honryu", "my-worker", "hello from brain").unwrap();

        let inbox = base.join("teams/honryu/inboxes/my-worker.json");
        assert!(inbox.exists());
        let raw = std::fs::read_to_string(&inbox).unwrap();
        let msgs: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["text"], "hello from brain");
        assert_eq!(msgs[0]["from"], "honryu");
        assert_eq!(msgs[0]["read"], false);
        assert!(
            msgs[0]["timestamp"].as_str().is_some(),
            "timestamp should be present"
        );

        let _ = std::fs::remove_dir_all(&base);
        // SAFETY: restoring env; lock still held.
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
    }

    #[test]
    fn write_to_inbox_appends_without_overwriting_existing_messages() {
        let _lock = INJECT_ENV_LOCK.lock().unwrap();
        let base = temp_claude_dir("write_append");
        // SAFETY: INJECT_ENV_LOCK serializes all CLAUDE_CONFIG_DIR mutations in this module.
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &base) };

        write_to_inbox("honryu", "worker", "msg 1").unwrap();
        write_to_inbox("honryu", "worker", "msg 2").unwrap();

        let raw = std::fs::read_to_string(base.join("teams/honryu/inboxes/worker.json")).unwrap();
        let msgs: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();
        assert_eq!(msgs.len(), 2, "both messages should be present");
        assert_eq!(msgs[0]["text"], "msg 1");
        assert_eq!(msgs[1]["text"], "msg 2");

        let _ = std::fs::remove_dir_all(&base);
        // SAFETY: restoring env; lock still held.
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
    }

    #[test]
    fn write_to_inbox_returns_error_on_corrupt_inbox() {
        let _lock = INJECT_ENV_LOCK.lock().unwrap();
        let base = temp_claude_dir("write_corrupt");
        // SAFETY: INJECT_ENV_LOCK serializes all CLAUDE_CONFIG_DIR mutations in this module.
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &base) };
        let inbox_dir = base.join("teams/honryu/inboxes");
        std::fs::create_dir_all(&inbox_dir).unwrap();
        std::fs::write(inbox_dir.join("worker.json"), "not valid json {{").unwrap();

        let result = write_to_inbox("honryu", "worker", "text");
        assert!(result.is_err(), "should error on corrupt inbox");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("corrupt"),
            "error should mention corruption, got: {}",
            msg
        );

        let _ = std::fs::remove_dir_all(&base);
        // SAFETY: restoring env; lock still held.
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
    }
}
