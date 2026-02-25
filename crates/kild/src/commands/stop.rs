use clap::ArgMatches;
use tracing::{error, info};

use kild_core::SessionStatus;
use kild_core::events;
use kild_core::session_ops;

use super::helpers::{FailedOperation, format_count, format_partial_failure_error};
use crate::color;

pub(crate) fn handle_stop_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(pane_id) = matches.get_one::<String>("pane") {
        let branch = matches
            .get_one::<String>("branch")
            .ok_or("Branch argument is required with --pane")?;
        return handle_stop_teammate(branch, pane_id);
    }

    // Check for --all flag
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
            println!("{}", color::muted("Stopped. Worktree preserved."));
            println!(
                "  {} kild open {}",
                color::muted("Resume:"),
                color::ice(branch)
            );
            info!(event = "cli.stop_completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            eprintln!("{} '{}': {}", color::error("Could not stop"), branch, e);
            error!(event = "cli.stop_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Handle `kild stop <branch> --pane <pane_id>` - stop a single teammate pane
fn handle_stop_teammate(branch: &str, pane_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        event = "cli.stop_teammate_started",
        branch = branch,
        pane_id = pane_id
    );

    match session_ops::stop_teammate(branch, pane_id) {
        Ok(()) => {
            println!(
                "{} Pane {} stopped.",
                color::muted("Teammate"),
                color::ice(pane_id)
            );
            println!(
                "  {} kild attach {} --pane {}",
                color::muted("Reattach:"),
                color::ice(branch),
                pane_id
            );
            info!(
                event = "cli.stop_teammate_completed",
                branch = branch,
                pane_id = pane_id
            );
            Ok(())
        }
        Err(e) => {
            eprintln!(
                "{} pane {} in '{}': {}",
                color::error("Could not stop"),
                pane_id,
                branch,
                e
            );
            error!(
                event = "cli.stop_teammate_failed",
                branch = branch,
                pane_id = pane_id,
                error = %e
            );
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Handle `kild stop --all` - stop all running kilds
fn handle_stop_all() -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.stop_all_started");

    let sessions = session_ops::list_sessions()?;
    let mut active = Vec::new();
    let mut already_stopped = Vec::new();

    for s in sessions {
        match s.status {
            SessionStatus::Active => active.push(s),
            SessionStatus::Stopped => already_stopped.push(s),
            _ => {}
        }
    }

    if active.is_empty() && already_stopped.is_empty() {
        println!("No running kilds to stop.");
        info!(event = "cli.stop_all_completed", stopped = 0, failed = 0);
        return Ok(());
    }

    let mut stopped: Vec<String> = Vec::new();
    let mut errors: Vec<FailedOperation> = Vec::new();

    for session in active {
        match session_ops::stop_session(&session.branch) {
            Ok(()) => {
                info!(event = "cli.stop_completed", branch = %session.branch);
                stopped.push(session.branch.to_string());
            }
            Err(e) => {
                error!(
                    event = "cli.stop_failed",
                    branch = %session.branch,
                    error = %e
                );
                events::log_app_error(&e);
                errors.push((session.branch.to_string(), e.to_string()));
            }
        }
    }

    // Report successes
    if !stopped.is_empty() {
        println!(
            "{}",
            color::muted(&format!("Stopped {}:", format_count(stopped.len())))
        );
        for branch in &stopped {
            println!("  {}", color::ice(branch));
        }
    }

    // Report failures
    if !errors.is_empty() {
        eprintln!(
            "{}",
            color::error(&format!("{} failed to stop:", format_count(errors.len())))
        );
        for (branch, err) in &errors {
            eprintln!("  {}: {}", color::ice(branch), err);
        }
    }

    // Report already-stopped sessions
    if !already_stopped.is_empty() {
        println!(
            "{}",
            color::muted(&format!(
                "Already stopped {}:",
                format_count(already_stopped.len())
            ))
        );
        for session in &already_stopped {
            println!("  {}", color::ice(&session.branch));
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
