use clap::ArgMatches;
use tracing::{error, info, warn};

use kild_core::events;
use kild_core::process;
use kild_core::session_ops;

use unicode_width::UnicodeWidthStr;

use super::helpers::load_config_with_warning;
use super::json_types::EnrichedSession;

pub(crate) fn handle_status_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let json_output = matches.get_flag("json");

    info!(
        event = "cli.status_started",
        branch = branch,
        json_output = json_output
    );

    let config = load_config_with_warning();
    let base_branch = config.git.base_branch();

    match session_ops::get_session(branch) {
        Ok(mut session) => {
            // Sync daemon-managed session: if daemon says stopped, update JSON
            session_ops::sync_daemon_session_status(&mut session);

            let git_stats =
                kild_core::git::collect_git_stats(&session.worktree_path, branch, base_branch);
            let status_info = session_ops::read_agent_status(&session.id);
            let pr_info = session_ops::read_pr_info(&session.id);

            if json_output {
                let terminal_window_title = session
                    .latest_agent()
                    .and_then(|a| a.terminal_window_id().map(|s| s.to_string()));
                let agent_count = session.agent_count();
                let enriched = EnrichedSession {
                    session,
                    git_stats,
                    agent_status: status_info.as_ref().map(|i| i.status.to_string()),
                    agent_status_updated_at: status_info.map(|i| i.updated_at),
                    terminal_window_title,
                    pr_info,
                };
                println!("{}", serde_json::to_string_pretty(&enriched)?);
                info!(
                    event = "cli.status_completed",
                    branch = branch,
                    agent_count = agent_count
                );
                return Ok(());
            }

            // Collect all label-value pairs for dynamic width computation
            let label_width = 13; // "Port Range:  " is the longest label stem
            let mut rows: Vec<(&str, String)> = Vec::new();

            rows.push(("Branch:", session.branch.clone()));
            rows.push(("Status:", format!("{:?}", session.status).to_lowercase()));
            if let Some(ref info) = status_info {
                rows.push(("Activity:", info.status.to_string()));
            }
            rows.push(("Created:", session.created_at.clone()));
            if let Some(ref note) = session.note {
                rows.push(("Note:", note.clone()));
            }
            rows.push(("Worktree:", session.worktree_path.display().to_string()));

            // Git stats rows
            if let Some(ref stats) = git_stats {
                // Show diff vs base as primary changes metric (total branch work)
                if let Some(ref dvb) = stats.diff_vs_base {
                    let changes_line = format!(
                        "+{} -{} ({} files)",
                        dvb.insertions, dvb.deletions, dvb.files_changed
                    );
                    rows.push(("Changes:", changes_line));
                }

                // Show uncommitted details if present
                if let Some(ref ws) = stats.worktree_status
                    && let Some(ref details) = ws.uncommitted_details
                    && !details.is_empty()
                {
                    let uncommitted_line = format!(
                        "{} staged, {} modified, {} untracked",
                        details.staged_files, details.modified_files, details.untracked_files
                    );
                    rows.push(("Uncommitted:", uncommitted_line));
                }

                // Show base-branch drift
                if let Some(ref drift) = stats.drift {
                    let commits_line = format!(
                        "{} ahead, {} behind {}",
                        drift.ahead, drift.behind, drift.base_branch
                    );
                    rows.push(("Commits:", commits_line));
                } else if stats.diff_vs_base.is_none() {
                    rows.push((
                        "Commits:",
                        "(unavailable — run with -v for details)".to_string(),
                    ));
                }

                // Show remote status (push state)
                if let Some(ref ws) = stats.worktree_status {
                    let remote_status = determine_remote_status(ws);
                    rows.push(("Remote:", remote_status.to_string()));
                }
            }

            // PR rows
            if let Some(ref pr) = pr_info {
                rows.push(("PR:", format!("PR #{} ({})", pr.number, pr.state)));
                if let Some(ref ci) = pr.ci_summary {
                    rows.push(("CI:", ci.clone()));
                }
                if let Some(ref reviews) = pr.review_summary {
                    rows.push(("Reviews:", reviews.clone()));
                }
            }

            // Agent rows
            let mut agent_rows: Vec<String> = Vec::new();
            if session.has_agents() {
                rows.push(("Agents:", format!("{} agent(s)", session.agent_count())));
                for (i, agent_proc) in session.agents().iter().enumerate() {
                    let status = agent_proc.process_id().map_or("No PID".to_string(), |pid| {
                        match process::is_process_running(pid) {
                            Ok(true) => format!("Running (PID: {})", pid),
                            Ok(false) => format!("Stopped (PID: {})", pid),
                            Err(e) => {
                                warn!(
                                    event = "cli.status.process_check_failed",
                                    pid = pid,
                                    agent = agent_proc.agent(),
                                    error = %e
                                );
                                format!("Unknown (PID: {})", pid)
                            }
                        }
                    });
                    agent_rows.push(format!("  {}. {:<6} {}", i + 1, agent_proc.agent(), status));
                }
            } else {
                rows.push(("Agent:", session.agent.clone()));
                rows.push(("Process:", "No agents tracked".to_string()));
            }

            // Compute max value width using display width for correct Unicode alignment
            let value_width = rows
                .iter()
                .map(|(_, v)| UnicodeWidthStr::width(v.as_str()))
                .chain(
                    agent_rows
                        .iter()
                        .map(|r| UnicodeWidthStr::width(r.as_str())),
                )
                .max()
                .unwrap_or(0);

            // Box width = "│ " + label_width + value_width + " │"
            let inner_width = label_width + value_width;
            let border = "─".repeat(inner_width + 2);

            println!("📊 KILD Status: {}", branch);
            println!("┌{}┐", border);

            // Print main rows (up to but not including agent detail rows)
            for (label, value) in &rows {
                println!(
                    "│ {:<label_w$}{:<value_w$} │",
                    label,
                    value,
                    label_w = label_width,
                    value_w = value_width,
                );
            }

            // Print agent detail rows
            for row in &agent_rows {
                // Indent agent detail rows under the Agents label
                println!(
                    "│ {:<label_w$}{:<value_w$} │",
                    "",
                    row,
                    label_w = label_width,
                    value_w = value_width,
                );
            }

            println!("└{}┘", border);

            info!(
                event = "cli.status_completed",
                branch = branch,
                agent_count = session.agent_count()
            );

            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to get status for kild '{}': {}", branch, e);

            error!(
                event = "cli.status_failed",
                branch = branch,
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Determine human-readable remote status from worktree status.
fn determine_remote_status(ws: &kild_core::git::types::WorktreeStatus) -> &'static str {
    if !ws.has_remote_branch {
        return "Never pushed";
    }

    if ws.unpushed_commit_count == 0 && ws.behind_commit_count == 0 && !ws.behind_count_failed {
        return "Up to date";
    }

    if ws.unpushed_commit_count == 0 {
        return "Behind remote";
    }

    if ws.behind_commit_count == 0 && !ws.behind_count_failed {
        return "Unpushed changes";
    }

    "Diverged"
}
