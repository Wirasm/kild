use clap::ArgMatches;
use tracing::{error, info};

use kild_core::events;
use kild_core::session_ops;

pub(crate) fn handle_focus_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    info!(event = "cli.focus_started", branch = branch);

    // 1. Look up the session
    let session = match session_ops::get_session(branch) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("No kild found: {}", branch);
            error!(event = "cli.focus_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    // 2. Check for daemon-managed session (no terminal window to focus)
    if session
        .latest_agent()
        .and_then(|a| a.daemon_session_id())
        .is_some()
    {
        eprintln!("Cannot focus '{}': daemon-managed session.", branch);
        eprintln!("  Use 'kild attach {}' to connect.", branch);
        error!(
            event = "cli.focus_failed",
            branch = branch,
            error = "daemon_managed"
        );
        return Err("Cannot focus daemon-managed kild".into());
    }

    // 3. Get terminal type and window ID from latest agent
    let (term_type, window_id) = session
        .latest_agent()
        .map(|latest| {
            (
                latest.terminal_type().cloned(),
                latest.terminal_window_id().map(|s| s.to_string()),
            )
        })
        .unwrap_or((None, None));

    let terminal_type = term_type.ok_or_else(|| {
        eprintln!("No terminal type recorded for '{}'.", branch);
        error!(
            event = "cli.focus_failed",
            branch = branch,
            error = "no_terminal_type"
        );
        "No terminal type recorded for this kild"
    })?;

    let window_id = window_id.ok_or_else(|| {
        eprintln!("No window ID recorded for '{}'.", branch);
        error!(
            event = "cli.focus_failed",
            branch = branch,
            error = "no_window_id"
        );
        "No window ID recorded for this kild"
    })?;

    // 4. Focus the terminal window
    match kild_core::terminal_ops::focus_terminal(&terminal_type, &window_id) {
        Ok(()) => {
            println!("Focused '{}'.", branch);
            info!(event = "cli.focus_completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            eprintln!("Could not focus '{}': {}", branch, e);
            error!(event = "cli.focus_failed", branch = branch, error = %e);
            Err(e.into())
        }
    }
}
