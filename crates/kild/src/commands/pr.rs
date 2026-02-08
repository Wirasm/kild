use clap::ArgMatches;
use tracing::{error, info, warn};

use kild_core::events;
use kild_core::session_ops;

use super::helpers::is_valid_branch_name;

pub(crate) fn handle_pr_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let json_output = matches.get_flag("json");
    let refresh = matches.get_flag("refresh");

    if !is_valid_branch_name(branch) {
        eprintln!("Invalid branch name: {}", branch);
        error!(event = "cli.pr_invalid_branch", branch = branch);
        return Err("Invalid branch name".into());
    }

    info!(
        event = "cli.pr_started",
        branch = branch,
        json_output = json_output,
        refresh = refresh
    );

    // 1. Look up session
    let session = match session_ops::get_session(branch) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Failed to find kild '{}': {}", branch, e);
            error!(event = "cli.pr_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    // 2. Check for remote
    if !session_ops::has_remote_configured(&session.worktree_path) {
        println!("No remote configured — PR tracking unavailable.");
        info!(
            event = "cli.pr_completed",
            branch = branch,
            result = "no_remote"
        );
        return Ok(());
    }

    let kild_branch = kild_core::git::operations::kild_branch_name(branch);

    // 3. Get PR info: refresh or read from cache
    let pr_info = if refresh || session_ops::read_pr_info(&session.id).is_none() {
        // Fetch from GitHub and write sidecar
        let fetched = session_ops::fetch_pr_info(&session.worktree_path, &kild_branch);
        if let Some(ref info) = fetched {
            let config = kild_core::config::Config::new();
            if let Err(e) = kild_core::sessions::persistence::write_pr_info(
                &config.sessions_dir(),
                &session.id,
                info,
            ) {
                warn!(
                    event = "cli.pr_sidecar_write_failed",
                    branch = branch,
                    error = %e
                );
            }
        }
        fetched
    } else {
        session_ops::read_pr_info(&session.id)
    };

    // 4. Output
    match pr_info {
        Some(info) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                println!("PR #{}: {}", info.number, info.url);
                println!("State:   {}", info.state);
                println!(
                    "CI:      {}",
                    info.ci_summary
                        .as_deref()
                        .unwrap_or(&info.ci_status.to_string())
                );
                println!(
                    "Reviews: {}",
                    info.review_summary
                        .as_deref()
                        .unwrap_or(&info.review_status.to_string())
                );
            }
            info!(
                event = "cli.pr_completed",
                branch = branch,
                pr_number = info.number
            );
        }
        None => {
            println!("No PR found for branch 'kild/{}'", branch);
            info!(
                event = "cli.pr_completed",
                branch = branch,
                result = "no_pr"
            );
        }
    }

    Ok(())
}
