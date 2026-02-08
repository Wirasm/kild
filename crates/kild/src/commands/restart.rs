use clap::ArgMatches;
use tracing::{error, info, warn};

use kild_core::events;
use kild_core::session_ops;

pub(crate) fn handle_restart_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").unwrap();
    let agent_override = matches.get_one::<String>("agent").cloned();

    eprintln!(
        "⚠️  'restart' is deprecated. Use 'kild stop {}' then 'kild open {}' for similar behavior.",
        branch, branch
    );
    eprintln!(
        "   Note: 'restart' kills the existing process. 'open' is additive (keeps existing terminals)."
    );
    warn!(event = "cli.restart_deprecated", branch = branch);
    info!(event = "cli.restart_started", branch = branch, agent_override = ?agent_override);

    match session_ops::restart_session(branch, agent_override) {
        Ok(session) => {
            println!("✅ KILD '{}' restarted successfully!", branch);
            println!("   Agent: {}", session.agent);
            println!(
                "   Process ID: {:?}",
                session.latest_agent().and_then(|a| a.process_id())
            );
            println!("   Worktree: {}", session.worktree_path.display());
            info!(
                event = "cli.restart_completed",
                branch = branch,
                process_id = session.latest_agent().and_then(|a| a.process_id())
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to restart kild '{}': {}", branch, e);
            error!(event = "cli.restart_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
