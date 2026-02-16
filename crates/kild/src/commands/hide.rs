use clap::ArgMatches;
use tracing::{debug, error, info, warn};

use kild_core::SessionStatus;
use kild_core::events;
use kild_core::session_ops;

use super::helpers::{
    self, FailedOperation, format_count, format_partial_failure_error, get_terminal_info, plural,
};

pub(crate) fn handle_hide_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    if matches.get_flag("all") {
        return handle_hide_all();
    }

    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;

    info!(event = "cli.hide_started", branch = branch);

    let session = helpers::require_session(branch, "cli.hide_failed")?;

    // Daemon-managed sessions have no terminal window to hide
    if session
        .latest_agent()
        .and_then(|a| a.daemon_session_id())
        .is_some()
    {
        eprintln!("Cannot hide '{}': daemon-managed session.", branch);
        eprintln!("  Use 'kild attach {}' to connect.", branch);
        error!(
            event = "cli.hide_failed",
            branch = branch,
            error = "daemon_managed"
        );
        return Err("Cannot hide daemon-managed kild".into());
    }

    let (terminal_type, window_id) = match get_terminal_info(&session) {
        Ok(info) => info,
        Err(msg) => {
            eprintln!("{}: {}", msg, branch);
            error!(event = "cli.hide_failed", branch = branch, error = %msg);
            return Err(msg.into());
        }
    };

    match kild_core::terminal_ops::hide_terminal(&terminal_type, &window_id) {
        Ok(()) => {
            println!("Hidden '{}'.", branch);
            info!(event = "cli.hide_completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            eprintln!("Could not hide '{}': {}", branch, e);
            error!(event = "cli.hide_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Handle `kild hide --all` - hide all active kild terminal windows
fn handle_hide_all() -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.hide_all_started");

    let sessions = session_ops::list_sessions()?;
    let active: Vec<_> = sessions
        .into_iter()
        .filter(|s| s.status == SessionStatus::Active)
        .collect();

    if active.is_empty() {
        println!("No kild windows to hide.");
        info!(event = "cli.hide_all_completed", hidden = 0, failed = 0);
        return Ok(());
    }

    let mut hidden: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut errors: Vec<FailedOperation> = Vec::new();

    for session in active {
        // Skip daemon-managed sessions (no terminal window to hide)
        if session
            .latest_agent()
            .and_then(|a| a.daemon_session_id())
            .is_some()
        {
            debug!(event = "cli.hide_skipped_daemon", branch = %session.branch);
            skipped.push(session.branch.to_string());
            continue;
        }

        let (terminal_type, window_id) = match get_terminal_info(&session) {
            Ok(info) => info,
            Err(msg) => {
                warn!(
                    event = "cli.hide_skipped",
                    branch = %session.branch,
                    reason = %msg
                );
                errors.push((session.branch.to_string(), msg));
                continue;
            }
        };

        match kild_core::terminal_ops::hide_terminal(&terminal_type, &window_id) {
            Ok(()) => {
                info!(event = "cli.hide_completed", branch = %session.branch);
                hidden.push(session.branch.to_string());
            }
            Err(e) => {
                error!(
                    event = "cli.hide_failed",
                    branch = %session.branch,
                    error = %e
                );
                errors.push((session.branch.to_string(), e.to_string()));
            }
        }
    }

    // Report successes
    if !hidden.is_empty() {
        println!("Hidden {}:", format_count(hidden.len()));
        for branch in &hidden {
            println!("  {}", branch);
        }
    }

    // Report failures
    if !errors.is_empty() {
        eprintln!("{} failed to hide:", format_count(errors.len()));
        for (branch, err) in &errors {
            eprintln!("  {}: {}", branch, err);
        }
    }

    // Report skipped daemon sessions (informational, not an error)
    if !skipped.is_empty() {
        println!(
            "Skipped {} daemon-managed {}.",
            skipped.len(),
            plural(skipped.len())
        );
    }

    info!(
        event = "cli.hide_all_completed",
        hidden = hidden.len(),
        skipped = skipped.len(),
        failed = errors.len()
    );

    // Return error only for actual failures (daemon skips don't count)
    if !errors.is_empty() {
        let total_count = hidden.len() + errors.len();
        return Err(format_partial_failure_error("hide", errors.len(), total_count).into());
    }

    Ok(())
}
