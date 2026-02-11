use clap::ArgMatches;
use tracing::{error, info};

use kild_core::events;
use kild_core::session_ops;

use super::json_types::EnrichedSession;

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

            if json_output {
                let enriched: Vec<EnrichedSession> = sessions
                    .into_iter()
                    .map(|session| {
                        let git_stats = kild_core::git::collect_git_stats(
                            &session.worktree_path,
                            &session.branch,
                        );
                        let status_info = session_ops::read_agent_status(&session.id);
                        let terminal_window_title = session
                            .latest_agent()
                            .and_then(|a| a.terminal_window_id().map(|s| s.to_string()));
                        let pr_info = session_ops::read_pr_info(&session.id);
                        EnrichedSession {
                            session,
                            git_stats,
                            agent_status: status_info.as_ref().map(|i| i.status.to_string()),
                            agent_status_updated_at: status_info.map(|i| i.updated_at),
                            terminal_window_title,
                            pr_info,
                        }
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&enriched)?);
            } else if sessions.is_empty() {
                println!("No active kilds found.");
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
            }

            info!(event = "cli.list_completed", count = session_count);

            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to list kilds: {}", e);

            error!(
                event = "cli.list_failed",
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
