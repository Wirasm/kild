use clap::ArgMatches;
use serde::Serialize;
use tracing::{error, info, warn};

use kild_core::session_ops;
use kild_core::sessions::inbox;

use super::helpers;

/// JSON output shape for `kild prime --json`.
#[derive(Serialize)]
struct PrimeOutput {
    branch: String,
    status: Option<String>,
    task: Option<String>,
    report: Option<String>,
    fleet: Vec<inbox::FleetEntry>,
}

pub(crate) fn handle_prime_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    if matches.get_flag("all") {
        return handle_all_prime(matches.get_flag("json"), matches.get_flag("status"));
    }

    let branch = if matches.get_flag("self") {
        std::env::var("KILD_SESSION_BRANCH").map_err(
            |_| "KILD_SESSION_BRANCH not set — --self requires running inside a kild session",
        )?
    } else {
        matches
            .get_one::<String>("branch")
            .ok_or("Branch argument is required (or use --all / --self)")?
            .clone()
    };
    let json_output = matches.get_flag("json");
    let status_only = matches.get_flag("status");

    handle_single_prime(&branch, json_output, status_only)
}

fn handle_single_prime(
    branch: &str,
    json_output: bool,
    status_only: bool,
) -> Result<(), Box<dyn std::error::Error>> {
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

    let context = inbox::generate_prime_context(&session.project_id, &session.branch, &sessions)
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
        let inbox_state = inbox::read_inbox_state(&session.project_id, &session.branch)
            .map_err(|e| {
                warn!(event = "cli.prime_inbox_read_failed", branch = branch, error = %e);
                e
            })
            .ok()
            .flatten();
        let fleet = inbox::build_fleet_entries_for_json(&session.project_id, &sessions);
        let output = PrimeOutput {
            branch: branch.to_string(),
            status: inbox_state.as_ref().map(|s| s.status.clone()),
            task: inbox_state.as_ref().and_then(|s| s.task.clone()),
            report: inbox_state.as_ref().and_then(|s| s.report.clone()),
            fleet,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if status_only {
        if let Some(table) = inbox::generate_status_table(&session.project_id, &sessions) {
            print!("# Fleet Status: {}\n\n{}", branch, table);
        } else {
            println!("No fleet sessions found.");
        }
    } else {
        print!("{}", context);
    }

    info!(event = "cli.prime_completed", branch = branch);
    Ok(())
}

fn handle_all_prime(
    json_output: bool,
    status_only: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.prime_all_started");

    let sessions = session_ops::list_sessions().map_err(|e| {
        error!(event = "cli.prime_all_failed", error = %e);
        Box::<dyn std::error::Error>::from(e)
    })?;

    if sessions.is_empty() {
        if json_output {
            println!("[]");
        } else {
            println!("No kilds found.");
        }
        return Ok(());
    }

    // Build project sessions once (single-project tool).
    let project_id = sessions
        .first()
        .map(|s| s.project_id.to_string())
        .unwrap_or_default();
    let project_sessions: Vec<_> = sessions
        .iter()
        .filter(|s| s.project_id.as_ref() == project_id)
        .cloned()
        .collect();

    let mut contexts: Vec<(String, String)> = Vec::new(); // (branch, markdown)
    let mut errors: Vec<(String, String)> = Vec::new();

    for session in &project_sessions {
        match inbox::generate_prime_context(&session.project_id, &session.branch, &project_sessions)
        {
            Ok(Some(ctx)) => contexts.push((session.branch.to_string(), ctx)),
            Ok(None) => {} // non-fleet session, skip
            Err(e) => {
                error!(
                    event = "cli.prime_read_failed",
                    branch = %session.branch,
                    error = %e,
                );
                errors.push((session.branch.to_string(), e));
            }
        }
    }

    if contexts.is_empty() {
        if json_output {
            println!("[]");
        } else {
            println!("No fleet sessions found.");
        }
        info!(event = "cli.prime_all_completed", count = 0);
        return Ok(());
    }

    if json_output {
        // Simple JSON array of branch+context pairs
        let output: Vec<serde_json::Value> = contexts
            .iter()
            .map(|(branch, md)| {
                serde_json::json!({
                    "branch": branch,
                    "context": md,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if status_only {
        if let Some(table) = inbox::generate_status_table(&project_id, &project_sessions) {
            print!("# Fleet Status\n\n{}", table);
        }
    } else {
        for (i, (_branch, ctx)) in contexts.iter().enumerate() {
            if i > 0 {
                println!("\n---\n");
            }
            print!("{}", ctx);
        }
    }

    info!(
        event = "cli.prime_all_completed",
        count = contexts.len(),
        failed = errors.len(),
    );

    if !errors.is_empty() {
        eprintln!();
        for (branch, msg) in &errors {
            eprintln!(
                "{} '{}': {}",
                crate::color::error("Prime context failed for"),
                branch,
                msg,
            );
        }
        let total = contexts.len() + errors.len();
        return Err(helpers::format_partial_failure_error(
            "generate prime context",
            errors.len(),
            total,
        )
        .into());
    }

    Ok(())
}
