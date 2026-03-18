use clap::ArgMatches;
use serde::Serialize;
use tracing::{error, info};

use kild_core::session_ops;
use kild_core::sessions::inbox::{self, InboxState};

use super::helpers;
use crate::color;

/// JSON output for a single inbox state.
#[derive(Serialize)]
struct InboxOutput {
    branch: String,
    status: String,
    task: Option<String>,
    report: Option<String>,
}

pub(crate) fn handle_inbox_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    if matches.get_flag("all") {
        return handle_all_inbox(matches.get_flag("json"));
    }

    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;
    let json_output = matches.get_flag("json");

    handle_single_inbox(branch, json_output)
}

fn handle_single_inbox(branch: &str, json_output: bool) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.inbox_started", branch = branch);

    let session = helpers::require_session_json(branch, "cli.inbox_failed", json_output)?;
    let state =
        inbox::read_inbox_state_resolved(&session.project_id, &session.branch).map_err(|e| {
            error!(event = "cli.inbox_failed", branch = branch, error = %e);
            Box::<dyn std::error::Error>::from(e)
        })?;

    let state = match state {
        Some(s) => s,
        None => {
            let msg = format!("No fleet inbox for '{}'. Is fleet mode active?", branch);
            if json_output {
                return Err(helpers::print_json_error(&msg, "NO_FLEET_INBOX"));
            }
            eprintln!("{}", msg);
            error!(event = "cli.inbox_no_fleet", branch = branch);
            return Err(msg.into());
        }
    };

    if json_output {
        let output = inbox_output_from_state(&state);
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_single_inbox(&state);
    }

    info!(event = "cli.inbox_completed", branch = branch);
    Ok(())
}

fn handle_all_inbox(json_output: bool) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.inbox_all_started");

    let sessions = session_ops::list_sessions().map_err(|e| {
        error!(event = "cli.inbox_all_failed", error = %e);
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

    let mut states: Vec<InboxState> = Vec::new();
    let mut errors: Vec<(String, String)> = Vec::new();

    for session in &sessions {
        match inbox::read_inbox_state_resolved(&session.project_id, &session.branch) {
            Ok(Some(state)) => states.push(state),
            Ok(None) => {} // non-fleet session, skip
            Err(e) => {
                error!(
                    event = "cli.inbox_read_failed",
                    branch = %session.branch,
                    error = %e,
                );
                errors.push((session.branch.to_string(), e));
            }
        }
    }

    if states.is_empty() {
        if json_output {
            println!("[]");
        } else {
            println!("No fleet sessions found.");
        }
        info!(event = "cli.inbox_all_completed", count = 0);
        return Ok(());
    }

    if json_output {
        let output: Vec<InboxOutput> = states.iter().map(inbox_output_from_state).collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_fleet_inbox_table(&states);
    }

    info!(
        event = "cli.inbox_all_completed",
        count = states.len(),
        failed = errors.len(),
    );

    if !errors.is_empty() {
        eprintln!();
        for (branch, msg) in &errors {
            eprintln!(
                "{} '{}': {}",
                color::error("Inbox read failed for"),
                branch,
                msg,
            );
        }
        let total = states.len() + errors.len();
        return Err(
            helpers::format_partial_failure_error("read inbox", errors.len(), total).into(),
        );
    }

    Ok(())
}

fn inbox_output_from_state(state: &InboxState) -> InboxOutput {
    InboxOutput {
        branch: state.branch.clone(),
        status: state.status.clone(),
        task: state.task.clone(),
        report: state.report.clone(),
    }
}

fn print_single_inbox(state: &InboxState) {
    println!("Status:  {}", color::aurora(&state.status));

    let task_str = state
        .task
        .as_ref()
        .map(|c| first_line(c, 80))
        .unwrap_or_else(|| color::muted("(none)"));
    println!("Task:    {task_str}");

    let report_str = state
        .report
        .as_ref()
        .map(|r| first_line(r, 80))
        .unwrap_or_else(|| color::muted("(none)"));
    println!("Report:  {report_str}");
}

fn print_fleet_inbox_table(states: &[InboxState]) {
    let branch_w = states
        .iter()
        .map(|s| s.branch.len())
        .max()
        .unwrap_or(6)
        .clamp(6, 30);
    let status_w = 10;
    let task_w = 40;
    let report_w = 30;

    println!(
        "┌{}┬{}┬{}┬{}┐",
        "─".repeat(branch_w + 2),
        "─".repeat(status_w + 2),
        "─".repeat(task_w + 2),
        "─".repeat(report_w + 2),
    );
    println!(
        "│ {:<branch_w$} │ {:<status_w$} │ {:<task_w$} │ {:<report_w$} │",
        "Branch", "Status", "Task", "Report",
    );
    println!(
        "├{}┼{}┼{}┼{}┤",
        "─".repeat(branch_w + 2),
        "─".repeat(status_w + 2),
        "─".repeat(task_w + 2),
        "─".repeat(report_w + 2),
    );

    for state in states {
        let task_str = state
            .task
            .as_ref()
            .map(|c| first_line(c, task_w))
            .unwrap_or_else(|| "—".to_string());

        let report_str = state
            .report
            .as_ref()
            .map(|r| first_line(r, report_w))
            .unwrap_or_else(|| "—".to_string());

        println!(
            "│ {:<branch_w$} │ {:<status_w$} │ {:<task_w$} │ {:<report_w$} │",
            truncate_str(&state.branch, branch_w),
            truncate_str(&state.status, status_w),
            truncate_str(&task_str, task_w),
            truncate_str(&report_str, report_w),
        );
    }

    println!(
        "└{}┴{}┴{}┴{}┘",
        "─".repeat(branch_w + 2),
        "─".repeat(status_w + 2),
        "─".repeat(task_w + 2),
        "─".repeat(report_w + 2),
    );
}

fn first_line(text: &str, max_chars: usize) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    truncate_str(line, max_chars)
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
    format!("{truncated}...")
}
