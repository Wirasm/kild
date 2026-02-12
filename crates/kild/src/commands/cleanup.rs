use clap::ArgMatches;
use tracing::{error, info};

use kild_core::cleanup;
use kild_core::events;

use super::helpers::shorten_home_path;

pub(crate) fn handle_cleanup_command(
    sub_matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.cleanup_started");

    let strategy = if sub_matches.get_flag("no-pid") {
        cleanup::CleanupStrategy::NoPid
    } else if sub_matches.get_flag("stopped") {
        cleanup::CleanupStrategy::Stopped
    } else if let Some(days) = sub_matches.get_one::<u64>("older-than") {
        cleanup::CleanupStrategy::OlderThan(*days)
    } else if sub_matches.get_flag("orphans") {
        cleanup::CleanupStrategy::Orphans
    } else {
        cleanup::CleanupStrategy::All
    };

    match cleanup::cleanup_all_with_strategy(strategy) {
        Ok(summary) => {
            println!("Cleanup complete.");

            if summary.total_cleaned > 0 {
                println!("  Resources cleaned:");

                if !summary.orphaned_branches.is_empty() {
                    println!("  Branches removed: {}", summary.orphaned_branches.len());
                    for branch in &summary.orphaned_branches {
                        println!("    - {}", branch);
                    }
                }

                if !summary.orphaned_worktrees.is_empty() {
                    println!("  Worktrees removed: {}", summary.orphaned_worktrees.len());
                    for worktree in &summary.orphaned_worktrees {
                        println!("    - {}", shorten_home_path(worktree));
                    }
                }

                if !summary.stale_sessions.is_empty() {
                    println!("  Sessions removed: {}", summary.stale_sessions.len());
                    for session in &summary.stale_sessions {
                        println!("    - {}", session);
                    }
                }

                println!("  Total: {} resources cleaned", summary.total_cleaned);
            } else {
                println!("  No orphaned resources found.");
            }

            info!(
                event = "cli.cleanup_completed",
                total_cleaned = summary.total_cleaned
            );

            Ok(())
        }
        Err(cleanup::CleanupError::NoOrphanedResources) => {
            println!("No orphaned resources found.");

            info!(event = "cli.cleanup_completed_no_resources");

            Ok(())
        }
        Err(e) => {
            eprintln!("{}", e);

            error!(
                event = "cli.cleanup_failed",
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
