use clap::ArgMatches;
use serde::Serialize;
use tracing::{error, info};

use kild_core::session_ops;
use kild_core::sessions::dropbox::{self, DeliveryMethod, DropboxState};

use super::helpers;
use crate::color;

/// JSON output for a single inbox state.
#[derive(Serialize)]
struct InboxOutput {
    branch: String,
    task_id: Option<u64>,
    ack: Option<u64>,
    acked: bool,
    delivery: Vec<String>,
    task_content: Option<String>,
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

    handle_single_inbox(branch, matches, json_output)
}

fn handle_single_inbox(
    branch: &str,
    matches: &ArgMatches,
    json_output: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.inbox_started", branch = branch);

    let session = helpers::require_session_json(branch, "cli.inbox_failed", json_output)?;
    let state = dropbox::read_dropbox_state(&session.project_id, &session.branch)?;

    let state = match state {
        Some(s) => s,
        None => {
            let msg = format!("No fleet dropbox for '{}'. Is fleet mode active?", branch);
            if json_output {
                return Err(helpers::print_json_error(&msg, "NO_FLEET_DROPBOX"));
            }
            eprintln!("{}", msg);
            error!(event = "cli.inbox_no_fleet", branch = branch);
            return Err(msg.into());
        }
    };

    // Filter flags: --task, --report, --status
    if matches.get_flag("task") {
        match &state.task_content {
            Some(content) => print!("{content}"),
            None => println!("No task assigned."),
        }
        return Ok(());
    }

    if matches.get_flag("report") {
        match &state.report {
            Some(content) => print!("{content}"),
            None => println!("No report yet."),
        }
        return Ok(());
    }

    if matches.get_flag("status") {
        print_status_line(&state);
        println!();
        return Ok(());
    }

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

    let sessions = session_ops::list_sessions()?;

    if sessions.is_empty() {
        if json_output {
            println!("[]");
        } else {
            println!("No kilds found.");
        }
        return Ok(());
    }

    let mut states: Vec<DropboxState> = Vec::new();
    for session in &sessions {
        match dropbox::read_dropbox_state(&session.project_id, &session.branch) {
            Ok(Some(state)) => states.push(state),
            Ok(None) => {} // non-fleet session, skip
            Err(e) => {
                error!(
                    event = "cli.inbox_read_failed",
                    branch = %session.branch,
                    error = %e,
                );
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

    info!(event = "cli.inbox_all_completed", count = states.len());
    Ok(())
}

fn inbox_output_from_state(state: &DropboxState) -> InboxOutput {
    let acked = state.task_id.is_some() && state.task_id == state.ack;
    let delivery = state
        .latest_history
        .as_ref()
        .map(|h| h.delivery.iter().map(delivery_display).collect())
        .unwrap_or_default();

    InboxOutput {
        branch: state.branch.clone(),
        task_id: state.task_id,
        ack: state.ack,
        acked,
        delivery,
        task_content: state.task_content.clone(),
        report: state.report.clone(),
    }
}

fn print_single_inbox(state: &DropboxState) {
    // Task ID line with ack status
    print_status_line(state);
    println!();

    // Delivery
    let delivery_str = state
        .latest_history
        .as_ref()
        .map(|h| {
            h.delivery
                .iter()
                .map(delivery_display)
                .collect::<Vec<_>>()
                .join(" + ")
        })
        .unwrap_or_else(|| color::muted("(unknown)"));
    println!("Delivery: {delivery_str}");

    // Task
    let task_str = state
        .task_content
        .as_ref()
        .map(|c| task_summary(c, 80))
        .unwrap_or_else(|| color::muted("(none)"));
    println!("Task:     {task_str}");

    // Report
    let report_str = state
        .report
        .as_ref()
        .map(|r| first_line(r, 80))
        .unwrap_or_else(|| color::muted("(none)"));
    println!("Report:   {report_str}");
}

fn print_status_line(state: &DropboxState) {
    let task_id_str = state
        .task_id
        .map(|id| format!("{id:>03}"))
        .unwrap_or_else(|| "—".to_string());

    let ack_str = match (state.task_id, state.ack) {
        (Some(tid), Some(ack)) if tid == ack => {
            format!(
                "ack: {} {}",
                color::aurora(&format!("{ack}")),
                color::aurora("✓")
            )
        }
        (Some(_), Some(ack)) => {
            format!(
                "ack: {} {}",
                color::copper(&format!("{ack}")),
                color::copper("✗")
            )
        }
        (Some(_), None) => format!("ack: {}", color::copper("— pending")),
        (None, _) => format!("ack: {}", color::muted("—")),
    };

    print!("Task ID:  {task_id_str} ({ack_str})");
}

fn print_fleet_inbox_table(states: &[DropboxState]) {
    let branch_w = states
        .iter()
        .map(|s| s.branch.len())
        .max()
        .unwrap_or(6)
        .clamp(6, 30);
    let ack_w = 9; // "001 ✓" or "— pend."
    let task_w = 40;
    let report_w = 30;

    // Header
    println!(
        "┌{}┬{}┬{}┬{}┐",
        "─".repeat(branch_w + 2),
        "─".repeat(ack_w + 2),
        "─".repeat(task_w + 2),
        "─".repeat(report_w + 2),
    );
    println!(
        "│ {:<branch_w$} │ {:<ack_w$} │ {:<task_w$} │ {:<report_w$} │",
        "Branch", "Ack", "Task", "Report",
    );
    println!(
        "├{}┼{}┼{}┼{}┤",
        "─".repeat(branch_w + 2),
        "─".repeat(ack_w + 2),
        "─".repeat(task_w + 2),
        "─".repeat(report_w + 2),
    );

    // Rows
    for state in states {
        let ack_str = format_ack_cell(state, ack_w);

        let task_str = state
            .task_content
            .as_ref()
            .map(|c| task_summary(c, task_w))
            .unwrap_or_else(|| "—".to_string());

        let report_str = state
            .report
            .as_ref()
            .map(|r| first_line(r, report_w))
            .unwrap_or_else(|| "—".to_string());

        println!(
            "│ {:<branch_w$} │ {:<ack_w$} │ {:<task_w$} │ {:<report_w$} │",
            truncate_str(&state.branch, branch_w),
            ack_str,
            truncate_str(&task_str, task_w),
            truncate_str(&report_str, report_w),
        );
    }

    // Footer
    println!(
        "└{}┴{}┴{}┴{}┘",
        "─".repeat(branch_w + 2),
        "─".repeat(ack_w + 2),
        "─".repeat(task_w + 2),
        "─".repeat(report_w + 2),
    );
}

/// Format the ack cell for the fleet table: "001 ✓", "001 ✗", "— pend.", or "—".
fn format_ack_cell(state: &DropboxState, _width: usize) -> String {
    match (state.task_id, state.ack) {
        (Some(tid), Some(ack)) if tid == ack => format!("{ack:>03} ✓"),
        (Some(_), Some(ack)) => format!("{ack:>03} ✗"),
        (Some(_), None) => "— pend.".to_string(),
        _ => "—".to_string(),
    }
}

fn delivery_display(method: &DeliveryMethod) -> String {
    match method {
        DeliveryMethod::Dropbox => "dropbox".to_string(),
        DeliveryMethod::ClaudeInbox => "claude_inbox".to_string(),
        DeliveryMethod::Pty => "pty".to_string(),
        DeliveryMethod::InitialPrompt => "initial_prompt".to_string(),
    }
}

/// First non-empty line of text, truncated. Used for report summaries.
fn first_line(text: &str, max_chars: usize) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    truncate_str(line, max_chars)
}

/// Summarize task.md content, skipping the `# Task N` heading that write_task prepends.
fn task_summary(text: &str, max_chars: usize) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty() && !l.starts_with("# Task "))
        .unwrap_or("");
    truncate_str(line, max_chars)
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
    format!("{truncated}...")
}
