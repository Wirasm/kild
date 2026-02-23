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

use crate::agents::types::AgentType;

/// Branch name reserved for the Honryū brain session.
pub const BRAIN_BRANCH: &str = "honryu";

/// Team name shared by brain + all workers. Intentionally matches BRAIN_BRANCH.
const TEAM_NAME: &str = BRAIN_BRANCH;

/// Returns the Claude config base directory, respecting CLAUDE_CONFIG_DIR env var.
///
/// Returns None when $HOME is unset and CLAUDE_CONFIG_DIR is not set.
fn claude_config_dir() -> Option<PathBuf> {
    std::env::var("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
}

fn team_dir() -> Option<PathBuf> {
    claude_config_dir().map(|d| d.join("teams").join(TEAM_NAME))
}

/// Returns true if the agent supports the fleet inbox protocol.
///
/// Only claude sessions participate in fleet mode; all other agents are unaffected.
fn is_fleet_capable_agent(agent: &str) -> bool {
    AgentType::parse(agent) == Some(AgentType::Claude)
}

/// Returns true if fleet mode should apply to a new daemon session.
///
/// Active when the session is the brain itself (team will be created by ensure_fleet_member)
/// or when the team directory already exists (brain was created earlier).
fn fleet_mode_active(branch: &str) -> bool {
    if branch == BRAIN_BRANCH {
        return true;
    }
    let Some(dir) = team_dir() else {
        warn!(event = "core.session.fleet.home_missing");
        return false;
    };
    match dir.try_exists() {
        Ok(exists) => exists,
        Err(e) => {
            warn!(
                event = "core.session.fleet.dir_check_failed",
                error = %e,
            );
            false
        }
    }
}

/// Returns extra args to append to the claude command for fleet mode.
///
/// Returns None if fleet mode does not apply (wrong agent, not active, etc.).
/// The returned string is appended to the existing agent command.
pub fn fleet_agent_flags(branch: &str, agent: &str) -> Option<String> {
    if !is_fleet_capable_agent(agent) || !fleet_mode_active(branch) {
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
/// Creates (if not already present):
/// - `~/.claude/teams/honryu/inboxes/<branch>.json` (initialized to empty array)
/// - `~/.claude/teams/honryu/config.json` (team membership record; appends member if not listed)
///
/// Idempotent — safe to call on every create/open. Warns on failure,
/// never blocks session creation.
pub fn ensure_fleet_member(branch: &str, cwd: &Path, agent: &str) {
    if !is_fleet_capable_agent(agent) || !fleet_mode_active(branch) {
        return;
    }

    let Some(dir) = team_dir() else {
        warn!(event = "core.session.fleet.home_missing", branch = branch,);
        eprintln!(
            "Warning: Fleet setup skipped for '{}' — HOME not set.",
            branch
        );
        eprintln!("Brain messages will not be delivered to this session.");
        return;
    };
    let inbox_dir = dir.join("inboxes");

    if let Err(e) = std::fs::create_dir_all(&inbox_dir) {
        warn!(
            event = "core.session.fleet.dir_create_failed",
            branch = branch,
            error = %e,
        );
        eprintln!("Warning: Fleet inbox setup failed for '{}': {}", branch, e);
        eprintln!("Brain messages will not be delivered to this session.");
        return;
    }

    // Create empty inbox if not present. Use try_exists to avoid silently returning
    // false on OS errors (which would cause overwriting an existing inbox file).
    let inbox = inbox_dir.join(format!("{branch}.json"));
    match inbox.try_exists() {
        Ok(true) => {} // Already exists — leave it intact (may contain queued messages).
        Ok(false) => {
            if let Err(e) = std::fs::write(&inbox, "[]") {
                warn!(
                    event = "core.session.fleet.inbox_create_failed",
                    branch = branch,
                    error = %e,
                );
                eprintln!(
                    "Warning: Fleet inbox creation failed for '{}': {}",
                    branch, e
                );
                eprintln!("Brain messages will not be delivered to this session.");
            }
        }
        Err(e) => {
            warn!(
                event = "core.session.fleet.inbox_exists_check_failed",
                branch = branch,
                error = %e,
            );
            eprintln!("Warning: Fleet inbox check failed for '{}': {}", branch, e);
            // Do not write — do not risk overwriting an existing inbox on FS error.
        }
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
    // On read or parse error, return immediately rather than falling back to a new config
    // that would silently overwrite all existing team membership data.
    let mut config: serde_json::Value = if config_path.exists() {
        let raw = match std::fs::read_to_string(&config_path) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    event = "core.session.fleet.config_read_failed",
                    branch = branch,
                    error = %e,
                );
                return;
            }
        };
        match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    event = "core.session.fleet.config_parse_failed",
                    branch = branch,
                    error = %e,
                );
                return;
            }
        }
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use std::fs;

    /// Serialize tests that mutate CLAUDE_CONFIG_DIR — env vars are process-global.
    static FLEET_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Create a temp dir with `~/.claude/teams/honryu/` already present and set
    /// `CLAUDE_CONFIG_DIR` to point at it. Calls `f` while holding the env lock.
    fn with_team_dir(test_name: &str, f: impl FnOnce(&std::path::Path)) {
        let _lock = FLEET_ENV_LOCK.lock().unwrap();
        let base = std::env::temp_dir().join(format!(
            "kild_fleet_test_{}_{}",
            test_name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let team_dir = base.join("teams").join(BRAIN_BRANCH);
        fs::create_dir_all(&team_dir).unwrap();
        // SAFETY: FLEET_ENV_LOCK serializes all CLAUDE_CONFIG_DIR mutations in this module.
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &base) };
        f(&base);
        let _ = fs::remove_dir_all(&base);
        // SAFETY: restoring env; lock still held.
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
    }

    /// Create a temp dir WITHOUT the team directory (fleet not yet started).
    fn without_team_dir(test_name: &str, f: impl FnOnce(&std::path::Path)) {
        let _lock = FLEET_ENV_LOCK.lock().unwrap();
        let base = std::env::temp_dir().join(format!(
            "kild_fleet_no_dir_{}_{}",
            test_name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        // SAFETY: FLEET_ENV_LOCK serializes all CLAUDE_CONFIG_DIR mutations in this module.
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &base) };
        f(&base);
        let _ = fs::remove_dir_all(&base);
        // SAFETY: restoring env; lock still held.
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
    }

    // --- fleet_agent_flags ---

    #[test]
    fn fleet_agent_flags_brain_gets_kild_brain_flag() {
        with_team_dir("brain_flag", |_| {
            let flags = fleet_agent_flags(BRAIN_BRANCH, "claude").unwrap();
            assert!(
                flags.contains("--agent kild-brain"),
                "brain should get --agent kild-brain, got: {}",
                flags
            );
            assert!(
                flags.contains(&format!("--team-name {TEAM_NAME}")),
                "brain should get --team-name, got: {}",
                flags
            );
        });
    }

    #[test]
    fn fleet_agent_flags_worker_does_not_get_brain_flag() {
        with_team_dir("worker_no_brain_flag", |_| {
            let flags = fleet_agent_flags("my-feature", "claude").unwrap();
            assert!(
                !flags.contains("--agent kild-brain"),
                "worker should not get --agent kild-brain, got: {}",
                flags
            );
            assert!(
                flags.contains("--agent-id my-feature@honryu"),
                "worker should get --agent-id, got: {}",
                flags
            );
        });
    }

    #[test]
    fn fleet_agent_flags_non_claude_returns_none() {
        with_team_dir("non_claude_none", |_| {
            assert!(fleet_agent_flags("my-feature", "amp").is_none());
            assert!(fleet_agent_flags("my-feature", "codex").is_none());
            assert!(fleet_agent_flags("my-feature", "kiro").is_none());
            assert!(fleet_agent_flags("my-feature", "gemini").is_none());
        });
    }

    #[test]
    fn fleet_agent_flags_returns_none_when_no_team_dir_and_not_brain() {
        without_team_dir("no_dir_worker", |_| {
            assert!(
                fleet_agent_flags("my-feature", "claude").is_none(),
                "should be None when team dir absent and branch is not brain"
            );
        });
    }

    #[test]
    fn fleet_agent_flags_brain_returns_flags_even_without_team_dir() {
        without_team_dir("no_dir_brain", |_| {
            // Brain creates the team — fleet activates unconditionally for the brain branch.
            let flags = fleet_agent_flags(BRAIN_BRANCH, "claude");
            assert!(
                flags.is_some(),
                "brain should get flags even when team dir absent"
            );
        });
    }

    // --- ensure_fleet_member ---

    #[test]
    fn ensure_fleet_member_creates_inbox_with_empty_array() {
        with_team_dir("inbox_created", |base| {
            ensure_fleet_member("my-feature", std::path::Path::new("/tmp/wt"), "claude");

            let inbox = base
                .join("teams")
                .join(BRAIN_BRANCH)
                .join("inboxes")
                .join("my-feature.json");
            assert!(inbox.exists(), "inbox should be created");
            let content = fs::read_to_string(&inbox).unwrap();
            assert_eq!(
                content.trim(),
                "[]",
                "inbox should be initialized to empty array"
            );
        });
    }

    #[test]
    fn ensure_fleet_member_brain_gets_team_lead_agent_type() {
        with_team_dir("brain_team_lead", |base| {
            ensure_fleet_member(BRAIN_BRANCH, std::path::Path::new("/tmp/brain"), "claude");

            let config_path = base.join("teams").join(BRAIN_BRANCH).join("config.json");
            let config: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
            let members = config["members"].as_array().unwrap();
            assert_eq!(members.len(), 1);
            assert_eq!(
                members[0]["agentType"], "team-lead",
                "brain should be registered as team-lead"
            );
        });
    }

    #[test]
    fn ensure_fleet_member_worker_gets_general_purpose_agent_type() {
        with_team_dir("worker_general_purpose", |base| {
            ensure_fleet_member("my-feature", std::path::Path::new("/tmp/wt"), "claude");

            let config_path = base.join("teams").join(BRAIN_BRANCH).join("config.json");
            let config: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
            let members = config["members"].as_array().unwrap();
            assert_eq!(members.len(), 1);
            assert_eq!(
                members[0]["agentType"], "general-purpose",
                "worker should be registered as general-purpose"
            );
        });
    }

    #[test]
    fn ensure_fleet_member_is_idempotent() {
        with_team_dir("idempotent", |base| {
            ensure_fleet_member("worker", std::path::Path::new("/tmp/wt"), "claude");
            ensure_fleet_member("worker", std::path::Path::new("/tmp/wt"), "claude");

            let config_path = base.join("teams").join(BRAIN_BRANCH).join("config.json");
            let config: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
            assert_eq!(
                config["members"].as_array().unwrap().len(),
                1,
                "second call should not duplicate the member entry"
            );
        });
    }

    #[test]
    fn ensure_fleet_member_does_not_overwrite_existing_inbox() {
        with_team_dir("no_overwrite_inbox", |base| {
            let inbox_dir = base.join("teams").join(BRAIN_BRANCH).join("inboxes");
            fs::create_dir_all(&inbox_dir).unwrap();
            let inbox = inbox_dir.join("worker.json");
            fs::write(
                &inbox,
                r#"[{"from":"honryu","text":"existing msg","read":false}]"#,
            )
            .unwrap();

            ensure_fleet_member("worker", std::path::Path::new("/tmp/wt"), "claude");

            let content = fs::read_to_string(&inbox).unwrap();
            assert!(
                content.contains("existing msg"),
                "existing inbox messages should be preserved"
            );
        });
    }

    #[test]
    fn ensure_fleet_member_non_claude_is_noop() {
        with_team_dir("non_claude_noop", |base| {
            ensure_fleet_member("my-feature", std::path::Path::new("/tmp/wt"), "codex");

            let inbox = base
                .join("teams")
                .join(BRAIN_BRANCH)
                .join("inboxes")
                .join("my-feature.json");
            assert!(!inbox.exists(), "non-claude agent should not create inbox");
        });
    }

    // --- update_team_config error handling ---

    #[test]
    fn update_team_config_returns_early_on_corrupt_config() {
        with_team_dir("corrupt_config", |base| {
            let team_dir = base.join("teams").join(BRAIN_BRANCH);
            let config_path = team_dir.join("config.json");

            // Pre-populate a valid member and then corrupt the config.
            let initial = serde_json::json!({
                "name": "honryu",
                "members": [{"agentId": "existing@honryu", "name": "existing", "agentType": "general-purpose"}]
            });
            fs::write(
                &config_path,
                serde_json::to_string_pretty(&initial).unwrap(),
            )
            .unwrap();

            // Overwrite with corrupt JSON to simulate filesystem corruption.
            fs::write(&config_path, "not valid json {{").unwrap();

            // Calling ensure_fleet_member should not panic and should not overwrite the file.
            ensure_fleet_member("new-worker", std::path::Path::new("/tmp/wt"), "claude");

            // The corrupt file should remain — we never fall back to a fresh empty config.
            let content = fs::read_to_string(&config_path).unwrap();
            assert_eq!(
                content, "not valid json {{",
                "corrupt config should not be overwritten"
            );
        });
    }
}
