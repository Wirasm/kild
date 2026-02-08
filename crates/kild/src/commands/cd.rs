use clap::ArgMatches;
use tracing::{error, info};

use kild_core::events;
use kild_core::session_ops;

use super::helpers::is_valid_branch_name;

pub(crate) fn handle_cd_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    // Validate branch name (no emoji - this command is for shell integration)
    if !is_valid_branch_name(branch) {
        eprintln!("Invalid branch name: {}", branch);
        error!(event = "cli.cd_invalid_branch", branch = branch);
        return Err("Invalid branch name".into());
    }

    info!(event = "cli.cd_started", branch = branch);

    match session_ops::get_session(branch) {
        Ok(session) => {
            // Print only the path - no formatting, no leading text
            // This enables shell integration: cd "$(kild cd branch)"
            println!("{}", session.worktree_path.display());

            info!(
                event = "cli.cd_completed",
                branch = branch,
                path = %session.worktree_path.display()
            );

            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to get path for kild '{}': {}", branch, e);

            error!(
                event = "cli.cd_failed",
                branch = branch,
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
