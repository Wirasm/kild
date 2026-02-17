use std::path::Path;

use kild_paths::KildPaths;
use tracing::{debug, info, warn};

use crate::agents;
use crate::config::KildConfig;
use crate::sessions::errors::SessionError;
use crate::terminal;
use crate::terminal::types::TerminalType;

/// Compute a unique spawn ID for a given session and spawn index.
///
/// Each agent spawn within a session gets its own spawn ID, which is used for
/// per-agent PID file paths and window titles. This prevents race conditions
/// where `kild open` on a running kild would read the wrong PID.
pub(super) fn compute_spawn_id(session_id: &str, spawn_index: usize) -> String {
    format!("{}_{}", session_id, spawn_index)
}

/// Ensure the tmux shim binary is installed at `~/.kild/bin/tmux`.
///
/// Looks for `kild-tmux-shim` next to the running `kild` binary and symlinks
/// it as `tmux` in `~/.kild/bin/`. Agent teams require this binary.
pub(crate) fn ensure_shim_binary() -> Result<(), String> {
    let paths = KildPaths::resolve().map_err(|e| e.to_string())?;
    let shim_bin_dir = paths.bin_dir();
    let shim_link = paths.tmux_shim_binary();

    if shim_link.exists() {
        return Ok(());
    }

    let shim_binary = crate::daemon::find_sibling_binary("kild-tmux-shim")?;

    std::fs::create_dir_all(&shim_bin_dir)
        .map_err(|e| format!("failed to create {}: {}", shim_bin_dir.display(), e))?;

    #[cfg(unix)]
    std::os::unix::fs::symlink(&shim_binary, &shim_link).map_err(|e| {
        format!(
            "failed to symlink {} -> {}: {}",
            shim_binary.display(),
            shim_link.display(),
            e
        )
    })?;

    info!(
        event = "core.session.shim_binary_installed",
        path = %shim_link.display()
    );

    Ok(())
}

/// Spawn a terminal attach window for a daemon session (best-effort).
///
/// After a daemon PTY is created, this spawns a terminal window running
/// `kild attach <branch>` so the CLI user gets immediate visual feedback.
/// The terminal backend is selected via user config or auto-detection
/// (Ghostty > iTerm > Terminal.app on macOS).
/// The attach process is ephemeral — Ctrl+C detaches without killing the agent.
///
/// Returns `Some((terminal_type, window_id))` on success for storage in
/// `AgentProcess`, enabling cleanup during destroy. Returns `None` on failure.
/// Failures are logged as warnings but never block session creation.
pub fn spawn_attach_window(
    branch: &str,
    spawn_id: &str,
    worktree_path: &Path,
    kild_config: &KildConfig,
) -> Option<(TerminalType, String)> {
    info!(event = "core.session.auto_attach_started", branch = branch);

    let kild_binary = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            warn!(
                event = "core.session.auto_attach_failed",
                branch = branch,
                error = %e,
                "Could not resolve kild binary path for auto-attach"
            );
            eprintln!("Warning: Could not auto-attach to daemon session: {}", e);
            eprintln!("         Use `kild attach {}` to connect manually.", branch);
            return None;
        }
    };

    let attach_command = format!("{} attach '{}'", kild_binary.display(), branch);

    // Pass None for kild_dir to skip PID file creation — the attach process is ephemeral
    match terminal::handler::spawn_terminal(
        worktree_path,
        &attach_command,
        kild_config,
        Some(spawn_id),
        None,
    ) {
        Ok(result) => {
            info!(
                event = "core.session.auto_attach_completed",
                branch = branch,
                window_id = ?result.terminal_window_id
            );
            result
                .terminal_window_id
                .map(|wid| (result.terminal_type, wid))
        }
        Err(e) => {
            warn!(
                event = "core.session.auto_attach_failed",
                branch = branch,
                error = %e,
                "Could not spawn attach window for daemon session"
            );
            eprintln!("Warning: Could not auto-attach to daemon session: {}", e);
            eprintln!("         Use `kild attach {}` to connect manually.", branch);
            None
        }
    }
}

/// Spawn attach window and update session with terminal info.
///
/// Only runs for daemon sessions. Calls `spawn_attach_window()` and updates
/// the session's latest agent with terminal type and window ID. Saves the
/// updated session to disk.
///
/// Returns `Ok(true)` on success, `Ok(false)` if attach window fails (best-effort).
/// Returns `Err` only if session save fails.
pub fn spawn_and_save_attach_window(
    session: &mut super::types::Session,
    branch: &str,
    kild_config: &KildConfig,
    sessions_dir: &Path,
) -> Result<bool, SessionError> {
    use super::persistence;

    let attach_spawn_id = session
        .latest_agent()
        .map(|a| a.spawn_id().to_string())
        .unwrap_or_default();

    let Some((tt, wid)) = spawn_attach_window(
        branch,
        &attach_spawn_id,
        &session.worktree_path,
        kild_config,
    ) else {
        return Ok(false);
    };

    let Some(agent) = session.latest_agent_mut() else {
        return Ok(false);
    };

    agent.set_attach_info(tt, wid);
    persistence::save_session_to_file(session, sessions_dir)?;
    Ok(true)
}

/// Ensure the Codex notify hook script is installed at `<home>/.kild/hooks/codex-notify`.
///
/// This script is called by Codex CLI's `notify` config. It reads JSON from stdin,
/// maps event types to KILD agent statuses, and calls `kild agent-status`.
/// Event mappings: `agent-turn-complete` → `idle`, `approval-requested` → `waiting`.
/// Idempotent: skips if script already exists.
fn ensure_codex_notify_hook_with_paths(paths: &KildPaths) -> Result<(), String> {
    let hooks_dir = paths.hooks_dir();
    let hook_path = paths.codex_notify_hook();

    if hook_path.exists() {
        debug!(
            event = "core.session.codex_notify_hook_already_exists",
            path = %hook_path.display()
        );
        return Ok(());
    }

    std::fs::create_dir_all(&hooks_dir)
        .map_err(|e| format!("failed to create {}: {}", hooks_dir.display(), e))?;

    let script = r#"#!/bin/sh
# KILD Codex notify hook — auto-generated, do not edit.
# Called by Codex CLI via notify config with JSON on stdin.
# Maps Codex events to KILD agent statuses.
INPUT=$(cat)
EVENT_TYPE=$(echo "$INPUT" | grep -o '"type":"[^"]*"' | head -1 | sed 's/"type":"//;s/"//')
case "$EVENT_TYPE" in
  agent-turn-complete) kild agent-status --self idle --notify ;;
  approval-requested)  kild agent-status --self waiting --notify ;;
esac
"#;

    std::fs::write(&hook_path, script)
        .map_err(|e| format!("failed to write {}: {}", hook_path.display(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("failed to chmod {}: {}", hook_path.display(), e))?;
    }

    info!(
        event = "core.session.codex_notify_hook_installed",
        path = %hook_path.display()
    );

    Ok(())
}

pub(crate) fn ensure_codex_notify_hook() -> Result<(), String> {
    let paths = KildPaths::resolve().map_err(|e| e.to_string())?;
    ensure_codex_notify_hook_with_paths(&paths)
}

/// Ensure Codex CLI config has the KILD notify hook configured.
///
/// Patches `<home>/.codex/config.toml` to add `notify = ["<path>"]` if the notify
/// field is missing or empty. Respects existing user configuration — if notify
/// is already set to a non-empty array, it is left unchanged and this function
/// returns Ok without modifying the file.
fn ensure_codex_config_with_home(home: &Path, paths: &KildPaths) -> Result<(), String> {
    let codex_dir = home.join(".codex");
    let config_path = codex_dir.join("config.toml");
    let hook_path = paths.codex_notify_hook();
    let hook_path_str = hook_path.display().to_string();

    use std::fmt::Write;

    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("failed to read {}: {}", config_path.display(), e))?;

        // Parse to check if notify is already configured with a non-empty array.
        // Propagate parse errors so we don't blindly append to a malformed file.
        let parsed = content.parse::<toml::Value>().map_err(|e| {
            format!(
                "failed to parse {}: {} — fix TOML syntax or remove the file to reset",
                config_path.display(),
                e
            )
        })?;

        if let Some(toml::Value::Array(arr)) = parsed.get("notify")
            && !arr.is_empty()
        {
            info!(event = "core.session.codex_config_already_configured");
            return Ok(());
        }

        // notify is missing or empty — append it, preserving existing content
        let mut new_content = content;
        if !new_content.ends_with('\n') && !new_content.is_empty() {
            new_content.push('\n');
        }
        writeln!(new_content, "notify = [\"{}\"]", hook_path_str)
            .expect("String formatting is infallible");

        std::fs::write(&config_path, new_content)
            .map_err(|e| format!("failed to write {}: {}", config_path.display(), e))?;
    } else {
        // Config doesn't exist — create it with just the notify line
        std::fs::create_dir_all(&codex_dir)
            .map_err(|e| format!("failed to create {}: {}", codex_dir.display(), e))?;
        let mut content = String::new();
        writeln!(content, "notify = [\"{}\"]", hook_path_str)
            .expect("String formatting is infallible");
        std::fs::write(&config_path, content)
            .map_err(|e| format!("failed to write {}: {}", config_path.display(), e))?;
    }

    info!(
        event = "core.session.codex_config_patched",
        path = %config_path.display()
    );

    Ok(())
}

pub(crate) fn ensure_codex_config() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("HOME not set — cannot patch Codex config")?;
    let paths = KildPaths::resolve().map_err(|e| e.to_string())?;
    ensure_codex_config_with_home(&home, &paths)
}

/// Install Codex notify hook and patch config if needed.
///
/// Best-effort: warns on failure but doesn't block session creation.
/// No-op for non-Codex agents.
pub(crate) fn setup_codex_integration(agent: &str) {
    if agent != "codex" {
        return;
    }

    if let Err(msg) = ensure_codex_notify_hook() {
        warn!(event = "core.session.codex_notify_hook_failed", error = %msg);
        eprintln!("Warning: {msg}");
        eprintln!("Codex status reporting may not work.");
    }

    if let Err(msg) = ensure_codex_config() {
        warn!(event = "core.session.codex_config_patch_failed", error = %msg);
        eprintln!("Warning: {msg}");
        let hook_path = KildPaths::resolve()
            .map(|p| p.codex_notify_hook().display().to_string())
            .unwrap_or_else(|_| "<HOME>/.kild/hooks/codex-notify".to_string());
        let config_path = dirs::home_dir()
            .map(|h| h.join(".codex/config.toml").display().to_string())
            .unwrap_or_else(|| "<HOME>/.codex/config.toml".to_string());
        eprintln!("Add notify = [\"{hook_path}\"] to {config_path} manually.");
    }
}

/// Ensure the Claude Code status hook script is installed at `~/.kild/hooks/claude-status`.
///
/// This script is registered in Claude Code's `~/.claude/settings.json` for Stop,
/// Notification, SubagentStop, TeammateIdle, and TaskCompleted hooks. It reads JSON
/// from stdin, maps Claude Code events to KILD agent statuses, and calls
/// `kild agent-status --self <status> --notify`.
/// Idempotent: skips if script already exists.
fn ensure_claude_status_hook_with_paths(paths: &KildPaths) -> Result<(), String> {
    let hooks_dir = paths.hooks_dir();
    let hook_path = paths.claude_status_hook();

    if hook_path.exists() {
        debug!(
            event = "core.session.claude_status_hook_already_exists",
            path = %hook_path.display()
        );
        return Ok(());
    }

    std::fs::create_dir_all(&hooks_dir)
        .map_err(|e| format!("failed to create {}: {}", hooks_dir.display(), e))?;

    let script = r#"#!/bin/sh
# KILD Claude Code status hook — auto-generated, do not edit.
# Registered in ~/.claude/settings.json for Stop, Notification, SubagentStop,
# TeammateIdle, and TaskCompleted hooks.
# Maps Claude Code events to KILD agent statuses.
INPUT=$(cat)
EVENT=$(echo "$INPUT" | grep -o '"hook_event_name":"[^"]*"' | head -1 | sed 's/"hook_event_name":"//;s/"//')
NTYPE=$(echo "$INPUT" | grep -o '"notification_type":"[^"]*"' | head -1 | sed 's/"notification_type":"//;s/"//')
case "$EVENT" in
  Stop|SubagentStop|TeammateIdle|TaskCompleted)
    kild agent-status --self idle --notify
    ;;
  Notification)
    case "$NTYPE" in
      permission_prompt) kild agent-status --self waiting --notify ;;
      idle_prompt)       kild agent-status --self idle --notify ;;
    esac
    ;;
esac
"#;

    std::fs::write(&hook_path, script)
        .map_err(|e| format!("failed to write {}: {}", hook_path.display(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("failed to chmod {}: {}", hook_path.display(), e))?;
    }

    info!(
        event = "core.session.claude_status_hook_installed",
        path = %hook_path.display()
    );

    Ok(())
}

pub fn ensure_claude_status_hook() -> Result<(), String> {
    let paths = KildPaths::resolve().map_err(|e| e.to_string())?;
    ensure_claude_status_hook_with_paths(&paths)
}

/// Ensure Claude Code settings.json has KILD status hooks configured.
///
/// Patches `~/.claude/settings.json` to add Stop, Notification, SubagentStop,
/// TeammateIdle, and TaskCompleted hook entries pointing to the claude-status
/// hook script. Preserves all existing settings and hooks.
/// Idempotent: skips if any hook already references the claude-status script.
fn ensure_claude_settings_with_home(home: &Path, paths: &KildPaths) -> Result<(), String> {
    let claude_dir = home.join(".claude");
    let settings_path = claude_dir.join("settings.json");
    let hook_path = paths.claude_status_hook();
    let hook_path_str = hook_path.display().to_string();

    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)
            .map_err(|e| format!("failed to read {}: {}", settings_path.display(), e))?;
        serde_json::from_str(&content).map_err(|e| {
            format!(
                "failed to parse {}: {} — fix JSON syntax or remove the file to reset",
                settings_path.display(),
                e
            )
        })?
    } else {
        serde_json::json!({})
    };

    // Helper: check if a hook array already contains our script
    let has_our_hook = |entries: &serde_json::Value| -> bool {
        if let Some(arr) = entries.as_array() {
            arr.iter().any(|entry| {
                if let Some(serde_json::Value::Array(hook_list)) = entry.get("hooks") {
                    hook_list
                        .iter()
                        .any(|h| h.get("command").and_then(|c| c.as_str()) == Some(&hook_path_str))
                } else {
                    false
                }
            })
        } else {
            false
        }
    };

    // Build hook entries
    let hook_entry = serde_json::json!({
        "type": "command",
        "command": hook_path_str,
        "timeout": 5
    });

    let hooks = settings
        .as_object_mut()
        .ok_or("settings.json root is not an object")?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    let hooks_obj = hooks
        .as_object_mut()
        .ok_or("\"hooks\" field in settings.json is not an object")?;

    let mut added = 0;

    // Stop, SubagentStop, TeammateIdle, TaskCompleted: no matcher needed
    for event in &["Stop", "SubagentStop", "TeammateIdle", "TaskCompleted"] {
        let entries = hooks_obj
            .entry(*event)
            .or_insert_with(|| serde_json::json!([]));

        if has_our_hook(entries) {
            continue;
        }

        let arr = entries
            .as_array_mut()
            .ok_or_else(|| format!("\"{event}\" field in settings.json is not an array"))?;
        arr.push(serde_json::json!({
            "hooks": [hook_entry.clone()]
        }));
        added += 1;
    }

    // Notification: matcher for permission_prompt and idle_prompt
    let notification_entries = hooks_obj
        .entry("Notification")
        .or_insert_with(|| serde_json::json!([]));

    if !has_our_hook(notification_entries) {
        let arr = notification_entries
            .as_array_mut()
            .ok_or("\"Notification\" field in settings.json is not an array")?;
        arr.push(serde_json::json!({
            "matcher": "permission_prompt|idle_prompt",
            "hooks": [hook_entry.clone()]
        }));
        added += 1;
    }

    if added == 0 {
        info!(event = "core.session.claude_settings_already_configured");
        return Ok(());
    }

    // Write back
    std::fs::create_dir_all(&claude_dir)
        .map_err(|e| format!("failed to create {}: {}", claude_dir.display(), e))?;

    let content = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("failed to serialize settings.json: {}", e))?;

    std::fs::write(&settings_path, format!("{}\n", content))
        .map_err(|e| format!("failed to write {}: {}", settings_path.display(), e))?;

    info!(
        event = "core.session.claude_settings_patched",
        path = %settings_path.display()
    );

    Ok(())
}

pub fn ensure_claude_settings() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("HOME not set — cannot patch Claude Code settings")?;
    let paths = KildPaths::resolve().map_err(|e| e.to_string())?;
    ensure_claude_settings_with_home(&home, &paths)
}

/// Install Claude Code status hook and patch settings if needed.
///
/// Best-effort: warns on failure but doesn't block session creation.
/// No-op for non-Claude agents.
pub(crate) fn setup_claude_integration(agent: &str) {
    if agent != "claude" {
        return;
    }

    if let Err(msg) = ensure_claude_status_hook() {
        warn!(event = "core.session.claude_status_hook_failed", error = %msg);
        eprintln!("Warning: {msg}");
        eprintln!("Claude Code status reporting may not work.");
    }

    if let Err(msg) = ensure_claude_settings() {
        warn!(event = "core.session.claude_settings_patch_failed", error = %msg);
        eprintln!("Warning: {msg}");
        let hook_path = match KildPaths::resolve() {
            Ok(p) => p.claude_status_hook().display().to_string(),
            Err(_) => "<HOME>/.kild/hooks/claude-status".to_string(),
        };
        let settings_path = match dirs::home_dir() {
            Some(h) => h.join(".claude/settings.json").display().to_string(),
            None => "<HOME>/.claude/settings.json".to_string(),
        };
        eprintln!("Add hooks entries referencing \"{hook_path}\" to {settings_path} manually.");
    }
}

/// Ensure the OpenCode KILD status plugin is installed in a worktree.
///
/// Creates `.opencode/plugins/kild-status.ts` in the worktree directory.
/// The plugin listens to OpenCode events and reports agent status back to KILD
/// via `kild agent-status --self <status> --notify`.
/// Idempotent: skips if `.opencode/plugins/kild-status.ts` already exists.
///
/// Public for use by `kild init-hooks` CLI command.
/// Most consumers should use `setup_opencode_integration()` instead.
pub fn ensure_opencode_plugin_in_worktree(worktree_path: &Path) -> Result<(), String> {
    let plugins_dir = worktree_path.join(".opencode").join("plugins");
    let plugin_path = plugins_dir.join("kild-status.ts");

    if plugin_path.exists() {
        debug!(
            event = "core.session.opencode_plugin_already_exists",
            path = %plugin_path.display()
        );
        return Ok(());
    }

    std::fs::create_dir_all(&plugins_dir)
        .map_err(|e| format!("failed to create {}: {}", plugins_dir.display(), e))?;

    let plugin_content = r#"import type { Plugin } from "@opencode-ai/plugin"

export default (async ({ $ }) => {
  const updateStatus = async (status: string) => {
    try {
      await $`kild agent-status --self ${status} --notify`.quiet().nothrow()
    } catch (error) {
      console.error(`[kild-status] Failed to report ${status}:`, error)
    }
  }

  return {
    event: async ({ event }) => {
      switch (event.type) {
        case "session.created":
          await updateStatus("working")
          break
        case "session.idle":
          await updateStatus("idle")
          break
        case "session.error":
          await updateStatus("error")
          break
        case "permission.ask":
          await updateStatus("waiting")
          break
      }
    }
  }
}) satisfies Plugin
"#;

    std::fs::write(&plugin_path, plugin_content)
        .map_err(|e| format!("failed to write {}: {}", plugin_path.display(), e))?;

    info!(
        event = "core.session.opencode_plugin_installed",
        path = %plugin_path.display()
    );

    Ok(())
}

/// Ensure the OpenCode `.opencode/package.json` exists with the plugin dependency.
///
/// Creates `.opencode/package.json` in the worktree or merges `@opencode-ai/plugin`
/// into an existing file's dependencies. Preserves all existing fields (name, scripts, etc.).
/// Idempotent: skips only if file exists and `dependencies` already contains `@opencode-ai/plugin`.
///
/// Public for use by `kild init-hooks` CLI command.
/// Most consumers should use `setup_opencode_integration()` instead.
///
/// # Errors
/// Returns `Err` if:
/// - `package.json` exists but contains invalid JSON syntax
/// - `package.json` root is not an object
/// - `dependencies` field exists but is not an object
pub fn ensure_opencode_package_json(worktree_path: &Path) -> Result<(), String> {
    let opencode_dir = worktree_path.join(".opencode");
    let package_path = opencode_dir.join("package.json");

    let mut package_json: serde_json::Value = if package_path.exists() {
        let content = std::fs::read_to_string(&package_path)
            .map_err(|e| format!("failed to read {}: {}", package_path.display(), e))?;
        serde_json::from_str(&content).map_err(|e| {
            format!(
                "failed to parse {}: {} — fix JSON syntax or remove the file to reset",
                package_path.display(),
                e
            )
        })?
    } else {
        serde_json::json!({
            "name": "opencode-kild-plugins",
            "private": true
        })
    };

    // Check if dependency already exists
    if let Some(serde_json::Value::Object(deps)) = package_json.get("dependencies")
        && deps.contains_key("@opencode-ai/plugin")
    {
        debug!(
            event = "core.session.opencode_package_json_already_exists",
            path = %package_path.display()
        );
        return Ok(());
    }

    // Add dependency — create dependencies object if missing
    let deps = package_json
        .as_object_mut()
        .ok_or("package.json root is not an object")?
        .entry("dependencies")
        .or_insert_with(|| serde_json::json!({}));

    if let serde_json::Value::Object(deps_obj) = deps {
        deps_obj.insert(
            "@opencode-ai/plugin".to_string(),
            serde_json::Value::String("latest".to_string()),
        );
    } else {
        return Err(format!(
            "\"dependencies\" field in {} is not an object",
            package_path.display()
        ));
    }

    std::fs::create_dir_all(&opencode_dir)
        .map_err(|e| format!("failed to create {}: {}", opencode_dir.display(), e))?;

    let content = serde_json::to_string_pretty(&package_json)
        .map_err(|e| format!("failed to serialize package.json: {}", e))?;

    std::fs::write(&package_path, format!("{}\n", content))
        .map_err(|e| format!("failed to write {}: {}", package_path.display(), e))?;

    info!(
        event = "core.session.opencode_package_json_installed",
        path = %package_path.display()
    );

    Ok(())
}

/// Ensure `opencode.json` in the worktree has the KILD status plugin configured.
///
/// Reads existing `opencode.json` or creates a new one, then adds
/// `"plugins": ["file://.opencode/plugins/kild-status.ts"]` if not already present.
/// Respects existing plugins: appends to array, doesn't replace.
/// Uses `serde_json` for safe JSON manipulation.
///
/// Public for use by `kild init-hooks` CLI command.
/// Most consumers should use `setup_opencode_integration()` instead.
///
/// # Errors
/// Returns `Err` if:
/// - `opencode.json` exists but contains invalid JSON syntax
/// - `opencode.json` root is not an object
/// - `plugins` field exists but is not an array
pub fn ensure_opencode_config(worktree_path: &Path) -> Result<(), String> {
    let config_path = worktree_path.join("opencode.json");
    let plugin_entry = "file://.opencode/plugins/kild-status.ts";

    let mut config: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("failed to read {}: {}", config_path.display(), e))?;
        serde_json::from_str(&content).map_err(|e| {
            format!(
                "failed to parse {}: {} — fix JSON syntax or remove the file to reset",
                config_path.display(),
                e
            )
        })?
    } else {
        serde_json::json!({})
    };

    // Check if plugin is already configured
    if let Some(serde_json::Value::Array(plugins)) = config.get("plugins")
        && plugins.iter().any(|v| v.as_str() == Some(plugin_entry))
    {
        info!(event = "core.session.opencode_config_already_configured");
        return Ok(());
    }

    // Add plugin entry — append to existing array or create new one
    let plugins = config
        .as_object_mut()
        .ok_or("opencode.json root is not an object")?
        .entry("plugins")
        .or_insert_with(|| serde_json::json!([]));

    if let serde_json::Value::Array(arr) = plugins {
        arr.push(serde_json::Value::String(plugin_entry.to_string()));
    } else {
        return Err(format!(
            "\"plugins\" field in {} is not an array",
            config_path.display()
        ));
    }

    let content = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("failed to serialize opencode.json: {}", e))?;

    std::fs::write(&config_path, format!("{}\n", content))
        .map_err(|e| format!("failed to write {}: {}", config_path.display(), e))?;

    info!(
        event = "core.session.opencode_config_patched",
        path = %config_path.display()
    );

    Ok(())
}

/// Install OpenCode plugin files and patch config if needed.
///
/// Best-effort: warns on failure but doesn't block session creation.
/// No-op for non-OpenCode agents.
pub(crate) fn setup_opencode_integration(agent: &str, worktree_path: &Path) {
    if agent != "opencode" {
        return;
    }

    if let Err(msg) = ensure_opencode_plugin_in_worktree(worktree_path) {
        warn!(event = "core.session.opencode_plugin_failed", error = %msg);
        eprintln!("Warning: {msg}");
        eprintln!("OpenCode status reporting may not work.");
    }

    if let Err(msg) = ensure_opencode_package_json(worktree_path) {
        warn!(event = "core.session.opencode_package_json_failed", error = %msg);
        eprintln!("Warning: {msg}");
    }

    if let Err(msg) = ensure_opencode_config(worktree_path) {
        warn!(event = "core.session.opencode_config_failed", error = %msg);
        eprintln!("Warning: {msg}");
        eprintln!(
            "Add \"file://.opencode/plugins/kild-status.ts\" to the plugins array in opencode.json manually."
        );
    }
}

/// Build the command, args, env vars, and login shell flag for a daemon PTY create request.
///
/// Both `create_session` and `open_session` need to parse the agent command string
/// and collect environment variables for the daemon. This helper centralises that logic.
///
/// Two strategies based on agent type:
/// - **Bare shell** (`agent_name == "shell"`): Sets `use_login_shell = true` so the daemon
///   uses `CommandBuilder::new_default_prog()` for a native login shell with profile sourcing.
/// - **Agents**: Wraps in `$SHELL -lc 'exec <command>'` so profile files are sourced
///   before the agent starts, providing full PATH and environment. The `exec` replaces
///   the wrapper shell with the agent for clean process tracking.
///
/// The `session_id` is used to set up tmux shim environment variables so that agents
/// running inside daemon PTYs see a `$TMUX` environment and can use pane-based workflows.
///
/// The `branch` is used to inject `KILD_SESSION_BRANCH` for agents like Codex that need
/// to report their status back to KILD via notify hooks.
#[allow(clippy::type_complexity)]
pub(super) fn build_daemon_create_request(
    agent_command: &str,
    agent_name: &str,
    session_id: &str,
    task_list_id: Option<&str>,
    branch: &str,
) -> Result<(String, Vec<String>, Vec<(String, String)>, bool), SessionError> {
    let use_login_shell = agent_name == "shell";

    let (cmd, cmd_args) = if use_login_shell {
        // For bare shell: command/args are ignored by new_default_prog(),
        // but we still pass them for logging purposes.
        (agent_command.to_string(), vec![])
    } else {
        // For agents: validate command is non-empty, then wrap in login shell.
        // sh -lc 'exec claude --flags' ensures profile files are sourced.
        if agent_command.split_whitespace().next().is_none() {
            return Err(SessionError::DaemonError {
                message: format!(
                    "Empty command string for agent '{}'. Check agent configuration.",
                    agent_name
                ),
            });
        }
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let escaped = agent_command.replace('\'', "'\\''");
        (shell, vec!["-lc".to_string(), format!("exec {}", escaped)])
    };

    let mut env_vars = Vec::new();
    for key in &["PATH", "HOME", "SHELL", "USER", "LANG", "TERM"] {
        if let Ok(val) = std::env::var(key) {
            env_vars.push((key.to_string(), val));
        }
    }

    // tmux shim environment for daemon sessions
    let paths = KildPaths::resolve().map_err(|e| SessionError::DaemonError {
        message: format!("{} — cannot configure tmux shim PATH", e),
    })?;
    let shim_bin_dir = paths.bin_dir();

    // Prepend shim dir to PATH so our tmux shim is found first.
    // NOTE: For login shells on macOS, /etc/zprofile runs path_helper which
    // reconstructs PATH and may push this to the end. The ZDOTDIR wrapper
    // below re-prepends it after all profile scripts have run.
    if let Some(path_entry) = env_vars.iter_mut().find(|(k, _)| k == "PATH") {
        path_entry.1 = format!("{}:{}", shim_bin_dir.display(), path_entry.1);
    } else if let Ok(system_path) = std::env::var("PATH") {
        env_vars.push((
            "PATH".to_string(),
            format!("{}:{}", shim_bin_dir.display(), system_path),
        ));
    }

    // Create a ZDOTDIR wrapper so that ~/.kild/bin is prepended to PATH
    // AFTER login shell profile scripts run (macOS path_helper in /etc/zprofile
    // reconstructs PATH and drops our prepended entry).
    let zdotdir = paths.shim_zdotdir(session_id);
    if let Err(e) = create_zdotdir_wrapper(&zdotdir, &shim_bin_dir) {
        warn!(
            event = "core.session.zdotdir_setup_failed",
            session_id = session_id,
            error = %e,
        );
        eprintln!(
            "Warning: Failed to set up shell PATH wrapper: {}. \
             The tmux shim may not be found by agents (macOS path_helper can reorder PATH).",
            e
        );
    } else {
        env_vars.push(("ZDOTDIR".to_string(), zdotdir.display().to_string()));
    }

    // $TMUX triggers Claude Code's tmux pane backend (auto mode)
    let daemon_sock = crate::daemon::socket_path();
    env_vars.push((
        "TMUX".to_string(),
        format!("{},{},0", daemon_sock.display(), std::process::id()),
    ));

    // $TMUX_PANE identifies the leader's own pane
    env_vars.push(("TMUX_PANE".to_string(), "%0".to_string()));

    // $KILD_SHIM_SESSION tells the shim where to find its state
    env_vars.push(("KILD_SHIM_SESSION".to_string(), session_id.to_string()));

    // $CLAUDE_CODE_TASK_LIST_ID for task list persistence across sessions
    if let Some(tlid) = task_list_id {
        let task_env = agents::resume::task_list_env_vars(agent_name, tlid);
        env_vars.extend(task_env);
    }

    // $KILD_SESSION_BRANCH for Codex notify hook status reporting
    let codex_env = agents::resume::codex_env_vars(agent_name, branch);
    env_vars.extend(codex_env);

    // $KILD_SESSION_BRANCH for Claude Code status hook reporting
    let claude_env = agents::resume::claude_env_vars(agent_name, branch);
    env_vars.extend(claude_env);

    Ok((cmd, cmd_args, env_vars, use_login_shell))
}

/// Create a ZDOTDIR wrapper that re-prepends `~/.kild/bin` to PATH.
///
/// On macOS, login shells source `/etc/zprofile` which runs `path_helper`,
/// reconstructing PATH from `/etc/paths` and dropping any prepended entries.
/// This wrapper sources the user's real `~/.zshrc` then prepends our shim dir,
/// ensuring `~/.kild/bin/tmux` is always found first.
fn create_zdotdir_wrapper(
    zdotdir: &std::path::Path,
    shim_bin_dir: &std::path::Path,
) -> Result<(), String> {
    std::fs::create_dir_all(zdotdir).map_err(|e| format!("failed to create zdotdir: {}", e))?;

    // .zshenv runs before .zprofile — we need .zshrc which runs after.
    // But we also need .zshenv to reset ZDOTDIR so the user's own .zshenv
    // and .zshrc are sourced from their real home directory.
    // zsh dotfile load order: .zshenv → .zprofile (login) → .zshrc (interactive)
    // ZDOTDIR must stay set throughout so zsh reads ALL our wrappers.
    // Each wrapper sources the user's real file from $HOME.
    // .zshrc (last) unsets ZDOTDIR so nested/child shells behave normally.

    let zshenv_content = r#"# KILD shim — auto-generated, do not edit.
# Source user's real .zshenv if it exists.
[[ -f "$HOME/.zshenv" ]] && source "$HOME/.zshenv"
"#;

    let zprofile_content = r#"# KILD shim — auto-generated, do not edit.
# Source user's real .zprofile if it exists.
[[ -f "$HOME/.zprofile" ]] && source "$HOME/.zprofile"
"#;

    let zshrc_content = format!(
        r#"# KILD shim — auto-generated, do not edit.
# Source user's real .zshrc if it exists.
[[ -f "$HOME/.zshrc" ]] && source "$HOME/.zshrc"

# Re-prepend shim bin dir to PATH (macOS path_helper may have reordered it).
export PATH="{shim_bin}:$PATH"

# Reset ZDOTDIR so child shells use the user's real dotfiles.
unset ZDOTDIR
"#,
        shim_bin = shim_bin_dir.display(),
    );

    std::fs::write(zdotdir.join(".zshenv"), zshenv_content)
        .map_err(|e| format!("failed to write .zshenv: {}", e))?;
    std::fs::write(zdotdir.join(".zprofile"), zprofile_content)
        .map_err(|e| format!("failed to write .zprofile: {}", e))?;
    std::fs::write(zdotdir.join(".zshrc"), zshrc_content)
        .map_err(|e| format!("failed to write .zshrc: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_daemon_request_agent_wraps_in_login_shell() {
        let (cmd, args, _env, use_login_shell) = build_daemon_create_request(
            "claude --agent --verbose",
            "claude",
            "test-session",
            None,
            "test-branch",
        )
        .unwrap();
        assert!(!use_login_shell, "Agent should not use login shell mode");
        // Agent commands are wrapped in $SHELL -lc 'exec <command>'
        assert!(
            cmd.ends_with("sh") || cmd.ends_with("zsh") || cmd.ends_with("bash"),
            "Command should be a shell, got: {}",
            cmd
        );
        assert_eq!(args.len(), 2, "Should have -lc and the exec command");
        assert_eq!(args[0], "-lc");
        assert!(
            args[1].contains("exec claude --agent --verbose"),
            "Should wrap command with exec, got: {}",
            args[1]
        );
    }

    #[test]
    fn test_build_daemon_request_single_word_agent_wraps_in_login_shell() {
        let (cmd, args, _env, use_login_shell) =
            build_daemon_create_request("claude", "claude", "test-session", None, "test-branch")
                .unwrap();
        assert!(!use_login_shell);
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "-lc");
        assert!(args[1].contains("exec claude"), "got: {}", args[1]);
        assert!(
            cmd.ends_with("sh") || cmd.ends_with("zsh") || cmd.ends_with("bash"),
            "got: {}",
            cmd
        );
    }

    #[test]
    fn test_build_daemon_request_bare_shell_uses_login_shell() {
        let (_cmd, args, _env, use_login_shell) =
            build_daemon_create_request("/bin/zsh", "shell", "test-session", None, "test-branch")
                .unwrap();
        assert!(use_login_shell, "Bare shell should use login shell mode");
        assert!(args.is_empty(), "Login shell mode should have no args");
    }

    #[test]
    fn test_build_daemon_request_empty_command_returns_error() {
        let result = build_daemon_create_request("", "claude", "test-session", None, "test-branch");
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            SessionError::DaemonError { message } => {
                assert!(
                    message.contains("claude"),
                    "Error should mention agent name, got: {}",
                    message
                );
                assert!(
                    message.contains("Empty command"),
                    "Error should mention empty command, got: {}",
                    message
                );
            }
            other => panic!("Expected DaemonError, got: {:?}", other),
        }
    }

    #[test]
    fn test_build_daemon_request_whitespace_only_command_returns_error() {
        let result =
            build_daemon_create_request("   ", "kiro", "test-session", None, "test-branch");
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            SessionError::DaemonError { message } => {
                assert!(message.contains("kiro"));
            }
            other => panic!("Expected DaemonError, got: {:?}", other),
        }
    }

    #[test]
    fn test_build_daemon_request_bare_shell_empty_command_still_works() {
        // Bare shell with empty-ish command: since use_login_shell=true,
        // the command is passed through for logging only (daemon ignores it)
        let result = build_daemon_create_request("", "shell", "test-session", None, "test-branch");
        assert!(result.is_ok(), "Bare shell should accept empty command");
        let (_cmd, _args, _env, use_login_shell) = result.unwrap();
        assert!(use_login_shell);
    }

    #[test]
    fn test_build_daemon_request_agent_escapes_single_quotes() {
        let (_, args, _, _) = build_daemon_create_request(
            "claude --note 'hello world'",
            "claude",
            "test-session",
            None,
            "test-branch",
        )
        .unwrap();
        assert!(
            args[1].contains("exec claude --note"),
            "Should contain the command, got: {}",
            args[1]
        );
    }

    #[test]
    fn test_build_daemon_request_collects_env_vars() {
        let (_cmd, _args, env_vars, _) =
            build_daemon_create_request("claude", "claude", "test-session", None, "test-branch")
                .unwrap();

        // PATH and HOME should always be present in the environment
        let keys: Vec<&str> = env_vars.iter().map(|(k, _)| k.as_str()).collect();
        assert!(
            keys.contains(&"PATH"),
            "Should collect PATH env var, got keys: {:?}",
            keys
        );
        assert!(
            keys.contains(&"HOME"),
            "Should collect HOME env var, got keys: {:?}",
            keys
        );
    }

    #[test]
    fn test_build_daemon_request_includes_shim_env_vars() {
        let (_cmd, _args, env_vars, _) =
            build_daemon_create_request("claude", "claude", "proj_my-branch", None, "my-branch")
                .unwrap();

        let keys: Vec<&str> = env_vars.iter().map(|(k, _)| k.as_str()).collect();

        // Should include tmux shim environment variables
        assert!(
            keys.contains(&"TMUX"),
            "Should set TMUX env var, got keys: {:?}",
            keys
        );
        assert!(
            keys.contains(&"TMUX_PANE"),
            "Should set TMUX_PANE env var, got keys: {:?}",
            keys
        );
        assert!(
            keys.contains(&"KILD_SHIM_SESSION"),
            "Should set KILD_SHIM_SESSION env var, got keys: {:?}",
            keys
        );

        // KILD_SHIM_SESSION should contain the session_id
        let shim_session = env_vars
            .iter()
            .find(|(k, _)| k == "KILD_SHIM_SESSION")
            .map(|(_, v)| v.as_str());
        assert_eq!(shim_session, Some("proj_my-branch"));

        // TMUX_PANE should be %0
        let tmux_pane = env_vars
            .iter()
            .find(|(k, _)| k == "TMUX_PANE")
            .map(|(_, v)| v.as_str());
        assert_eq!(tmux_pane, Some("%0"));

        // PATH should be prepended with shim bin dir
        let path_val = env_vars
            .iter()
            .find(|(k, _)| k == "PATH")
            .map(|(_, v)| v.as_str())
            .unwrap();
        assert!(
            path_val.contains(".kild/bin"),
            "PATH should contain .kild/bin shim dir, got: {}",
            path_val
        );
    }

    #[test]
    fn test_build_daemon_request_includes_task_list_env_var_for_claude() {
        let (_cmd, _args, env_vars, _) = build_daemon_create_request(
            "claude",
            "claude",
            "myproject_my-branch",
            Some("kild-myproject_my-branch"),
            "my-branch",
        )
        .unwrap();

        let task_list_val = env_vars
            .iter()
            .find(|(k, _)| k == "CLAUDE_CODE_TASK_LIST_ID")
            .map(|(_, v)| v.as_str());
        assert_eq!(
            task_list_val,
            Some("kild-myproject_my-branch"),
            "CLAUDE_CODE_TASK_LIST_ID should be set for claude agent"
        );
    }

    #[test]
    fn test_build_daemon_request_no_task_list_env_var_for_non_claude() {
        for (agent_cmd, agent_name) in &[
            ("kiro", "kiro"),
            ("gemini", "gemini"),
            ("amp", "amp"),
            ("opencode", "opencode"),
        ] {
            let (_cmd, _args, env_vars, _) = build_daemon_create_request(
                agent_cmd,
                agent_name,
                "test-session",
                Some("kild-test"),
                "test-branch",
            )
            .unwrap();

            let has_task_list = env_vars
                .iter()
                .any(|(k, _)| k == "CLAUDE_CODE_TASK_LIST_ID");
            assert!(
                !has_task_list,
                "CLAUDE_CODE_TASK_LIST_ID should not be set for agent '{}'",
                agent_name
            );
        }
    }

    #[test]
    fn test_compute_spawn_id_produces_unique_ids() {
        let session_id = "myproject_feature-auth";
        let id_0 = compute_spawn_id(session_id, 0);
        let id_1 = compute_spawn_id(session_id, 1);
        let id_2 = compute_spawn_id(session_id, 2);
        assert_eq!(id_0, "myproject_feature-auth_0");
        assert_eq!(id_1, "myproject_feature-auth_1");
        assert_eq!(id_2, "myproject_feature-auth_2");
        assert_ne!(id_0, id_1);
        assert_ne!(id_1, id_2);
    }

    #[test]
    fn test_build_daemon_request_no_task_list_env_var_when_none() {
        let (_cmd, _args, env_vars, _) =
            build_daemon_create_request("claude", "claude", "test-session", None, "test-branch")
                .unwrap();

        let has_task_list = env_vars
            .iter()
            .any(|(k, _)| k == "CLAUDE_CODE_TASK_LIST_ID");
        assert!(
            !has_task_list,
            "CLAUDE_CODE_TASK_LIST_ID should not be set when task_list_id is None"
        );
    }

    #[test]
    fn test_build_daemon_request_includes_codex_env_vars() {
        let (_cmd, _args, env_vars, _) =
            build_daemon_create_request("codex", "codex", "test-session", None, "my-feature")
                .unwrap();

        let branch_val = env_vars
            .iter()
            .find(|(k, _)| k == "KILD_SESSION_BRANCH")
            .map(|(_, v)| v.as_str());
        assert_eq!(
            branch_val,
            Some("my-feature"),
            "KILD_SESSION_BRANCH should be set for codex agent"
        );
    }

    #[test]
    fn test_build_daemon_request_includes_claude_env_vars() {
        let (_cmd, _args, env_vars, _) =
            build_daemon_create_request("claude", "claude", "test-session", None, "my-feature")
                .unwrap();

        let branch_val = env_vars
            .iter()
            .find(|(k, _)| k == "KILD_SESSION_BRANCH")
            .map(|(_, v)| v.as_str());
        assert_eq!(
            branch_val,
            Some("my-feature"),
            "KILD_SESSION_BRANCH should be set for claude agent"
        );
    }

    #[test]
    fn test_ensure_codex_notify_hook_creates_script() {
        use std::fs;

        let temp_home =
            std::env::temp_dir().join(format!("kild_test_codex_hook_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_home);
        let hook_path = temp_home.join(".kild").join("hooks").join("codex-notify");

        let result =
            ensure_codex_notify_hook_with_paths(&KildPaths::from_dir(temp_home.join(".kild")));
        assert!(result.is_ok(), "Hook install should succeed: {:?}", result);
        assert!(hook_path.exists(), "Hook script should exist");

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(
            content.starts_with("#!/bin/sh"),
            "Script should have shebang"
        );
        assert!(
            content.contains("agent-turn-complete"),
            "Script should handle agent-turn-complete"
        );
        assert!(
            content.contains("approval-requested"),
            "Script should handle approval-requested"
        );
        assert!(
            content.contains("kild agent-status --self idle --notify"),
            "Script should call kild agent-status for idle"
        );
        assert!(
            content.contains("kild agent-status --self waiting --notify"),
            "Script should call kild agent-status for waiting"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&hook_path).unwrap().permissions().mode();
            assert!(
                mode & 0o111 != 0,
                "Script should be executable, mode: {:o}",
                mode
            );
        }

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_codex_notify_hook_idempotent() {
        use std::fs;

        let temp_home =
            std::env::temp_dir().join(format!("kild_test_codex_hook_idem_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_home);
        let hook_path = temp_home.join(".kild").join("hooks").join("codex-notify");

        // First call creates the script
        let result =
            ensure_codex_notify_hook_with_paths(&KildPaths::from_dir(temp_home.join(".kild")));
        assert!(result.is_ok());
        let content1 = fs::read_to_string(&hook_path).unwrap();

        // Second call should succeed without changing content
        let result =
            ensure_codex_notify_hook_with_paths(&KildPaths::from_dir(temp_home.join(".kild")));
        assert!(result.is_ok());
        let content2 = fs::read_to_string(&hook_path).unwrap();
        assert_eq!(
            content1, content2,
            "Content should not change on second call"
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_codex_config_patches_empty_config() {
        use std::fs;

        let temp_home =
            std::env::temp_dir().join(format!("kild_test_codex_cfg_empty_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_home);
        let codex_dir = temp_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("config.toml"), "").unwrap();

        let result = ensure_codex_config_with_home(
            &temp_home,
            &KildPaths::from_dir(temp_home.join(".kild")),
        );
        assert!(result.is_ok(), "Config patch should succeed: {:?}", result);

        let content = fs::read_to_string(codex_dir.join("config.toml")).unwrap();
        assert!(
            content.contains("notify = [\""),
            "Config should contain notify setting, got: {}",
            content
        );
        assert!(
            content.contains("codex-notify"),
            "Config should reference codex-notify hook, got: {}",
            content
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_codex_config_preserves_existing_notify() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_codex_cfg_existing_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);
        let codex_dir = temp_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(
            codex_dir.join("config.toml"),
            "notify = [\"my-custom-program\"]\n",
        )
        .unwrap();

        let result = ensure_codex_config_with_home(
            &temp_home,
            &KildPaths::from_dir(temp_home.join(".kild")),
        );
        assert!(result.is_ok());

        let content = fs::read_to_string(codex_dir.join("config.toml")).unwrap();
        assert!(
            content.contains("my-custom-program"),
            "Custom notify should be preserved"
        );
        assert!(
            !content.contains("codex-notify"),
            "Should NOT overwrite user's custom notify config"
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_codex_config_patches_empty_notify_array() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_codex_cfg_empty_arr_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);
        let codex_dir = temp_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("config.toml"), "notify = []\n").unwrap();

        let result = ensure_codex_config_with_home(
            &temp_home,
            &KildPaths::from_dir(temp_home.join(".kild")),
        );
        assert!(result.is_ok());

        let content = fs::read_to_string(codex_dir.join("config.toml")).unwrap();
        assert!(
            content.contains("codex-notify"),
            "Empty notify array should be patched, got: {}",
            content
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_codex_config_creates_new_config() {
        use std::fs;

        let temp_home =
            std::env::temp_dir().join(format!("kild_test_codex_cfg_new_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_home);
        // Don't create .codex dir — it shouldn't exist yet

        let result = ensure_codex_config_with_home(
            &temp_home,
            &KildPaths::from_dir(temp_home.join(".kild")),
        );
        assert!(
            result.is_ok(),
            "Should create config from scratch: {:?}",
            result
        );

        let config_path = temp_home.join(".codex").join("config.toml");
        assert!(config_path.exists(), "Config file should be created");

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(
            content.contains("notify = [\""),
            "New config should contain notify, got: {}",
            content
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_codex_config_preserves_existing_content() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_codex_cfg_preserve_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);
        let codex_dir = temp_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(
            codex_dir.join("config.toml"),
            "[model]\nprovider = \"openai\"\n",
        )
        .unwrap();

        let result = ensure_codex_config_with_home(
            &temp_home,
            &KildPaths::from_dir(temp_home.join(".kild")),
        );
        assert!(result.is_ok());

        let content = fs::read_to_string(codex_dir.join("config.toml")).unwrap();
        assert!(
            content.contains("[model]"),
            "Existing content should be preserved"
        );
        assert!(
            content.contains("provider = \"openai\""),
            "Existing settings should be preserved"
        );
        assert!(content.contains("codex-notify"), "notify should be added");

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_codex_config_rejects_malformed_toml() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_codex_cfg_malformed_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);
        let codex_dir = temp_home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("config.toml"), "[invalid toml syntax\n").unwrap();

        let result = ensure_codex_config_with_home(
            &temp_home,
            &KildPaths::from_dir(temp_home.join(".kild")),
        );
        assert!(result.is_err(), "Should fail on malformed TOML");

        let err = result.unwrap_err();
        assert!(
            err.contains("failed to parse"),
            "Error should mention parse failure, got: {}",
            err
        );
        assert!(
            err.contains("fix TOML syntax"),
            "Error should suggest fixing TOML syntax, got: {}",
            err
        );

        // Verify the file was NOT modified
        let content = fs::read_to_string(codex_dir.join("config.toml")).unwrap();
        assert_eq!(
            content, "[invalid toml syntax\n",
            "Malformed file should not be modified"
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    // --- OpenCode integration tests ---

    #[test]
    fn test_ensure_opencode_plugin_creates_ts_file() {
        use std::fs;

        let temp_dir =
            std::env::temp_dir().join(format!("kild_test_opencode_plugin_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);

        let result = ensure_opencode_plugin_in_worktree(&temp_dir);
        assert!(
            result.is_ok(),
            "Plugin install should succeed: {:?}",
            result
        );

        let plugin_path = temp_dir.join(".opencode/plugins/kild-status.ts");
        assert!(plugin_path.exists(), "Plugin file should exist");

        let content = fs::read_to_string(&plugin_path).unwrap();
        assert!(
            content.contains("@opencode-ai/plugin"),
            "Plugin should import from @opencode-ai/plugin"
        );
        assert!(
            content.contains("kild agent-status --self"),
            "Plugin should call kild agent-status"
        );
        assert!(
            content.contains(".quiet().nothrow()"),
            "Plugin should use .quiet().nothrow()"
        );
        assert!(
            content.contains("session.created"),
            "Plugin should handle session.created"
        );
        assert!(
            content.contains("session.idle"),
            "Plugin should handle session.idle"
        );
        assert!(
            content.contains("session.error"),
            "Plugin should handle session.error"
        );
        assert!(
            content.contains("permission.ask"),
            "Plugin should handle permission.ask"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_opencode_plugin_idempotent() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!(
            "kild_test_opencode_plugin_idem_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);

        let result = ensure_opencode_plugin_in_worktree(&temp_dir);
        assert!(result.is_ok());
        let plugin_path = temp_dir.join(".opencode/plugins/kild-status.ts");
        let content1 = fs::read_to_string(&plugin_path).unwrap();

        // Second call should succeed without changing content
        let result = ensure_opencode_plugin_in_worktree(&temp_dir);
        assert!(result.is_ok());
        let content2 = fs::read_to_string(&plugin_path).unwrap();
        assert_eq!(
            content1, content2,
            "Content should not change on second call"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_opencode_package_json_creates_file() {
        use std::fs;

        let temp_dir =
            std::env::temp_dir().join(format!("kild_test_opencode_pkg_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);

        let result = ensure_opencode_package_json(&temp_dir);
        assert!(
            result.is_ok(),
            "Package.json creation should succeed: {:?}",
            result
        );

        let pkg_path = temp_dir.join(".opencode/package.json");
        assert!(pkg_path.exists(), "package.json should exist");

        let content = fs::read_to_string(&pkg_path).unwrap();
        assert!(
            content.contains("@opencode-ai/plugin"),
            "package.json should contain @opencode-ai/plugin dependency"
        );
        assert!(
            content.contains("\"private\": true"),
            "package.json should be private"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_opencode_package_json_idempotent() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!(
            "kild_test_opencode_pkg_idem_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);

        let result = ensure_opencode_package_json(&temp_dir);
        assert!(result.is_ok());
        let pkg_path = temp_dir.join(".opencode/package.json");
        let content1 = fs::read_to_string(&pkg_path).unwrap();

        // Second call should skip (contains dependency)
        let result = ensure_opencode_package_json(&temp_dir);
        assert!(result.is_ok());
        let content2 = fs::read_to_string(&pkg_path).unwrap();
        assert_eq!(
            content1, content2,
            "Content should not change on second call"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_opencode_config_creates_new() {
        use std::fs;

        let temp_dir =
            std::env::temp_dir().join(format!("kild_test_opencode_cfg_new_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let result = ensure_opencode_config(&temp_dir);
        assert!(
            result.is_ok(),
            "Config creation should succeed: {:?}",
            result
        );

        let config_path = temp_dir.join("opencode.json");
        assert!(config_path.exists(), "opencode.json should be created");

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(
            content.contains("kild-status.ts"),
            "Config should reference kild-status.ts plugin, got: {}",
            content
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_opencode_config_patches_existing() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!(
            "kild_test_opencode_cfg_patch_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Create existing opencode.json without plugins
        fs::write(temp_dir.join("opencode.json"), "{\"model\": \"gpt-4o\"}\n").unwrap();

        let result = ensure_opencode_config(&temp_dir);
        assert!(result.is_ok(), "Config patch should succeed: {:?}", result);

        let content = fs::read_to_string(temp_dir.join("opencode.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            parsed["model"], "gpt-4o",
            "Existing config should be preserved"
        );
        assert!(
            parsed["plugins"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_str().unwrap().contains("kild-status.ts")),
            "Plugin should be added"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_opencode_config_preserves_existing_plugins() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!(
            "kild_test_opencode_cfg_preserve_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Create existing opencode.json with other plugins
        fs::write(
            temp_dir.join("opencode.json"),
            "{\"plugins\": [\"file://my-plugin.ts\"]}\n",
        )
        .unwrap();

        let result = ensure_opencode_config(&temp_dir);
        assert!(result.is_ok());

        let content = fs::read_to_string(temp_dir.join("opencode.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let plugins = parsed["plugins"].as_array().unwrap();
        assert_eq!(plugins.len(), 2, "Should have both existing and new plugin");
        assert!(
            plugins
                .iter()
                .any(|v| v.as_str() == Some("file://my-plugin.ts")),
            "Existing plugin should be preserved"
        );
        assert!(
            plugins
                .iter()
                .any(|v| v.as_str().unwrap().contains("kild-status.ts")),
            "New plugin should be added"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_opencode_config_already_configured() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!(
            "kild_test_opencode_cfg_exists_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Create config with plugin already present
        let config = serde_json::json!({
            "plugins": ["file://.opencode/plugins/kild-status.ts"]
        });
        fs::write(
            temp_dir.join("opencode.json"),
            serde_json::to_string_pretty(&config).unwrap(),
        )
        .unwrap();

        let content_before = fs::read_to_string(temp_dir.join("opencode.json")).unwrap();

        let result = ensure_opencode_config(&temp_dir);
        assert!(result.is_ok());

        let content_after = fs::read_to_string(temp_dir.join("opencode.json")).unwrap();
        assert_eq!(
            content_before, content_after,
            "Config should not change when plugin already configured"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_opencode_config_rejects_malformed_json() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!(
            "kild_test_opencode_cfg_malformed_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        fs::write(temp_dir.join("opencode.json"), "{invalid json\n").unwrap();

        let result = ensure_opencode_config(&temp_dir);
        assert!(result.is_err(), "Should fail on malformed JSON");

        let err = result.unwrap_err();
        assert!(
            err.contains("failed to parse"),
            "Error should mention parse failure, got: {}",
            err
        );

        // Verify the file was NOT modified
        let content = fs::read_to_string(temp_dir.join("opencode.json")).unwrap();
        assert_eq!(
            content, "{invalid json\n",
            "Malformed file should not be modified"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_opencode_config_rejects_non_array_plugins_field() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!(
            "kild_test_opencode_cfg_bad_plugins_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        fs::write(
            temp_dir.join("opencode.json"),
            "{\"plugins\": \"not-an-array\"}\n",
        )
        .unwrap();

        let result = ensure_opencode_config(&temp_dir);
        assert!(result.is_err(), "Should reject non-array plugins field");

        let err = result.unwrap_err();
        assert!(
            err.contains("not an array"),
            "Error should mention 'not an array', got: {}",
            err
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_opencode_package_json_preserves_existing_fields() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!(
            "kild_test_opencode_pkg_preserve_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join(".opencode")).unwrap();

        // Create existing package.json with user content
        fs::write(
            temp_dir.join(".opencode/package.json"),
            r#"{
  "name": "my-custom-name",
  "version": "1.0.0",
  "dependencies": {
    "other-package": "^2.0.0"
  },
  "scripts": {
    "test": "bun test"
  }
}
"#,
        )
        .unwrap();

        let result = ensure_opencode_package_json(&temp_dir);
        assert!(
            result.is_ok(),
            "Should merge dependency into existing file: {:?}",
            result
        );

        let content = fs::read_to_string(temp_dir.join(".opencode/package.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(
            parsed["name"], "my-custom-name",
            "Custom name should be preserved"
        );
        assert_eq!(parsed["version"], "1.0.0", "Version should be preserved");
        assert_eq!(
            parsed["scripts"]["test"], "bun test",
            "Scripts should be preserved"
        );

        let deps = parsed["dependencies"].as_object().unwrap();
        assert_eq!(deps.len(), 2, "Both dependencies should exist");
        assert_eq!(
            deps["other-package"], "^2.0.0",
            "Existing dependency should be preserved"
        );
        assert_eq!(
            deps["@opencode-ai/plugin"], "latest",
            "New dependency should be added"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_opencode_package_json_rejects_malformed_json() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!(
            "kild_test_opencode_pkg_malformed_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join(".opencode")).unwrap();

        fs::write(temp_dir.join(".opencode/package.json"), "{invalid json\n").unwrap();

        let result = ensure_opencode_package_json(&temp_dir);
        assert!(result.is_err(), "Should fail on malformed JSON");

        let err = result.unwrap_err();
        assert!(
            err.contains("failed to parse"),
            "Error should mention parse failure, got: {}",
            err
        );

        // Verify the file was NOT modified
        let content = fs::read_to_string(temp_dir.join(".opencode/package.json")).unwrap();
        assert_eq!(
            content, "{invalid json\n",
            "Malformed file should not be modified"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    // --- Claude Code integration tests ---

    #[test]
    fn test_ensure_claude_status_hook_creates_script() {
        use std::fs;

        let temp_home =
            std::env::temp_dir().join(format!("kild_test_claude_hook_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_home);
        let hook_path = temp_home.join(".kild").join("hooks").join("claude-status");

        let result =
            ensure_claude_status_hook_with_paths(&KildPaths::from_dir(temp_home.join(".kild")));
        assert!(result.is_ok(), "Hook install should succeed: {:?}", result);
        assert!(hook_path.exists(), "Hook script should exist");

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(
            content.starts_with("#!/bin/sh"),
            "Script should have shebang"
        );
        assert!(
            content.contains("hook_event_name"),
            "Script should parse hook_event_name from JSON"
        );
        assert!(
            content.contains("Stop|SubagentStop|TeammateIdle|TaskCompleted"),
            "Script should handle Stop, SubagentStop, TeammateIdle, TaskCompleted"
        );
        assert!(
            content.contains("permission_prompt"),
            "Script should handle permission_prompt notification"
        );
        assert!(
            content.contains("idle_prompt"),
            "Script should handle idle_prompt notification"
        );
        assert!(
            content.contains("kild agent-status --self idle --notify"),
            "Script should call kild agent-status for idle"
        );
        assert!(
            content.contains("kild agent-status --self waiting --notify"),
            "Script should call kild agent-status for waiting"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&hook_path).unwrap().permissions().mode();
            assert!(
                mode & 0o111 != 0,
                "Script should be executable, mode: {:o}",
                mode
            );
        }

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_claude_status_hook_idempotent() {
        use std::fs;

        let temp_home =
            std::env::temp_dir().join(format!("kild_test_claude_hook_idem_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_home);
        let hook_path = temp_home.join(".kild").join("hooks").join("claude-status");

        let result =
            ensure_claude_status_hook_with_paths(&KildPaths::from_dir(temp_home.join(".kild")));
        assert!(result.is_ok());
        let content1 = fs::read_to_string(&hook_path).unwrap();

        let result =
            ensure_claude_status_hook_with_paths(&KildPaths::from_dir(temp_home.join(".kild")));
        assert!(result.is_ok());
        let content2 = fs::read_to_string(&hook_path).unwrap();
        assert_eq!(
            content1, content2,
            "Content should not change on second call"
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_claude_settings_creates_new_config() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_claude_settings_new_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);

        let result = ensure_claude_settings_with_home(
            &temp_home,
            &KildPaths::from_dir(temp_home.join(".kild")),
        );
        assert!(
            result.is_ok(),
            "Should create settings from scratch: {:?}",
            result
        );

        let settings_path = temp_home.join(".claude").join("settings.json");
        assert!(settings_path.exists(), "Settings file should be created");

        let content = fs::read_to_string(&settings_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        // Verify all hook events are present
        let hooks = parsed["hooks"].as_object().unwrap();
        assert!(hooks.contains_key("Stop"), "Should have Stop hooks");
        assert!(
            hooks.contains_key("Notification"),
            "Should have Notification hooks"
        );
        assert!(
            hooks.contains_key("SubagentStop"),
            "Should have SubagentStop hooks"
        );
        assert!(
            hooks.contains_key("TeammateIdle"),
            "Should have TeammateIdle hooks"
        );
        assert!(
            hooks.contains_key("TaskCompleted"),
            "Should have TaskCompleted hooks"
        );

        // Verify Notification has matcher
        let notification = parsed["hooks"]["Notification"][0].as_object().unwrap();
        assert_eq!(
            notification["matcher"], "permission_prompt|idle_prompt",
            "Notification should have matcher"
        );

        // Verify hook command points to claude-status
        assert!(
            content.contains("claude-status"),
            "Settings should reference claude-status hook"
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_claude_settings_patches_existing_config() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_claude_settings_patch_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);
        let claude_dir = temp_home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("settings.json"),
            "{\"permissions\": {\"allow\": [\"Bash(*)\"]}, \"enabledPlugins\": [\"my-plugin\"]}\n",
        )
        .unwrap();

        let result = ensure_claude_settings_with_home(
            &temp_home,
            &KildPaths::from_dir(temp_home.join(".kild")),
        );
        assert!(result.is_ok(), "Config patch should succeed: {:?}", result);

        let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        // Existing settings preserved
        assert!(
            parsed["permissions"]["allow"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "Bash(*)"),
            "Existing permissions should be preserved"
        );
        assert!(
            parsed["enabledPlugins"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "my-plugin"),
            "Existing enabledPlugins should be preserved"
        );

        // New hooks added
        assert!(
            parsed["hooks"]["Stop"].is_array(),
            "Stop hooks should be added"
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_claude_settings_preserves_existing_hooks() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_claude_settings_idem_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);
        let claude_dir = temp_home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        // First call creates the config
        let result = ensure_claude_settings_with_home(
            &temp_home,
            &KildPaths::from_dir(temp_home.join(".kild")),
        );
        assert!(result.is_ok());
        let content1 = fs::read_to_string(claude_dir.join("settings.json")).unwrap();

        // Second call should skip (already configured)
        let result = ensure_claude_settings_with_home(
            &temp_home,
            &KildPaths::from_dir(temp_home.join(".kild")),
        );
        assert!(result.is_ok());
        let content2 = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
        assert_eq!(
            content1, content2,
            "Content should not change when already configured"
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_claude_settings_preserves_existing_user_hooks() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_claude_settings_user_hooks_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);
        let claude_dir = temp_home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        // Create settings with existing user hooks
        let existing = serde_json::json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{"type": "command", "command": "/usr/local/bin/my-linter"}]
                }]
            }
        });
        fs::write(
            claude_dir.join("settings.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        let result = ensure_claude_settings_with_home(
            &temp_home,
            &KildPaths::from_dir(temp_home.join(".kild")),
        );
        assert!(result.is_ok());

        let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        // Existing PreToolUse hook preserved
        let pre_tool = parsed["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(
            pre_tool.len(),
            1,
            "Existing PreToolUse hooks should be preserved"
        );
        assert!(
            content.contains("my-linter"),
            "Existing user hook command should be preserved"
        );

        // Our hooks added
        assert!(parsed["hooks"]["Stop"].is_array());

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_claude_settings_handles_malformed_json() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_claude_settings_malformed_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);
        let claude_dir = temp_home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(claude_dir.join("settings.json"), "{invalid json\n").unwrap();

        let result = ensure_claude_settings_with_home(
            &temp_home,
            &KildPaths::from_dir(temp_home.join(".kild")),
        );
        assert!(result.is_err(), "Should fail on malformed JSON");

        let err = result.unwrap_err();
        assert!(
            err.contains("failed to parse"),
            "Error should mention parse failure, got: {}",
            err
        );

        // Verify the file was NOT modified
        let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
        assert_eq!(
            content, "{invalid json\n",
            "Malformed file should not be modified"
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_claude_settings_partial_idempotency() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_claude_settings_partial_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);
        let claude_dir = temp_home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let paths = KildPaths::from_dir(temp_home.join(".kild"));
        let hook_path_str = paths.claude_status_hook().display().to_string();

        // Create settings with our hook on Stop only (partial configuration)
        let existing = serde_json::json!({
            "hooks": {
                "Stop": [{
                    "hooks": [{"type": "command", "command": hook_path_str, "timeout": 5}]
                }]
            }
        });
        fs::write(
            claude_dir.join("settings.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        let result = ensure_claude_settings_with_home(&temp_home, &paths);
        assert!(result.is_ok(), "Should patch missing events: {:?}", result);

        let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let hooks = parsed["hooks"].as_object().unwrap();

        // Stop should NOT be duplicated
        let stop_arr = hooks["Stop"].as_array().unwrap();
        assert_eq!(stop_arr.len(), 1, "Stop hook should not be duplicated");

        // Missing events should be added
        for event in &[
            "SubagentStop",
            "TeammateIdle",
            "TaskCompleted",
            "Notification",
        ] {
            assert!(
                hooks[*event].is_array() && !hooks[*event].as_array().unwrap().is_empty(),
                "{event} hook should be added"
            );
        }

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_claude_settings_rejects_non_array_event() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_claude_settings_bad_type_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);
        let claude_dir = temp_home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        // Create settings with a non-array event value
        let existing = serde_json::json!({
            "hooks": {
                "Stop": "invalid"
            }
        });
        fs::write(
            claude_dir.join("settings.json"),
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        let result = ensure_claude_settings_with_home(
            &temp_home,
            &KildPaths::from_dir(temp_home.join(".kild")),
        );
        assert!(result.is_err(), "Should fail on non-array event value");
        let err = result.unwrap_err();
        assert!(
            err.contains("not an array"),
            "Error should mention type issue, got: {err}"
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_claude_status_hook_script_syntax() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_claude_hook_syntax_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);

        let paths = KildPaths::from_dir(temp_home.join(".kild"));
        let result = ensure_claude_status_hook_with_paths(&paths);
        assert!(result.is_ok());

        let hook_path = paths.claude_status_hook();
        // Validate shell syntax with sh -n (parse without executing)
        let output = std::process::Command::new("sh")
            .arg("-n")
            .arg(&hook_path)
            .output()
            .expect("sh should be available");
        assert!(
            output.status.success(),
            "Hook script should have valid shell syntax: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn test_ensure_claude_settings_hook_structure() {
        use std::fs;

        let temp_home = std::env::temp_dir().join(format!(
            "kild_test_claude_settings_structure_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_home);

        let paths = KildPaths::from_dir(temp_home.join(".kild"));
        let result = ensure_claude_settings_with_home(&temp_home, &paths);
        assert!(result.is_ok());

        let content = fs::read_to_string(temp_home.join(".claude/settings.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        // Verify Stop hook entry structure (no matcher)
        let stop_entries = parsed["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop_entries.len(), 1);
        let stop_entry = stop_entries[0].as_object().unwrap();
        assert!(
            !stop_entry.contains_key("matcher"),
            "Stop should not have matcher"
        );
        let stop_hooks = stop_entry["hooks"].as_array().unwrap();
        assert_eq!(stop_hooks.len(), 1);
        assert_eq!(stop_hooks[0]["type"], "command");
        assert!(
            stop_hooks[0]["command"]
                .as_str()
                .unwrap()
                .ends_with("claude-status")
        );
        assert_eq!(stop_hooks[0]["timeout"], 5);

        // Verify Notification hook entry structure (with matcher)
        let notif_entries = parsed["hooks"]["Notification"].as_array().unwrap();
        assert_eq!(notif_entries.len(), 1);
        let notif_entry = notif_entries[0].as_object().unwrap();
        assert_eq!(notif_entry["matcher"], "permission_prompt|idle_prompt");
        let notif_hooks = notif_entry["hooks"].as_array().unwrap();
        assert_eq!(notif_hooks.len(), 1);
        assert_eq!(notif_hooks[0]["type"], "command");
        assert_eq!(notif_hooks[0]["timeout"], 5);

        let _ = fs::remove_dir_all(&temp_home);
    }
}
