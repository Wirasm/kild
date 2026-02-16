use clap::ArgMatches;
use tracing::{error, info};

use kild_core::events;

use super::helpers;

pub(crate) fn handle_commits_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;

    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let count = *matches.get_one::<usize>("count").unwrap_or(&10);

    info!(
        event = "cli.commits_started",
        branch = branch,
        count = count
    );

    let session = helpers::require_session(branch, "cli.commits_failed")?;

    // Run git log in worktree directory via kild-core
    let commits = match kild_core::git::cli::get_commits(&session.worktree_path, count) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            error!(event = "cli.commits_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    // Output commits to stdout, handling broken pipe gracefully
    if let Err(e) = std::io::stdout().write_all(commits.as_bytes())
        && e.kind() != std::io::ErrorKind::BrokenPipe
    {
        eprintln!("Write failed: {}", e);
        error!(
            event = "cli.commits_write_failed",
            branch = branch,
            error = %e
        );
        return Err(format!("Write failed: {}", e).into());
    }

    info!(
        event = "cli.commits_completed",
        branch = branch,
        count = count
    );

    Ok(())
}
