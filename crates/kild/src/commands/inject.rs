use clap::ArgMatches;
use tracing::{error, info, warn};

use kild_core::agents::{InjectMethod, get_inject_method};
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

    // Determine delivery methods that will be attempted.
    use kild_core::sessions::dropbox::DeliveryMethod;
    let delivery_methods: Vec<DeliveryMethod> = match method {
        InjectMethod::ClaudeInbox => vec![DeliveryMethod::Dropbox, DeliveryMethod::ClaudeInbox],
        InjectMethod::Pty => vec![DeliveryMethod::Dropbox, DeliveryMethod::Pty],
    };

    // Write task files to dropbox (fleet mode only — no-op otherwise).
    // Runs before PTY/inbox dispatch so task.md exists when wake-up fires.
    let dropbox_task_id = kild_core::sessions::dropbox::write_task(
        &session.project_id,
        &session.branch,
        text,
        &delivery_methods,
    )
    .unwrap_or_else(|e| {
        eprintln!(
            "{}",
            crate::color::warning(&format!(
                "Warning: Dropbox write failed for '{}': {}",
                branch, e
            ))
        );
        warn!(event = "cli.inject.dropbox_write_failed", branch = branch, error = %e);
        None
    });

    let inbox_name = fleet::fleet_safe_name(branch);
    let result = match method {
        InjectMethod::Pty => write_to_pty(&session, text),
        InjectMethod::ClaudeInbox => {
            fleet::write_to_inbox(fleet::BRAIN_BRANCH, &inbox_name, text).map_err(|e| e.into())
        }
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

    if let Some(task_id) = dropbox_task_id {
        println!(
            "{} task {} to {}",
            crate::color::muted("Wrote"),
            crate::color::aurora(&task_id.to_string()),
            crate::color::ice(&format!("dropbox/{}", branch)),
        );
    }

    println!(
        "{} {} (via {})",
        crate::color::muted("Sent to"),
        crate::color::ice(branch),
        via
    );
    info!(event = "cli.inject_completed", branch = branch, via = via, dropbox_task_id = ?dropbox_task_id);
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
