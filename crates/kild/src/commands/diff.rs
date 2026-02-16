use clap::ArgMatches;
use tracing::{error, info};

use kild_core::events;
use kild_core::git::get_diff_stats;

use super::helpers;
use super::helpers::shorten_home_path;

pub(crate) fn handle_diff_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let staged = matches.get_flag("staged");
    let stat = matches.get_flag("stat");

    info!(
        event = "cli.diff_started",
        branch = branch,
        staged = staged,
        stat = stat
    );

    // 1. Look up the session
    let session = helpers::require_session(branch, "cli.diff_failed")?;

    // Handle --stat flag: show summary instead of full diff
    if stat {
        let diff = get_diff_stats(&session.worktree_path)?;
        println!(
            "+{} -{} ({} files changed)",
            diff.insertions, diff.deletions, diff.files_changed
        );
        info!(event = "cli.diff_completed", branch = branch, stat = true);
        return Ok(());
    }

    // 2. Execute git diff via kild-core (output appears directly in terminal)
    if let Err(e) = kild_core::git::cli::show_diff(&session.worktree_path, staged) {
        eprintln!("Diff failed: {}", e);
        eprintln!(
            "  Hint: Check that the worktree at {} is a valid git repository.",
            shorten_home_path(&session.worktree_path)
        );
        error!(event = "cli.diff_failed", branch = branch, error = %e);
        events::log_app_error(&e);
        return Err(e.into());
    }

    info!(
        event = "cli.diff_completed",
        branch = branch,
        staged = staged
    );

    Ok(())
}
