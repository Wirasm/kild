/// Fleet mode — Honryū team setup for daemon sessions.
///
/// When a daemon session is created or opened with the "claude" agent, fleet mode
/// injects Claude Code team flags so the inbox poller activates. This allows
/// `kild inject <branch>` to deliver messages via the inbox protocol.
///
/// Fleet mode is opt-in: it activates when the honryu team directory exists
/// (~/.claude/teams/honryu/) or when the brain session itself is being created.
/// Non-claude agents and terminal sessions are unaffected.
use std::path::{Path, PathBuf};

use tracing::warn;

/// Branch name reserved for the Honryū brain session.
pub const BRAIN_BRANCH: &str = "honryu";

/// Team name shared by brain + all workers.
const TEAM_NAME: &str = "honryu";

/// Returns the Claude config base directory, respecting CLAUDE_CONFIG_DIR env var.
fn claude_config_dir() -> PathBuf {
    std::env::var("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .expect("HOME directory not found")
                .join(".claude")
        })
}

fn team_dir() -> PathBuf {
    claude_config_dir().join("teams").join(TEAM_NAME)
}

/// Returns true if fleet mode should apply to a new daemon session.
///
/// Active when the session is the brain itself (creates the team)
/// or when the team directory already exists (brain was created earlier).
fn fleet_mode_active(branch: &str) -> bool {
    branch == BRAIN_BRANCH || team_dir().exists()
}

/// Returns extra args to append to the claude command for fleet mode.
///
/// Returns None if fleet mode does not apply (wrong agent, not active, etc.).
/// The returned string is appended to the existing agent command.
pub fn fleet_agent_flags(branch: &str, agent: &str) -> Option<String> {
    if agent != "claude" || !fleet_mode_active(branch) {
        return None;
    }

    let flags = if branch == BRAIN_BRANCH {
        // Brain loads the kild-brain agent definition and joins as team lead.
        format!(
            "--agent kild-brain --agent-id {BRAIN_BRANCH}@{TEAM_NAME} \
             --agent-name {BRAIN_BRANCH} --team-name {TEAM_NAME}"
        )
    } else {
        format!("--agent-id {branch}@{TEAM_NAME} --agent-name {branch} --team-name {TEAM_NAME}")
    };

    Some(flags)
}

/// Ensure the fleet team directory structure exists for this session.
///
/// Creates:
/// - `~/.claude/teams/honryu/inboxes/<branch>.json` (empty inbox)
/// - `~/.claude/teams/honryu/config.json` (team membership record)
///
/// Idempotent — safe to call on every create/open. Warns on failure,
/// never blocks session creation.
pub fn ensure_fleet_member(branch: &str, cwd: &Path, agent: &str) {
    if agent != "claude" || !fleet_mode_active(branch) {
        return;
    }

    let dir = team_dir();
    let inbox_dir = dir.join("inboxes");

    if let Err(e) = std::fs::create_dir_all(&inbox_dir) {
        warn!(
            event = "core.session.fleet.dir_create_failed",
            branch = branch,
            error = %e,
        );
        return;
    }

    // Create empty inbox if not present.
    let inbox = inbox_dir.join(format!("{branch}.json"));
    if !inbox.exists()
        && let Err(e) = std::fs::write(&inbox, "[]")
    {
        warn!(
            event = "core.session.fleet.inbox_create_failed",
            branch = branch,
            error = %e,
        );
    }

    // Create or update team config.json.
    update_team_config(branch, cwd, &dir);
}

fn update_team_config(branch: &str, cwd: &Path, dir: &Path) {
    let config_path = dir.join("config.json");

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Read existing config or start fresh.
    let mut config: serde_json::Value = if config_path.exists() {
        std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| new_config(now_ms))
    } else {
        new_config(now_ms)
    };

    let members = match config.get_mut("members").and_then(|m| m.as_array_mut()) {
        Some(m) => m,
        None => {
            warn!(
                event = "core.session.fleet.config_malformed",
                branch = branch,
            );
            return;
        }
    };

    let agent_id = if branch == BRAIN_BRANCH {
        format!("{BRAIN_BRANCH}@{TEAM_NAME}")
    } else {
        format!("{branch}@{TEAM_NAME}")
    };

    // Skip if already present.
    if members
        .iter()
        .any(|m| m.get("agentId").and_then(|v| v.as_str()) == Some(&agent_id))
    {
        return;
    }

    let member = if branch == BRAIN_BRANCH {
        serde_json::json!({
            "agentId": agent_id,
            "name": BRAIN_BRANCH,
            "agentType": "team-lead",
            "joinedAt": now_ms,
            "tmuxPaneId": "",
            "cwd": cwd.display().to_string(),
            "subscriptions": []
        })
    } else {
        serde_json::json!({
            "agentId": agent_id,
            "name": branch,
            "agentType": "general-purpose",
            "joinedAt": now_ms,
            "tmuxPaneId": "%1",
            "cwd": cwd.display().to_string(),
            "subscriptions": [],
            "backendType": "tmux",
            "isActive": true
        })
    };

    members.push(member);

    match serde_json::to_string_pretty(&config) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&config_path, json) {
                warn!(
                    event = "core.session.fleet.config_write_failed",
                    branch = branch,
                    error = %e,
                );
            }
        }
        Err(e) => {
            warn!(
                event = "core.session.fleet.config_serialize_failed",
                branch = branch,
                error = %e,
            );
        }
    }
}

fn new_config(now_ms: u64) -> serde_json::Value {
    serde_json::json!({
        "name": TEAM_NAME,
        "description": "Honryū fleet",
        "createdAt": now_ms,
        "leadAgentId": format!("{BRAIN_BRANCH}@{TEAM_NAME}"),
        "leadSessionId": "honryu-brain",
        "members": []
    })
}
