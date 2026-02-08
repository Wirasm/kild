use clap::ArgMatches;
use tracing::{error, info};

use kild_core::events;
use kild_core::session_ops;

use super::helpers::load_config_with_warning;

pub(crate) fn handle_code_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let editor_override = matches.get_one::<String>("editor").cloned();

    info!(
        event = "cli.code_started",
        branch = branch,
        editor_override = ?editor_override
    );

    // 1. Load config
    let config = load_config_with_warning();

    // 2. Look up the session to get worktree path
    let session = match session_ops::get_session(branch) {
        Ok(session) => session,
        Err(e) => {
            eprintln!("❌ Failed to find kild '{}': {}", branch, e);
            error!(event = "cli.code_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    // 3. Open editor via kild-core editor backend
    match kild_core::editor::open_editor(
        &session.worktree_path,
        editor_override.as_deref(),
        &config,
    ) {
        Ok(()) => {
            println!("✅ Opening '{}' in editor", branch);
            println!("   Path: {}", session.worktree_path.display());
            info!(
                event = "cli.code_completed",
                branch = branch,
                worktree_path = %session.worktree_path.display()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to open editor: {}", e);
            eprintln!("   Hint: Make sure the editor is installed and in your PATH");
            error!(
                event = "cli.code_failed",
                branch = branch,
                error = %e
            );
            Err(e.into())
        }
    }
}
