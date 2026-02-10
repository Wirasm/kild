use clap::ArgMatches;
use tracing::{error, info};

use kild_core::AgentStatus;
use kild_core::session_ops;

pub(crate) fn handle_agent_status_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let use_self = matches.get_flag("self");
    let notify = matches.get_flag("notify");
    let targets: Vec<&String> = matches.get_many::<String>("target").unwrap().collect();

    // Parse branch and status from positional args
    let (branch, status_str) = match (use_self, targets.as_slice()) {
        (true, [status]) => {
            let cwd = std::env::current_dir()?;
            let session = session_ops::find_session_by_worktree_path(&cwd)?.ok_or_else(|| {
                format!(
                    "No kild session found for current directory: {}",
                    cwd.display()
                )
            })?;
            (session.branch, status.as_str())
        }
        (false, [branch, status]) => ((*branch).clone(), status.as_str()),
        (true, _) => return Err("Usage: kild agent-status --self <status>".into()),
        (false, _) => return Err("Usage: kild agent-status <branch> <status>".into()),
    };

    let status: AgentStatus = status_str.parse().map_err(|_| {
        kild_core::sessions::errors::SessionError::InvalidAgentStatus {
            status: status_str.to_string(),
        }
    })?;

    info!(event = "cli.agent_status_started", branch = %branch, status = %status);

    if let Err(e) = session_ops::update_agent_status(&branch, status, notify) {
        error!(event = "cli.agent_status_failed", error = %e);
        return Err(e.into());
    }

    info!(event = "cli.agent_status_completed", branch = %branch, status = %status);
    Ok(())
}
