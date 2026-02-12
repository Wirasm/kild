use clap::ArgMatches;
use tracing::{error, info};

use kild_core::events;
use kild_core::session_ops;

use super::helpers::{
    FailedOperation, format_count, format_partial_failure_error, is_confirmation_accepted,
};

pub(crate) fn handle_destroy_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let force = matches.get_flag("force");

    if matches.get_flag("all") {
        return handle_destroy_all(force);
    }

    // Single branch operation
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;

    info!(
        event = "cli.destroy_started",
        branch = branch,
        force = force
    );

    // Pre-destroy safety check (unless --force is specified)
    if !force
        && let Ok(safety_info) = session_ops::get_destroy_safety_info(branch)
        && safety_info.has_warnings()
    {
        let warnings = safety_info.warning_messages();
        for warning in &warnings {
            if safety_info.should_block() {
                eprintln!("Warning: {}", warning);
            } else {
                println!("Warning: {}", warning);
            }
        }

        // Block on uncommitted changes
        if safety_info.should_block() {
            eprintln!();
            eprintln!("Cannot destroy '{}': uncommitted changes.", branch);
            eprintln!("  Inspect first: git -C $(kild cd {}) diff", branch);
            eprintln!(
                "  If you are an agent, do NOT force-destroy without checking the kild first."
            );
            eprintln!("  Use --force to destroy anyway (changes will be lost).");

            error!(
                event = "cli.destroy_blocked",
                branch = branch,
                reason = "uncommitted_changes"
            );

            return Err("Uncommitted changes detected. Use --force to override.".into());
        }
    }

    match session_ops::destroy_session(branch, force) {
        Ok(()) => {
            println!("Destroyed. Branch kild/{} removed.", branch);

            info!(event = "cli.destroy_completed", branch = branch);

            Ok(())
        }
        Err(e) => {
            eprintln!("Could not destroy '{}': {}", branch, e);

            error!(
                event = "cli.destroy_failed",
                branch = branch,
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Handle `kild destroy --all` - destroy all kilds for current project
fn handle_destroy_all(force: bool) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.destroy_all_started", force = force);

    let sessions = session_ops::list_sessions()?;

    if sessions.is_empty() {
        println!("No kilds to destroy.");
        info!(
            event = "cli.destroy_all_completed",
            destroyed = 0,
            failed = 0
        );
        return Ok(());
    }

    // Confirmation prompt unless --force is specified
    if !force {
        use std::io::{self, Write};

        print!(
            "Destroy all {}? Worktrees and sessions will be removed. [y/N] ",
            format_count(sessions.len())
        );
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !is_confirmation_accepted(&input) {
            println!("Aborted.");
            info!(event = "cli.destroy_all_aborted");
            return Ok(());
        }
    }

    let mut destroyed: Vec<String> = Vec::new();
    let mut errors: Vec<FailedOperation> = Vec::new();

    for session in sessions {
        match session_ops::destroy_session(&session.branch, force) {
            Ok(()) => {
                info!(event = "cli.destroy_completed", branch = session.branch);
                destroyed.push(session.branch);
            }
            Err(e) => {
                error!(
                    event = "cli.destroy_failed",
                    branch = session.branch,
                    error = %e
                );
                events::log_app_error(&e);
                errors.push((session.branch, e.to_string()));
            }
        }
    }

    // Report successes
    if !destroyed.is_empty() {
        println!("Destroyed {}:", format_count(destroyed.len()));
        for branch in &destroyed {
            println!("  {}", branch);
        }
    }

    // Report failures
    if !errors.is_empty() {
        eprintln!("{} failed to destroy:", format_count(errors.len()));
        for (branch, err) in &errors {
            eprintln!("  {}: {}", branch, err);
        }
    }

    info!(
        event = "cli.destroy_all_completed",
        destroyed = destroyed.len(),
        failed = errors.len()
    );

    // Return error if any failures (for exit code)
    if !errors.is_empty() {
        let total_count = destroyed.len() + errors.len();
        return Err(format_partial_failure_error("destroy", errors.len(), total_count).into());
    }

    Ok(())
}
