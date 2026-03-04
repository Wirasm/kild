use clap::ArgMatches;
use tracing::{error, info};

use super::helpers;

pub(crate) fn handle_report_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let _self_flag = matches.get_flag("self");
    let from_hook = matches.get_flag("from-hook");

    if !from_hook {
        return Err("--from-hook is required".into());
    }

    let branch = std::env::var("KILD_SESSION_BRANCH").map_err(
        |_| "KILD_SESSION_BRANCH not set — --self requires running inside a kild session",
    )?;

    info!(event = "cli.report_started", branch = %branch);

    // Read JSON from stdin
    let input = std::io::read_to_string(std::io::stdin()).map_err(|e| {
        error!(event = "cli.report_failed", branch = %branch, error = %e);
        format!("failed to read stdin: {}", e)
    })?;

    // Parse TaskCompleted JSON to extract task info
    let parsed: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();

    let task_subject = parsed["task_subject"].as_str().unwrap_or("(unknown task)");
    let task_description = parsed["task_description"].as_str().unwrap_or("");
    let transcript_summary = parsed["transcript_summary"].as_str().unwrap_or("");

    let report = format!(
        "# Task Completed\n\n**Subject:** {}\n\n{}{}\n",
        task_subject,
        if task_description.is_empty() {
            String::new()
        } else {
            format!("**Description:** {}\n\n", task_description)
        },
        if transcript_summary.is_empty() {
            String::new()
        } else {
            format!("**Summary:** {}\n", transcript_summary)
        },
    );

    let session = helpers::require_session(&branch, "cli.report_failed")?;

    kild_core::sessions::dropbox::write_report(&session.project_id, &session.branch, &report)
        .map_err(|e| {
            error!(event = "cli.report_failed", branch = %branch, error = %e);
            Box::<dyn std::error::Error>::from(e)
        })?;

    info!(event = "cli.report_completed", branch = %branch);
    Ok(())
}
