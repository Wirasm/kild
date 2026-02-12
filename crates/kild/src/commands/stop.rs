use clap::ArgMatches;
use tracing::{error, info};

use kild_core::SessionStatus;
use kild_core::events;
use kild_core::session_ops;

use super::helpers::{FailedOperation, format_count, format_partial_failure_error};

pub(crate) fn handle_stop_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    // Check for --all flag first
    if matches.get_flag("all") {
        return handle_stop_all();
    }

    // Single branch operation
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;

    info!(event = "cli.stop_started", branch = branch);

    match session_ops::stop_session(branch) {
        Ok(()) => {
            println!("Stopped. Worktree preserved.");
            println!("  Resume: kild open {}", branch);
            info!(event = "cli.stop_completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            eprintln!("Could not stop '{}': {}", branch, e);
            error!(event = "cli.stop_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Handle `kild stop --all` - stop all running kilds
fn handle_stop_all() -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.stop_all_started");

    let sessions = session_ops::list_sessions()?;
    let active: Vec<_> = sessions
        .into_iter()
        .filter(|s| s.status == SessionStatus::Active)
        .collect();

    if active.is_empty() {
        println!("No running kilds to stop.");
        info!(event = "cli.stop_all_completed", stopped = 0, failed = 0);
        return Ok(());
    }

    let mut stopped: Vec<String> = Vec::new();
    let mut errors: Vec<FailedOperation> = Vec::new();

    for session in active {
        match session_ops::stop_session(&session.branch) {
            Ok(()) => {
                info!(event = "cli.stop_completed", branch = session.branch);
                stopped.push(session.branch);
            }
            Err(e) => {
                error!(
                    event = "cli.stop_failed",
                    branch = session.branch,
                    error = %e
                );
                events::log_app_error(&e);
                errors.push((session.branch, e.to_string()));
            }
        }
    }

    // Report successes
    if !stopped.is_empty() {
        println!("Stopped {}:", format_count(stopped.len()));
        for branch in &stopped {
            println!("  {}", branch);
        }
    }

    // Report failures
    if !errors.is_empty() {
        eprintln!("{} failed to stop:", format_count(errors.len()));
        for (branch, err) in &errors {
            eprintln!("  {}: {}", branch, err);
        }
    }

    info!(
        event = "cli.stop_all_completed",
        stopped = stopped.len(),
        failed = errors.len()
    );

    // Return error if any failures (for exit code)
    if !errors.is_empty() {
        let total_count = stopped.len() + errors.len();
        return Err(format_partial_failure_error("stop", errors.len(), total_count).into());
    }

    Ok(())
}
