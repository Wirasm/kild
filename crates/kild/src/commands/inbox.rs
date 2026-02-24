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
    let state = dropbox::read_dropbox_state(&session.project_id, &session.branch).map_err(|e| {
        error!(event = "cli.inbox_failed", branch = branch, error = %e);
        let boxed: Box<dyn std::error::Error> = e.into();
        boxed
    })?;

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
        info!(
            event = "cli.inbox_completed",
            branch = branch,
            mode = "task"
        );
        return Ok(());
    }

    if matches.get_flag("report") {
        match &state.report {
            Some(content) => print!("{content}"),
            None => println!("No report yet."),
        }
        info!(
            event = "cli.inbox_completed",
            branch = branch,
            mode = "report"
        );
        return Ok(());
    }

    if matches.get_flag("status") {
        print_status_line(&state);
        println!();
        info!(
            event = "cli.inbox_completed",
            branch = branch,
            mode = "status"
        );
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

    let sessions = session_ops::list_sessions().map_err(|e| {
        error!(event = "cli.inbox_all_failed", error = %e);
        let boxed: Box<dyn std::error::Error> = e.into();
        boxed
    })?;

    if sessions.is_empty() {
        if json_output {
            println!("[]");
        } else {
            println!("No kilds found.");
        }
        return Ok(());
    }

    let mut states: Vec<DropboxState> = Vec::new();
    let mut errors: Vec<(String, String)> = Vec::new();
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
                errors.push((session.branch.to_string(), e.to_string()));
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

fn inbox_output_from_state(state: &DropboxState) -> InboxOutput {
    let acked = state.task_id.is_some() && state.task_id == state.ack;
    let delivery = state
        .latest_history
        .as_ref()
        .map(|h| h.delivery().iter().map(delivery_display).collect())
        .unwrap_or_default();

    InboxOutput {
        branch: state.branch.to_string(),
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
            h.delivery()
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
    if s.chars().count() <= max_len {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
    format!("{truncated}...")
}

#[cfg(test)]
mod tests {
    use super::*;
    use kild_core::BranchName;

    fn make_state(task_id: Option<u64>, ack: Option<u64>) -> DropboxState {
        DropboxState {
            branch: BranchName::from("test"),
            task_id,
            task_content: None,
            ack,
            report: None,
            latest_history: None,
        }
    }

    #[test]
    fn inbox_output_acked_true_only_when_ids_match() {
        assert!(!inbox_output_from_state(&make_state(None, None)).acked);
        assert!(!inbox_output_from_state(&make_state(Some(1), None)).acked);
        assert!(inbox_output_from_state(&make_state(Some(1), Some(1))).acked);
        assert!(!inbox_output_from_state(&make_state(Some(2), Some(1))).acked);
    }

    #[test]
    fn task_summary_skips_task_heading_line() {
        assert_eq!(
            task_summary("# Task 3\n\nFix the auth flow.\n", 80),
            "Fix the auth flow."
        );
    }

    #[test]
    fn task_summary_returns_empty_when_only_heading() {
        assert_eq!(task_summary("# Task 1\n", 80), "");
    }

    #[test]
    fn task_summary_truncates_long_body() {
        let text = format!("# Task 1\n\n{}\n", "A".repeat(100));
        let result = task_summary(&text, 40);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 40);
    }

    #[test]
    fn first_line_skips_blank_lines() {
        assert_eq!(
            first_line("\n\nActual content here\nSecond line", 80),
            "Actual content here"
        );
    }

    #[test]
    fn truncate_str_handles_multibyte_chars() {
        // Em dash is 3 bytes but 1 char — should not be truncated at max_len=12
        let s = "Fix — issue";
        assert_eq!(truncate_str(s, 12), "Fix — issue");
        // Should truncate when char count exceeds limit
        assert_eq!(truncate_str("abcdefghij", 7), "abcd...");
    }
}
