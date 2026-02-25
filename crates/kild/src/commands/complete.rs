use clap::ArgMatches;
use tracing::{error, info};

use kild_core::CompleteRequest;
use kild_core::CompleteResult;
use kild_core::events;
use kild_core::session_ops;

use super::helpers::is_valid_branch_name;

pub(crate) fn handle_complete_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;

    if !is_valid_branch_name(branch) {
        eprintln!("Invalid branch name: {}", branch);
        error!(event = "cli.complete_invalid_branch", branch = branch);
        return Err("Invalid branch name".into());
    }

    // merge-strategy has default_value("squash") in the CLI definition, so get_one always succeeds.
    let strategy_str = matches
        .get_one::<String>("merge-strategy")
        .expect("merge-strategy has a default value");
    let merge_strategy: kild_core::MergeStrategy = strategy_str
        .parse()
        .map_err(|e: String| -> Box<dyn std::error::Error> { e.into() })?;

    let request = CompleteRequest {
        name: branch.clone(),
        merge_strategy,
        no_merge: matches.get_flag("no-merge"),
        force: matches.get_flag("force"),
        dry_run: matches.get_flag("dry-run"),
        skip_ci: matches.get_flag("skip-ci"),
    };

    info!(
        event = "cli.complete_started",
        branch = branch,
        merge_strategy = %request.merge_strategy,
        no_merge = request.no_merge,
        force = request.force,
        dry_run = request.dry_run,
        skip_ci = request.skip_ci,
    );

    match session_ops::complete_session(&request) {
        Ok(result) => {
            match result {
                CompleteResult::Merged {
                    strategy,
                    remote_deleted,
                } => {
                    println!(
                        "Merged '{}' via {} and destroyed session.",
                        branch, strategy
                    );
                    if !remote_deleted {
                        println!("  Warning: remote branch deletion failed.");
                    }
                }
                CompleteResult::AlreadyMerged { remote_deleted } => {
                    println!("Completed '{}'. PR was already merged.", branch);
                    if remote_deleted {
                        println!("  Remote branch deleted.");
                    } else {
                        println!("  Warning: remote branch deletion failed.");
                    }
                }
                CompleteResult::CleanupOnly => {
                    println!("Completed '{}'. Session destroyed.", branch);
                    println!("  PR not merged — remote branch preserved.");
                }
                CompleteResult::DryRun { ref steps } => {
                    println!("Dry run for '{}':", branch);
                    for (i, step) in steps.iter().enumerate() {
                        println!("  {}. {}", i + 1, step);
                    }
                }
            }

            info!(
                event = "cli.complete_completed",
                branch = branch,
                result = ?result
            );

            Ok(())
        }
        Err(e) => {
            eprintln!("{}", e);

            error!(
                event = "cli.complete_failed",
                branch = branch,
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
