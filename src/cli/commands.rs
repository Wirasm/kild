use clap::ArgMatches;
use tracing::{error, info};

use crate::cleanup;
use crate::core::events;
use crate::sessions::{handler as session_handler, types::CreateSessionRequest};

pub fn run_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    events::log_app_startup();

    match matches.subcommand() {
        Some(("create", sub_matches)) => handle_create_command(sub_matches),
        Some(("list", _)) => handle_list_command(),
        Some(("destroy", sub_matches)) => handle_destroy_command(sub_matches),
        Some(("cleanup", _)) => handle_cleanup_command(),
        _ => {
            error!(event = "cli.command_unknown");
            Err("Unknown command".into())
        }
    }
}

fn handle_create_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").unwrap();
    let agent = matches.get_one::<String>("agent").cloned();

    info!(
        event = "cli.create_started",
        branch = branch,
        agent = agent.as_deref().unwrap_or("claude")
    );

    let request = CreateSessionRequest::new(branch.clone(), agent);

    match session_handler::create_session(request) {
        Ok(session) => {
            println!("✅ Shard created successfully!");
            println!("   Branch: {}", session.branch);
            println!("   Agent: {}", session.agent);
            println!("   Worktree: {}", session.worktree_path.display());
            println!("   Status: {:?}", session.status);

            info!(
                event = "cli.create_completed",
                session_id = session.id,
                branch = session.branch
            );

            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to create shard: {}", e);

            error!(
                event = "cli.create_failed",
                branch = branch,
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_list_command() -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.list_started");

    match session_handler::list_sessions() {
        Ok(sessions) => {
            if sessions.is_empty() {
                println!("No active shards found.");
            } else {
                println!("Active shards:");
                println!("┌──────────────────┬─────────┬─────────┬─────────────────────┐");
                println!("│ Branch           │ Agent   │ Status  │ Created             │");
                println!("├──────────────────┼─────────┼─────────┼─────────────────────┤");

                for session in &sessions {
                    println!(
                        "│ {:<16} │ {:<7} │ {:<7} │ {:<19} │",
                        truncate(&session.branch, 16),
                        truncate(&session.agent, 7),
                        format!("{:?}", session.status).to_lowercase(),
                        truncate(&session.created_at, 19)
                    );
                }

                println!("└──────────────────┴─────────┴─────────┴─────────────────────┘");
            }

            info!(event = "cli.list_completed", count = sessions.len());

            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to list shards: {}", e);

            error!(
                event = "cli.list_failed",
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn handle_destroy_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches.get_one::<String>("branch").unwrap();

    info!(event = "cli.destroy_started", branch = branch);

    match session_handler::destroy_session(branch) {
        Ok(()) => {
            println!("✅ Shard '{}' destroyed successfully!", branch);

            info!(event = "cli.destroy_completed", branch = branch);

            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to destroy shard '{}': {}", branch, e);

            error!(
                event = "cli.destroy_failed",
                branch = branch,
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        format!("{:<width$}", s, width = max_len)
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

fn handle_cleanup_command() -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.cleanup_started");

    match cleanup::cleanup_all() {
        Ok(summary) => {
            println!("✅ Cleanup completed successfully!");
            
            if summary.total_cleaned > 0 {
                println!("   Resources cleaned:");
                
                if !summary.orphaned_branches.is_empty() {
                    println!("   📦 Branches removed: {}", summary.orphaned_branches.len());
                    for branch in &summary.orphaned_branches {
                        println!("      - {}", branch);
                    }
                }
                
                if !summary.orphaned_worktrees.is_empty() {
                    println!("   📁 Worktrees removed: {}", summary.orphaned_worktrees.len());
                    for worktree in &summary.orphaned_worktrees {
                        println!("      - {}", worktree.display());
                    }
                }
                
                if !summary.stale_sessions.is_empty() {
                    println!("   📄 Sessions removed: {}", summary.stale_sessions.len());
                    for session in &summary.stale_sessions {
                        println!("      - {}", session);
                    }
                }
                
                println!("   Total: {} resources cleaned", summary.total_cleaned);
            } else {
                println!("   No orphaned resources found.");
            }

            info!(
                event = "cli.cleanup_completed",
                total_cleaned = summary.total_cleaned
            );

            Ok(())
        }
        Err(cleanup::CleanupError::NoOrphanedResources) => {
            println!("✅ No orphaned resources found - repository is clean!");
            
            info!(event = "cli.cleanup_completed_no_resources");
            
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to cleanup resources: {}", e);

            error!(
                event = "cli.cleanup_failed",
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short     ");
        assert_eq!(truncate("this-is-a-very-long-string", 10), "this-is...");
        assert_eq!(truncate("exact", 5), "exact");
    }

    #[test]
    fn test_truncate_edge_cases() {
        assert_eq!(truncate("", 5), "     ");
        assert_eq!(truncate("abc", 3), "abc");
        assert_eq!(truncate("abcd", 3), "...");
    }
}
