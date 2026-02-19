use std::fs;

use chrono::Utc;
use clap::ArgMatches;
use serde_json::json;
use tracing::{error, info};

use super::helpers;

/// Default team name for fleet mode. Will become a config key.
const DEFAULT_TEAM: &str = "honryu";

pub(crate) fn handle_inject_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let text = matches
        .get_one::<String>("text")
        .ok_or("Text argument is required")?;

    info!(event = "cli.inject_started", branch = branch);

    // Validate the session exists.
    let _session = helpers::require_session(branch, "cli.inject_failed")?;

    let team = DEFAULT_TEAM;
    if let Err(e) = write_to_inbox(team, branch, text) {
        eprintln!("{}", crate::color::error(&format!("Inject failed: {}", e)));
        error!(event = "cli.inject_failed", branch = branch, error = %e);
        return Err(e);
    }

    info!(event = "cli.inject_completed", branch = branch, team = team);
    Ok(())
}

/// Write a message to a Claude Code inbox file.
///
/// Claude Code polls `~/.claude/teams/<team>/inboxes/<agent>.json` every 1 second
/// and delivers unread messages as user turns. The session must have been started
/// with `--agent-id <agent>@<team> --agent-name <agent> --team-name <team>`.
pub(crate) fn write_to_inbox(
    team: &str,
    agent: &str,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let base = std::env::var("CLAUDE_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .expect("HOME directory not found")
                .join(".claude")
        });

    let inbox_dir = base.join("teams").join(team).join("inboxes");
    fs::create_dir_all(&inbox_dir)?;

    let inbox_path = inbox_dir.join(format!("{}.json", agent));

    // Read existing messages (preserving history for the session).
    let mut messages: Vec<serde_json::Value> = if inbox_path.exists() {
        let raw = fs::read_to_string(&inbox_path)?;
        serde_json::from_str(&raw).unwrap_or_default()
    } else {
        Vec::new()
    };

    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    messages.push(json!({
        "from": team,
        "text": text,
        "timestamp": timestamp,
        "read": false
    }));

    fs::write(&inbox_path, serde_json::to_string_pretty(&messages)?)?;

    info!(
        event = "cli.inject_completed",
        team = team,
        agent = agent,
        inbox = %inbox_path.display(),
    );
    Ok(())
}
