use clap::ArgMatches;
use tracing::{error, info, warn};

use kild_core::agents::is_claude_agent;
use kild_core::sessions::fleet;

use super::helpers;

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

    let mut session = helpers::require_session(branch, "cli.inject_failed")?;

    // If the daemon crashed or the socket is gone, update status to Stopped
    // so the active-session check below blocks the inject with a clear message.
    kild_core::session_ops::sync_daemon_session_status(&mut session);

    // Block inject to non-active sessions.
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

    // 1. Write task to file inbox (universal, all agents).
    match kild_core::sessions::inbox::write_task(&session.project_id, &session.branch, text) {
        Ok(Some(())) => {
            info!(event = "cli.inject.inbox_written", branch = branch);
        }
        Ok(None) => {
            // No inbox dir — fleet mode not active. Just proceed with delivery.
        }
        Err(e) => {
            eprintln!(
                "{}",
                crate::color::warning(&format!(
                    "Warning: Inbox write failed for '{}': {}",
                    branch, e
                ))
            );
            warn!(event = "cli.inject.inbox_write_failed", branch = branch, error = %e);
        }
    }

    // 2. Claude fast-path: also write to Claude Code inbox for near-instant delivery.
    let is_claude = force_inbox || is_claude_agent(&session.agent);
    if is_claude {
        let inbox_name = fleet::fleet_safe_name(branch);
        if let Err(e) = fleet::write_to_inbox(fleet::BRAIN_BRANCH, &inbox_name, text) {
            eprintln!("{}", crate::color::error(&format!("Inject failed: {}", e)));
            error!(event = "cli.inject_failed", branch = branch, error = %e);
            return Err(e.into());
        }
    } else {
        // 3. Non-Claude: PTY nudge — write the task text directly to PTY stdin.
        write_to_pty(&session, text)?;
    }

    let via = if is_claude { "inbox" } else { "pty" };

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

    kild_core::daemon::client::write_stdin(daemon_session_id, text.as_bytes())
        .map_err(|e| format!("PTY write failed (text): {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(50));

    kild_core::daemon::client::write_stdin(daemon_session_id, b"\r")
        .map_err(|e| format!("PTY write failed (enter): {}", e).into())
}
