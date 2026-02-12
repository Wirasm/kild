use clap::ArgMatches;
use tracing::{error, info};

use kild_core::AgentStatus;
use kild_core::errors::KildError;
use kild_core::session_ops;

pub(crate) fn handle_agent_status_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let use_self = matches.get_flag("self");
    let notify = matches.get_flag("notify");
    let json_output = matches.get_flag("json");
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

    let status: AgentStatus = match status_str.parse() {
        Ok(s) => s,
        Err(_) => {
            let e = kild_core::sessions::errors::SessionError::InvalidAgentStatus {
                status: status_str.to_string(),
            };
            if json_output {
                return Err(super::helpers::print_json_error(&e, e.error_code()));
            }
            return Err(e.into());
        }
    };

    info!(event = "cli.agent_status_started", branch = %branch, status = %status);

    match session_ops::update_agent_status(&branch, status, notify) {
        Ok(result) => {
            if json_output {
                let response = super::json_types::AgentStatusResponse {
                    branch: result.branch,
                    status: result.status.to_string(),
                    updated_at: result.updated_at,
                };
                println!("{}", serde_json::to_string_pretty(&response)?);
            }
            info!(event = "cli.agent_status_completed", branch = %branch, status = %status);
            Ok(())
        }
        Err(e) => {
            error!(event = "cli.agent_status_failed", error = %e);
            if json_output {
                return Err(super::helpers::print_json_error(&e, e.error_code()));
            }
            Err(e.into())
        }
    }
}
