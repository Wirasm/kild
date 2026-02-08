use clap::ArgMatches;
use tracing::{error, info, warn};

use kild_core::events;
use kild_core::process;
use kild_core::session_ops;

use crate::table::truncate;

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

    match session_ops::get_session(branch) {
        Ok(session) => {
            let git_stats =
                kild_core::git::operations::collect_git_stats(&session.worktree_path, branch);
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

            // Human-readable table output
            println!("📊 KILD Status: {}", branch);
            println!("┌─────────────────────────────────────────────────────────────┐");
            println!("│ Branch:      {:<47} │", session.branch);
            println!(
                "│ Status:      {:<47} │",
                format!("{:?}", session.status).to_lowercase()
            );
            if let Some(ref info) = status_info {
                println!("│ Activity:    {:<47} │", info.status);
            }
            println!("│ Created:     {:<47} │", session.created_at);
            if let Some(ref note) = session.note {
                println!("│ Note:        {} │", truncate(note, 47));
            }
            println!("│ Worktree:    {:<47} │", session.worktree_path.display());

            // Display git stats
            if let Some(ref stats) = git_stats {
                if let Some(ref diff) = stats.diff_stats {
                    let base = format!(
                        "+{} -{} ({} files)",
                        diff.insertions, diff.deletions, diff.files_changed
                    );

                    let changes_line = match &stats.worktree_status {
                        Some(ws) if ws.uncommitted_details.is_some() => {
                            let details = ws.uncommitted_details.as_ref().unwrap();
                            format!(
                                "{} -- {} staged, {} modified, {} untracked",
                                base,
                                details.staged_files,
                                details.modified_files,
                                details.untracked_files
                            )
                        }
                        _ => base,
                    };

                    println!("│ Changes:     {} │", truncate(&changes_line, 47));
                }

                if let Some(ref ws) = stats.worktree_status {
                    let commits_line = if ws.behind_count_failed {
                        format!(
                            "{} ahead, ? behind (check failed)",
                            ws.unpushed_commit_count
                        )
                    } else {
                        format!(
                            "{} ahead, {} behind",
                            ws.unpushed_commit_count, ws.behind_commit_count
                        )
                    };
                    println!("│ Commits:     {:<47} │", commits_line);
                    let remote_status = if !ws.has_remote_branch {
                        "Never pushed"
                    } else if ws.unpushed_commit_count == 0
                        && ws.behind_commit_count == 0
                        && !ws.behind_count_failed
                    {
                        "Up to date"
                    } else if ws.unpushed_commit_count == 0 {
                        "Behind remote"
                    } else if ws.behind_commit_count == 0 && !ws.behind_count_failed {
                        "Unpushed changes"
                    } else {
                        "Diverged"
                    };
                    println!("│ Remote:      {:<47} │", remote_status);
                }
            }

            // Display PR info (if cached)
            if let Some(ref pr) = pr_info {
                let pr_line = format!("PR #{} ({})", pr.number, pr.state);
                println!("│ PR:          {:<47} │", truncate(&pr_line, 47));
                if let Some(ref ci) = pr.ci_summary {
                    println!("│ CI:          {:<47} │", truncate(ci, 47));
                }
                if let Some(ref reviews) = pr.review_summary {
                    println!("│ Reviews:     {:<47} │", truncate(reviews, 47));
                }
            }

            // Display agents
            if session.has_agents() {
                println!(
                    "│ Agents:      {:<47} │",
                    format!("{} agent(s)", session.agent_count())
                );
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
                    println!("│   {}. {:<6} {:<38} │", i + 1, agent_proc.agent(), status);
                }
            } else {
                println!("│ Agent:       {:<47} │", session.agent);
                println!("│ Process:     {:<47} │", "No agents tracked");
            }

            println!("└─────────────────────────────────────────────────────────────┘");

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
