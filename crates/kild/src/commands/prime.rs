use clap::ArgMatches;
use serde::Serialize;
use tracing::{error, info, warn};

use kild_core::session_ops;
use kild_core::sessions::dropbox::{self, FleetEntry, PrimeContext};

use super::helpers;

/// JSON output shape for `kild prime --json`.
///
/// Flattens `PrimeContext.dropbox_state` fields (task_id, task_content, ack, report)
/// to the top level for a flatter JSON schema. If `dropbox_state` is None, these
/// fields are all null. See `prime_output_from_context()` for the mapping.
#[derive(Serialize)]
struct PrimeOutput {
    branch: String,
    protocol: Option<String>,
    task_id: Option<u64>,
    task_content: Option<String>,
    ack: Option<u64>,
    acked: bool,
    report: Option<String>,
    fleet: Vec<FleetEntryOutput>,
}

/// JSON output for a single fleet entry.
#[derive(Serialize)]
struct FleetEntryOutput {
    branch: String,
    agent: String,
    session_status: String,
    agent_status: Option<String>,
    task_id: Option<u64>,
    ack: Option<u64>,
    is_brain: bool,
}

pub(crate) fn handle_prime_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let json_output = matches.get_flag("json");
    let status_only = matches.get_flag("status");

    info!(event = "cli.prime_started", branch = branch);

    let session = helpers::require_session_json(branch, "cli.prime_failed", json_output)?;
    let all_sessions = session_ops::list_sessions().map_err(|e| {
        error!(event = "cli.prime_failed", branch = branch, error = %e);
        Box::<dyn std::error::Error>::from(e)
    })?;
    let sessions: Vec<_> = all_sessions
        .into_iter()
        .filter(|s| s.project_id == session.project_id)
        .collect();

    let context = dropbox::generate_prime_context(&session.project_id, &session.branch, &sessions)
        .map_err(|e| {
            error!(event = "cli.prime_failed", branch = branch, error = %e);
            Box::<dyn std::error::Error>::from(e)
        })?;

    let context = match context {
        Some(ctx) => ctx,
        None => {
            let msg = format!("No fleet context for '{}'. Is fleet mode active?", branch);
            if json_output {
                return Err(helpers::print_json_error(&msg, "NO_FLEET_CONTEXT"));
            }
            eprintln!("{}", msg);
            warn!(event = "cli.prime_no_fleet", branch = branch);
            return Err(msg.into());
        }
    };

    if json_output {
        let output = prime_output_from_context(&context);
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if status_only {
        print!("{}", context.to_status_markdown());
    } else {
        print!("{}", context.to_markdown());
    }

    info!(
        event = "cli.prime_completed",
        branch = branch,
        fleet_count = context.fleet.len(),
    );
    Ok(())
}

fn prime_output_from_context(ctx: &PrimeContext) -> PrimeOutput {
    let (task_id, task_content, ack, report) = if let Some(state) = &ctx.dropbox_state {
        (
            state.task_id,
            state.task_content.clone(),
            state.ack,
            state.report.clone(),
        )
    } else {
        (None, None, None, None)
    };

    let acked = task_id.is_some() && task_id == ack;

    PrimeOutput {
        branch: ctx.branch.to_string(),
        protocol: ctx.protocol.clone(),
        task_id,
        task_content,
        ack,
        acked,
        report,
        fleet: ctx.fleet.iter().map(fleet_entry_output).collect(),
    }
}

fn fleet_entry_output(entry: &FleetEntry) -> FleetEntryOutput {
    FleetEntryOutput {
        branch: entry.branch.to_string(),
        agent: entry.agent.clone(),
        session_status: entry.session_status.to_string(),
        agent_status: entry.agent_status.map(|s| s.to_string()),
        task_id: entry.task_id,
        ack: entry.ack,
        is_brain: entry.is_brain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kild_core::sessions::dropbox::{DropboxState, PrimeContext};
    use kild_core::{AgentStatus, BranchName, SessionStatus};

    #[test]
    fn prime_output_fleet_entries_serializes_correctly() {
        let output = PrimeOutput {
            branch: "worker-a".to_string(),
            protocol: Some("# Protocol".to_string()),
            task_id: Some(3),
            task_content: Some("# Task 3\n\nDo the thing.".to_string()),
            ack: Some(3),
            acked: true,
            report: None,
            fleet: vec![FleetEntryOutput {
                branch: "worker-a".to_string(),
                agent: "claude".to_string(),
                session_status: "active".to_string(),
                agent_status: Some("idle".to_string()),
                task_id: Some(3),
                ack: Some(3),
                is_brain: false,
            }],
        };

        let json = serde_json::to_string_pretty(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["branch"], "worker-a");
        assert_eq!(parsed["task_id"], 3);
        assert_eq!(parsed["acked"], true);
        assert_eq!(parsed["fleet"][0]["agent"], "claude");
        assert_eq!(parsed["fleet"][0]["agent_status"], "idle");
        assert_eq!(parsed["fleet"][0]["is_brain"], false);
    }

    #[test]
    fn prime_output_from_context_maps_fields() {
        let ctx = PrimeContext {
            branch: BranchName::from("worker-a"),
            protocol: Some("protocol text".to_string()),
            dropbox_state: Some(DropboxState {
                branch: BranchName::from("worker-a"),
                task_id: Some(2),
                task_content: Some("task body".to_string()),
                ack: Some(1),
                report: Some("done".to_string()),
                latest_history: None,
            }),
            fleet: vec![FleetEntry {
                branch: BranchName::from("honryu"),
                agent: "claude".to_string(),
                session_status: SessionStatus::Active,
                agent_status: Some(AgentStatus::Working),
                task_id: None,
                ack: None,
                is_brain: true,
            }],
        };

        let output = prime_output_from_context(&ctx);

        assert_eq!(output.branch, "worker-a");
        assert_eq!(output.protocol.as_deref(), Some("protocol text"));
        assert_eq!(output.task_id, Some(2));
        assert_eq!(output.task_content.as_deref(), Some("task body"));
        assert_eq!(output.ack, Some(1));
        assert!(!output.acked, "ack != task_id means not acked");
        assert_eq!(output.report.as_deref(), Some("done"));
        assert_eq!(output.fleet.len(), 1);
        assert_eq!(output.fleet[0].branch, "honryu");
        assert_eq!(output.fleet[0].session_status, "active");
        assert_eq!(output.fleet[0].agent_status.as_deref(), Some("working"));
        assert!(output.fleet[0].is_brain);
    }

    #[test]
    fn prime_output_handles_empty_fleet() {
        let ctx = PrimeContext {
            branch: BranchName::from("worker-a"),
            protocol: None,
            dropbox_state: None,
            fleet: vec![],
        };

        let output = prime_output_from_context(&ctx);

        assert_eq!(output.branch, "worker-a");
        assert!(output.protocol.is_none());
        assert!(output.task_id.is_none());
        assert!(!output.acked, "no task means not acked");
        assert!(output.fleet.is_empty());
    }
}
