/// Fleet mode — Honryū team setup for daemon sessions.
///
/// Two separate gates control fleet functionality:
///
/// - **Dropbox** (`is_dropbox_capable_agent`): file-based protocol (task.md, ack,
///   report.md) available to ALL real AI agents (claude, codex, gemini, kiro, amp,
///   opencode). Bare shell sessions are excluded.
///
/// - **Claude inbox/team** (`is_claude_fleet_agent`): Claude Code inbox JSON injection
///   and `--agent-id`/`--team-name` CLI flags. Claude-only.
///
/// Fleet mode is opt-in: it activates when the honryu team directory exists
/// (~/.claude/teams/honryu/) or when the brain session itself is being created.
use std::path::{Path, PathBuf};

use tracing::warn;

use crate::agents::types::AgentType;

/// Branch name reserved for the Honryū brain session.
pub const BRAIN_BRANCH: &str = "honryu";

/// Team name shared by brain + all workers. Intentionally matches BRAIN_BRANCH.
const TEAM_NAME: &str = BRAIN_BRANCH;

/// Sanitize a branch name for use as a Claude Code agent name and inbox filename.
///
/// Replaces `/` with `-` so branch names like `refactor/consolidate-ipc` become
/// flat filenames (`refactor-consolidate-ipc.json`) instead of nested paths
/// (`refactor/consolidate-ipc.json` which fails because the parent dir doesn't exist).
///
/// Must be used consistently across:
/// - `--agent-name` / `--agent-id` flags passed to Claude Code (`fleet_agent_flags()`)
/// - `agentId` field construction (`fleet_agent_id()`)
/// - Inbox file creation in `ensure_fleet_member()`
/// - Inbox file writes in `write_to_inbox()` (inject.rs)
/// - Config.json member `name` entries (`update_team_config()`)
/// - Inbox file removal in `remove_fleet_member()`
pub fn fleet_safe_name(branch: &str) -> String {
    branch.replace('/', "-")
}

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

/// Returns true if the agent supports the file-based dropbox protocol.
///
/// All real AI agents can read/write dropbox files (task.md, ack, report.md).
/// Only bare shell sessions are excluded — they have no agent to consume tasks.
pub(super) fn is_dropbox_capable_agent(agent: &str) -> bool {
    AgentType::parse(agent).is_some()
}

/// Returns true if the agent supports the Claude Code inbox/team protocol.
///
/// Only claude sessions get inbox JSON injection and `--agent-id`/`--team-name` flags.
pub(super) fn is_claude_fleet_agent(agent: &str) -> bool {
    AgentType::parse(agent) == Some(AgentType::Claude)
}

/// Returns true if fleet mode should apply to a new daemon session.
///
/// Active when the session is the brain itself (team will be created by ensure_fleet_member)
/// or when the team directory already exists (brain was created earlier).
pub(super) fn fleet_mode_active(branch: &str) -> bool {
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
    if !is_claude_fleet_agent(agent) || !fleet_mode_active(branch) {
        return None;
    }

    let safe_name = fleet_safe_name(branch);
    let flags = if branch == BRAIN_BRANCH {
        // Brain loads the kild-brain agent definition and joins as team lead.
        format!(
            "--agent kild-brain --agent-id {safe_name}@{TEAM_NAME} \
             --agent-name {safe_name} --team-name {TEAM_NAME}"
        )
    } else {
        format!(
            "--agent-id {safe_name}@{TEAM_NAME} --agent-name {safe_name} --team-name {TEAM_NAME}"
        )
    };

    Some(flags)
}

/// Ensure the fleet team directory structure exists for this session.
///
/// Creates (if not already present):
/// - `~/.claude/teams/honryu/inboxes/<safe_name>.json` where `safe_name` is
///   `fleet_safe_name(branch)` — e.g., `refactor/foo` → `refactor-foo.json`
/// - `~/.claude/teams/honryu/config.json` (team membership record; appends member if not listed)
///
/// Idempotent — safe to call on every create/open. Warns on failure,
/// never blocks session creation.
pub fn ensure_fleet_member(branch: &str, cwd: &Path, agent: &str) {
    if !is_claude_fleet_agent(agent) || !fleet_mode_active(branch) {
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
    // Sanitize branch name to avoid nested paths (e.g. refactor/foo → refactor-foo).
    let safe_name = fleet_safe_name(branch);
    let inbox = inbox_dir.join(format!("{safe_name}.json"));
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

    let agent_id = fleet_agent_id(branch);

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
        let safe_name = fleet_safe_name(branch);
        serde_json::json!({
            "agentId": agent_id,
            "name": safe_name,
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
                eprintln!(
                    "Warning: Fleet config update failed for '{}': {}",
                    branch, e
                );
                eprintln!("Brain may not see this session in team config.");
            }
        }
        Err(e) => {
            warn!(
                event = "core.session.fleet.config_serialize_failed",
                branch = branch,
                error = %e,
            );
            eprintln!(
                "Warning: Fleet config serialization failed for '{}': {}",
                branch, e
            );
        }
    }
}

/// Returns the agent ID string for a branch in the honryu team.
///
/// Both brain and worker branches use the same `<safe_name>@<team>` format,
/// where `safe_name` is `fleet_safe_name(branch)` (slashes replaced with dashes).
/// Since `TEAM_NAME == BRAIN_BRANCH`, the result is always `"<safe_name>@honryu"`.
fn fleet_agent_id(branch: &str) -> String {
    let safe_name = fleet_safe_name(branch);
    format!("{safe_name}@{TEAM_NAME}")
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

/// Remove fleet membership artifacts for a destroyed session.
///
/// Removes (best-effort, never blocks destroy):
/// - `~/.claude/teams/honryu/inboxes/<safe_name>.json` (inbox file, where
///   `safe_name = fleet_safe_name(branch)`)
/// - The member entry from `~/.claude/teams/honryu/config.json`
///
/// No-op if the Claude config directory cannot be resolved (HOME not set),
/// or if the files don't exist. Agent type and fleet-mode checks are
/// intentionally skipped — the session is already being destroyed and its
/// agent type is unavailable. Cleanup is opportunistic: if the files exist,
/// they are removed.
pub fn remove_fleet_member(branch: &str) {
    let Some(dir) = team_dir() else {
        return;
    };

    // Remove inbox file. Sanitize branch to match the path used in ensure_fleet_member.
    let safe_name = fleet_safe_name(branch);
    let inbox = dir.join("inboxes").join(format!("{safe_name}.json"));
    match inbox.try_exists() {
        Ok(true) => {
            if let Err(e) = std::fs::remove_file(&inbox) {
                warn!(
                    event = "core.session.fleet.inbox_remove_failed",
                    branch = branch,
                    path = %inbox.display(),
                    error = %e,
                );
                eprintln!(
                    "Warning: Failed to remove fleet inbox for '{}': {}",
                    branch, e
                );
            }
        }
        Ok(false) => {} // Already gone — nothing to do.
        Err(e) => {
            warn!(
                event = "core.session.fleet.inbox_remove_check_failed",
                branch = branch,
                path = %inbox.display(),
                error = %e,
            );
            eprintln!(
                "Warning: Failed to check fleet inbox for '{}': {}",
                branch, e
            );
        }
    }

    // Remove member entry from config.json.
    remove_from_team_config(branch, &dir);
}

fn remove_from_team_config(branch: &str, dir: &Path) {
    let config_path = dir.join("config.json");
    match config_path.try_exists() {
        Ok(true) => {}
        Ok(false) => return,
        Err(e) => {
            warn!(
                event = "core.session.fleet.config_exists_check_failed",
                branch = branch,
                error = %e,
            );
            return;
        }
    }

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

    let mut config: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            warn!(
                event = "core.session.fleet.config_parse_failed",
                branch = branch,
                error = %e,
            );
            return;
        }
    };

    let members = match config.get_mut("members").and_then(|m| m.as_array_mut()) {
        Some(m) => m,
        None => return,
    };

    let agent_id = fleet_agent_id(branch);

    let before = members.len();
    members.retain(|m| m.get("agentId").and_then(|v| v.as_str()) != Some(&agent_id));

    if members.len() == before {
        return; // Nothing removed — skip the write.
    }

    match serde_json::to_string_pretty(&config) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&config_path, json) {
                warn!(
                    event = "core.session.fleet.config_write_failed",
                    branch = branch,
                    path = %config_path.display(),
                    error = %e,
                );
                eprintln!(
                    "Warning: Failed to update fleet config for '{}': {}",
                    branch, e
                );
            }
        }
        Err(e) => {
            warn!(
                event = "core.session.fleet.config_serialize_failed",
                branch = branch,
                error = %e,
            );
            eprintln!(
                "Warning: Failed to serialize fleet config for '{}': {}",
                branch, e
            );
        }
    }
}

/// Serialize all tests that mutate CLAUDE_CONFIG_DIR — env vars are process-global.
///
/// Shared across `fleet::tests` and `dropbox::tests` so neither module can
/// overwrite `CLAUDE_CONFIG_DIR` while the other is mid-test.
#[cfg(test)]
pub(super) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a temp dir with `~/.claude/teams/honryu/` already present and set
    /// `CLAUDE_CONFIG_DIR` to point at it. Calls `f` while holding the env lock.
    fn with_team_dir(test_name: &str, f: impl FnOnce(&std::path::Path)) {
        let _lock = ENV_LOCK.lock().unwrap();
        let base = std::env::temp_dir().join(format!(
            "kild_fleet_test_{}_{}",
            test_name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let team_dir = base.join("teams").join(BRAIN_BRANCH);
        fs::create_dir_all(&team_dir).unwrap();
        // SAFETY: ENV_LOCK serializes all CLAUDE_CONFIG_DIR mutations in this module.
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &base) };
        f(&base);
        let _ = fs::remove_dir_all(&base);
        // SAFETY: restoring env; lock still held.
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
    }

    /// Create a temp dir WITHOUT the team directory (fleet not yet started).
    fn without_team_dir(test_name: &str, f: impl FnOnce(&std::path::Path)) {
        let _lock = ENV_LOCK.lock().unwrap();
        let base = std::env::temp_dir().join(format!(
            "kild_fleet_no_dir_{}_{}",
            test_name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        // SAFETY: ENV_LOCK serializes all CLAUDE_CONFIG_DIR mutations in this module.
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &base) };
        f(&base);
        let _ = fs::remove_dir_all(&base);
        // SAFETY: restoring env; lock still held.
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
    }

    // --- fleet_safe_name ---

    #[test]
    fn fleet_safe_name_no_slash_unchanged() {
        assert_eq!(fleet_safe_name("my-feature"), "my-feature");
    }

    #[test]
    fn fleet_safe_name_single_slash_replaced() {
        assert_eq!(fleet_safe_name("refactor/foo"), "refactor-foo");
    }

    #[test]
    fn fleet_safe_name_multiple_slashes_replaced() {
        assert_eq!(fleet_safe_name("a/b/c"), "a-b-c");
    }

    #[test]
    fn fleet_safe_name_brain_unchanged() {
        assert_eq!(fleet_safe_name(BRAIN_BRANCH), BRAIN_BRANCH);
    }

    // --- is_dropbox_capable_agent ---

    #[test]
    fn is_dropbox_capable_agent_true_for_all_real_agents() {
        assert!(is_dropbox_capable_agent("claude"));
        assert!(is_dropbox_capable_agent("codex"));
        assert!(is_dropbox_capable_agent("gemini"));
        assert!(is_dropbox_capable_agent("kiro"));
        assert!(is_dropbox_capable_agent("amp"));
        assert!(is_dropbox_capable_agent("opencode"));
    }

    #[test]
    fn is_dropbox_capable_agent_false_for_shell() {
        assert!(!is_dropbox_capable_agent("shell"));
        assert!(!is_dropbox_capable_agent(""));
        assert!(!is_dropbox_capable_agent("unknown-thing"));
    }

    // --- is_claude_fleet_agent ---

    #[test]
    fn is_claude_fleet_agent_true_only_for_claude() {
        assert!(is_claude_fleet_agent("claude"));
        assert!(!is_claude_fleet_agent("codex"));
        assert!(!is_claude_fleet_agent("gemini"));
        assert!(!is_claude_fleet_agent("kiro"));
        assert!(!is_claude_fleet_agent("amp"));
        assert!(!is_claude_fleet_agent("opencode"));
        assert!(!is_claude_fleet_agent("shell"));
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
    fn fleet_agent_flags_slashed_branch_sanitized_in_agent_name() {
        with_team_dir("slashed_branch_flags", |_| {
            let flags = fleet_agent_flags("refactor/consolidate-ipc", "claude").unwrap();
            assert!(
                flags.contains("--agent-id refactor-consolidate-ipc@honryu"),
                "slashed branch should be sanitized in agent-id, got: {}",
                flags
            );
            assert!(
                flags.contains("--agent-name refactor-consolidate-ipc"),
                "slashed branch should be sanitized in agent-name, got: {}",
                flags
            );
            assert!(
                !flags.contains("refactor/consolidate-ipc"),
                "raw slashed branch should not appear in flags, got: {}",
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
    fn ensure_fleet_member_slashed_branch_creates_flat_inbox_file() {
        with_team_dir("slashed_inbox", |base| {
            ensure_fleet_member(
                "refactor/consolidate-ipc",
                std::path::Path::new("/tmp/wt"),
                "claude",
            );

            // Should create a flat file, not a nested directory.
            let flat_inbox = base
                .join("teams")
                .join(BRAIN_BRANCH)
                .join("inboxes")
                .join("refactor-consolidate-ipc.json");
            assert!(
                flat_inbox.exists(),
                "slashed branch should create flat inbox file"
            );

            // The nested path should NOT exist.
            let nested_dir = base
                .join("teams")
                .join(BRAIN_BRANCH)
                .join("inboxes")
                .join("refactor");
            assert!(
                !nested_dir.exists(),
                "slashed branch should not create nested directory"
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

    // --- remove_fleet_member ---

    #[test]
    fn remove_fleet_member_removes_inbox_and_config_entry() {
        with_team_dir("remove_both", |base| {
            // Set up a member first.
            ensure_fleet_member("worker", std::path::Path::new("/tmp/wt"), "claude");

            let inbox = base
                .join("teams")
                .join(BRAIN_BRANCH)
                .join("inboxes")
                .join("worker.json");
            let config_path = base.join("teams").join(BRAIN_BRANCH).join("config.json");
            assert!(inbox.exists(), "precondition: inbox exists");

            let config: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
            assert_eq!(config["members"].as_array().unwrap().len(), 1);

            remove_fleet_member("worker");

            assert!(!inbox.exists(), "inbox should be removed after destroy");

            let config: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
            assert_eq!(
                config["members"].as_array().unwrap().len(),
                0,
                "member entry should be removed from config"
            );
        });
    }

    #[test]
    fn remove_fleet_member_slashed_branch_removes_flat_inbox() {
        with_team_dir("slashed_remove", |base| {
            ensure_fleet_member("refactor/foo", std::path::Path::new("/tmp/wt"), "claude");

            let flat_inbox = base
                .join("teams")
                .join(BRAIN_BRANCH)
                .join("inboxes")
                .join("refactor-foo.json");
            assert!(
                flat_inbox.exists(),
                "precondition: flat inbox exists after create"
            );

            remove_fleet_member("refactor/foo");

            assert!(
                !flat_inbox.exists(),
                "flat inbox should be removed for slashed branch"
            );
        });
    }

    #[test]
    fn remove_fleet_member_noop_when_files_absent() {
        with_team_dir("remove_noop", |_base| {
            // No inbox or config entry exists — should not panic.
            remove_fleet_member("nonexistent");
        });
    }

    #[test]
    fn remove_fleet_member_noop_without_team_dir() {
        without_team_dir("remove_no_team", |_base| {
            // Team dir doesn't exist — should not panic.
            remove_fleet_member("worker");
        });
    }

    #[test]
    fn remove_fleet_member_preserves_other_members() {
        with_team_dir("remove_preserves_others", |base| {
            ensure_fleet_member("worker-a", std::path::Path::new("/tmp/a"), "claude");
            ensure_fleet_member("worker-b", std::path::Path::new("/tmp/b"), "claude");

            let config_path = base.join("teams").join(BRAIN_BRANCH).join("config.json");
            let config: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
            assert_eq!(config["members"].as_array().unwrap().len(), 2);

            remove_fleet_member("worker-a");

            let config: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
            let members = config["members"].as_array().unwrap();
            assert_eq!(members.len(), 1, "only worker-a should be removed");
            assert_eq!(members[0]["name"], "worker-b");

            // worker-a inbox removed, worker-b inbox still present.
            let inbox_a = base
                .join("teams")
                .join(BRAIN_BRANCH)
                .join("inboxes")
                .join("worker-a.json");
            let inbox_b = base
                .join("teams")
                .join(BRAIN_BRANCH)
                .join("inboxes")
                .join("worker-b.json");
            assert!(!inbox_a.exists());
            assert!(inbox_b.exists());
        });
    }

    #[test]
    fn remove_fleet_member_removes_brain_entry() {
        with_team_dir("remove_brain", |base| {
            ensure_fleet_member(BRAIN_BRANCH, std::path::Path::new("/tmp/brain"), "claude");

            let config_path = base.join("teams").join(BRAIN_BRANCH).join("config.json");
            let config: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
            assert_eq!(config["members"].as_array().unwrap().len(), 1);

            remove_fleet_member(BRAIN_BRANCH);

            let config: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
            assert_eq!(
                config["members"].as_array().unwrap().len(),
                0,
                "brain's team-lead entry should be removed"
            );
        });
    }

    #[test]
    fn remove_fleet_member_removes_config_when_inbox_already_gone() {
        with_team_dir("config_only_remove", |base| {
            ensure_fleet_member("worker", std::path::Path::new("/tmp/wt"), "claude");

            // Simulate inbox already consumed/removed.
            let inbox = base
                .join("teams")
                .join(BRAIN_BRANCH)
                .join("inboxes")
                .join("worker.json");
            fs::remove_file(&inbox).unwrap();
            assert!(!inbox.exists());

            let config_path = base.join("teams").join(BRAIN_BRANCH).join("config.json");

            remove_fleet_member("worker");

            let config: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
            assert_eq!(
                config["members"].as_array().unwrap().len(),
                0,
                "config entry should be removed even when inbox was already absent"
            );
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
