use clap::ArgMatches;
use tracing::{error, info, warn};

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

    info!(event = "cli.complete_started", branch = branch);

    // Pre-complete safety check (always — complete never bypasses uncommitted check)
    let safety_info = match session_ops::get_destroy_safety_info(branch) {
        Ok(info) => Some(info),
        Err(e) => {
            warn!(
                event = "cli.complete_safety_check_failed",
                branch = branch,
                error = %e
            );
            None
        }
    };

    if let Some(safety_info) = &safety_info {
        if safety_info.has_warnings() {
            let warnings = safety_info.warning_messages();
            for warning in &warnings {
                if safety_info.should_block() {
                    eprintln!("⚠️  {}", warning);
                } else {
                    println!("⚠️  {}", warning);
                }
            }
        }

        if safety_info.should_block() {
            eprintln!();
            eprintln!("❌ Cannot complete '{}' with uncommitted changes.", branch);
            eprintln!("   Use 'kild destroy --force {}' to remove anyway.", branch);

            error!(
                event = "cli.complete_blocked",
                branch = branch,
                reason = "uncommitted_changes"
            );

            return Err(
                "Uncommitted changes detected. Use 'kild destroy --force' to override.".into(),
            );
        }
    }

    match session_ops::complete_session(branch) {
        Ok(result) => {
            use kild_core::CompleteResult;

            println!("✅ KILD '{}' completed!", branch);
            match result {
                CompleteResult::RemoteDeleted => {
                    println!("   Remote branch deleted (PR was merged)");
                }
                CompleteResult::RemoteDeleteFailed => {
                    println!("   Remote branch deletion failed (PR was merged, check logs)");
                }
                CompleteResult::PrNotMerged => {
                    println!("   Remote branch preserved (merge will delete it)");
                }
                CompleteResult::PrCheckUnavailable => {
                    println!("   Could not verify PR merge status — remote branch preserved");
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
            eprintln!("❌ Failed to complete kild '{}': {}", branch, e);

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
