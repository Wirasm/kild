use std::collections::HashSet;

use clap::ArgMatches;
use tracing::{error, info, warn};

use kild_core::errors::KildError;
use kild_core::events;
use kild_core::session_ops;

use super::json_types::{EnrichedSession, FleetSummary, ListOutput};

pub(crate) fn handle_list_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let json_output = matches.get_flag("json");

    info!(event = "cli.list_started", json_output = json_output);

    match session_ops::list_sessions() {
        Ok(mut sessions) => {
            // Sync daemon-managed sessions: if daemon says stopped, update JSON
            for session in &mut sessions {
                session_ops::sync_daemon_session_status(session);
            }

            let session_count = sessions.len();

            if sessions.is_empty() && !json_output {
                println!("No active kilds found.");
                info!(event = "cli.list_completed", count = session_count);
                return Ok(());
            }

            // Shared: load config and compute overlaps for both output paths
            let config = super::helpers::load_config_with_warning();
            let base_branch = config.git.base_branch();

            let (overlap_report, overlap_errors) =
                kild_core::git::collect_file_overlaps(&sessions, base_branch);
            for (branch, err_msg) in &overlap_errors {
                warn!(
                    event = "cli.list.overlap_detection_failed",
                    branch = branch,
                    error = err_msg
                );
            }

            // Count kilds with conflicts
            let kilds_with_conflicts: HashSet<&str> = overlap_report
                .overlapping_files
                .iter()
                .flat_map(|fo| fo.branches.iter().map(|s| s.as_str()))
                .collect();
            let conflict_count = kilds_with_conflicts.len();

            if json_output {
                let enriched: Vec<EnrichedSession> = sessions
                    .into_iter()
                    .map(|session| {
                        let git_stats = kild_core::git::collect_git_stats(
                            &session.worktree_path,
                            &session.branch,
                            base_branch,
                        );
                        let process_status =
                            kild_core::sessions::info::determine_process_status(&session);
                        let branch_health = kild_core::git::collect_branch_health(
                            &session.worktree_path,
                            &session.branch,
                            base_branch,
                            &session.created_at,
                        )
                        .ok();
                        let status_info = session_ops::read_agent_status(&session.id);
                        let pr_info = session_ops::read_pr_info(&session.id);

                        let latest_agent = session.latest_agent();
                        let terminal_window_title =
                            latest_agent.and_then(|a| a.terminal_window_id().map(str::to_string));
                        let terminal_type =
                            latest_agent.and_then(|a| a.terminal_type().map(|t| t.to_string()));

                        let worktree_status_ref =
                            &git_stats.as_ref().and_then(|g| g.worktree_status.clone());
                        let merge_readiness = branch_health.as_ref().map(|h| {
                            kild_core::MergeReadiness::compute(
                                h,
                                worktree_status_ref,
                                pr_info.as_ref(),
                            )
                        });

                        let overlapping_files = Some(
                            overlap_report
                                .overlapping_files
                                .iter()
                                .filter(|fo| fo.branches.iter().any(|b| b == &*session.branch))
                                .map(|fo| fo.file.display().to_string())
                                .collect(),
                        );

                        EnrichedSession {
                            session,
                            process_status,
                            git_stats,
                            branch_health,
                            merge_readiness,
                            agent_status: status_info.as_ref().map(|i| i.status.to_string()),
                            agent_status_updated_at: status_info.map(|i| i.updated_at),
                            terminal_window_title,
                            terminal_type,
                            pr_info,
                            overlapping_files,
                        }
                    })
                    .collect();

                let output = ListOutput::new(enriched, &kilds_with_conflicts);
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!("Active kilds:");
                // Read sidecar statuses for table display
                let statuses: Vec<Option<kild_core::sessions::types::AgentStatusInfo>> = sessions
                    .iter()
                    .map(|s| session_ops::read_agent_status(&s.id))
                    .collect();
                let pr_infos: Vec<Option<kild_core::PrInfo>> = sessions
                    .iter()
                    .map(|s| session_ops::read_pr_info(&s.id))
                    .collect();
                let formatter = crate::table::TableFormatter::new(&sessions, &statuses, &pr_infos);
                formatter.print_table(&sessions, &statuses, &pr_infos);

                // Collect git stats once for fleet summary
                let git_stats: Vec<Option<kild_core::GitStats>> = sessions
                    .iter()
                    .map(|s| {
                        kild_core::git::collect_git_stats(&s.worktree_path, &s.branch, base_branch)
                    })
                    .collect();

                let summary = FleetSummary::from_sessions(&sessions, &git_stats, conflict_count);

                println!();
                println!(
                    "{} kilds: {} active, {} stopped | {} conflicts | {} needs push",
                    summary.total,
                    summary.active,
                    summary.stopped,
                    summary.conflicts,
                    summary.needs_push,
                );
            }

            info!(event = "cli.list_completed", count = session_count);

            Ok(())
        }
        Err(e) => {
            error!(event = "cli.list_failed", error = %e);
            events::log_app_error(&e);

            if json_output {
                return Err(super::helpers::print_json_error(&e, e.error_code()));
            }

            eprintln!("{}", e);
            Err(e.into())
        }
    }
}
