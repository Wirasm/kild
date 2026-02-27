use std::path::Path;

use kild_paths::KildPaths;
use tracing::{info, warn};

/// Ensure the Claude Code status hook script is installed at `~/.kild/hooks/claude-status`.
///
/// This script is registered in Claude Code's `~/.claude/settings.json` for Stop,
/// Notification, SubagentStop, TeammateIdle, and TaskCompleted hooks. It reads JSON
/// from stdin, maps Claude Code events to KILD agent statuses, and calls
/// `kild agent-status --self <status> --notify`.
/// Always overwrites to pick up updated hook content (e.g. new brain-inject block).
/// User edits to `~/.kild/hooks/claude-status` will be replaced on every create/open.
fn ensure_claude_status_hook_with_paths(paths: &KildPaths) -> Result<(), String> {
    let hooks_dir = paths.hooks_dir();
    let hook_path = paths.claude_status_hook();

    std::fs::create_dir_all(&hooks_dir)
        .map_err(|e| format!("failed to create {}: {}", hooks_dir.display(), e))?;

    let script = r#"#!/bin/sh
# KILD Claude Code status hook — auto-generated, do not edit.
# Registered in ~/.claude/settings.json for Stop, Notification, SubagentStop,
# TeammateIdle, and TaskCompleted hooks.
# Maps Claude Code events to KILD agent statuses.
INPUT=$(cat)
BRANCH="${KILD_SESSION_BRANCH:-unknown}"
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
# Inject event into Honryū brain session if it is running.
# Skip if this session IS the brain to prevent self-referential feedback loops.
# Gate file ($KILD_DROPBOX/.idle_sent) deduplicates: only the first idle event
# per task cycle fires an inject. Cleared by `kild inject` when writing a new task.
# Event tagging (#611): events use semantic tags for the brain.
# By default only primary agent events (Stop, Notification) are forwarded.
# Set KILD_HOOK_VERBOSE=1 to forward all events including subagent/teammate noise.
LAST_MSG=$(echo "$INPUT" | grep -o '"transcript_summary":"[^"]*"' | head -1 | sed 's/"transcript_summary":"//;s/"//')
TAG=""
FORWARD=""
SKIP_GATE=""
case "$EVENT" in
  Stop)           TAG="agent.stop";     FORWARD=1 ;;
  SubagentStop)   TAG="subagent.stop";  [ "${KILD_HOOK_VERBOSE:-0}" = "1" ] && FORWARD=1 ;;
  TeammateIdle)   TAG="teammate.idle";  [ "${KILD_HOOK_VERBOSE:-0}" = "1" ] && FORWARD=1 ;;
  TaskCompleted)  TAG="task.completed"; [ "${KILD_HOOK_VERBOSE:-0}" = "1" ] && FORWARD=1 ;;
  Notification)
    case "$NTYPE" in
      permission_prompt) TAG="agent.waiting"; FORWARD=1; SKIP_GATE=1 ;;
      idle_prompt)       TAG="agent.idle";    FORWARD=1 ;;
    esac
    ;;
esac
if [ -n "$FORWARD" ]; then
  MSG="[EVENT] $BRANCH $TAG${LAST_MSG:+: $LAST_MSG}"
  GATE="${KILD_DROPBOX:+$KILD_DROPBOX/.idle_sent}"
  if [ "$BRANCH" != "honryu" ] && \
     [ "$BRANCH" != "unknown" ] && \
     { [ -n "$SKIP_GATE" ] || [ -z "$GATE" ] || [ ! -f "$GATE" ]; } && \
     kild list --json 2>/dev/null | jq -e '.sessions[] | select(.branch == "honryu" and .status == "active")' > /dev/null 2>&1; then
    if kild inject honryu "$MSG"; then
      if [ -z "$SKIP_GATE" ] && [ -n "$GATE" ]; then
        touch "$GATE" || echo "[kild] Warning: failed to write idle gate $GATE" >&2
      fi
    fi
  fi
fi
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(
            content.contains(r#"BRANCH" != "honryu""#),
            "Script must guard against brain injecting into itself (self-loop prevention)"
        );
        assert!(
            content.contains(r#".status == "active""#),
            "Script must check that honryu session is active, not just present"
        );
        assert!(
            content.contains(".idle_sent"),
            "Script must use .idle_sent gate file for deduplication"
        );
        assert!(
            content.contains(r#"[ ! -f "$GATE" ]"#),
            "Script must check gate file before injecting"
        );
        assert!(
            content.contains(r#"touch "$GATE""#),
            "Script must create gate file after successful inject"
        );
        assert!(
            content.contains(r#"[ -z "$GATE" ]"#),
            "Script must allow inject when KILD_DROPBOX is not set (no-dropbox fallback)"
        );

        // Event tagging assertions (#611)
        assert!(
            content.contains("transcript_summary"),
            "Script should extract transcript_summary from JSON payload"
        );
        assert!(
            content.contains("KILD_HOOK_VERBOSE"),
            "Script should support KILD_HOOK_VERBOSE env var"
        );
        assert!(
            content.contains("[EVENT] $BRANCH $TAG"),
            "Script should use unified [EVENT] $BRANCH $TAG format"
        );
        assert!(
            content.contains(r#"TAG="agent.stop""#),
            "Stop events should be tagged agent.stop"
        );
        assert!(
            content.contains(r#"TAG="subagent.stop""#),
            "SubagentStop events should be tagged subagent.stop"
        );
        assert!(
            content.contains(r#"TAG="teammate.idle""#),
            "TeammateIdle events should be tagged teammate.idle"
        );
        assert!(
            content.contains(r#"TAG="task.completed""#),
            "TaskCompleted events should be tagged task.completed"
        );
        assert!(
            content.contains(r#"TAG="agent.waiting""#),
            "Notification(permission_prompt) should be tagged agent.waiting"
        );
        assert!(
            content.contains(r#"TAG="agent.idle""#),
            "Notification(idle_prompt) should be tagged agent.idle"
        );
        assert!(
            content.contains("SKIP_GATE"),
            "Script should bypass gate for permission_prompt events"
        );
        // Default forwarding: SubagentStop, TeammateIdle, TaskCompleted are verbose-only
        // (gated on KILD_HOOK_VERBOSE=1), while Stop and Notification forward unconditionally
        let forward_block = content
            .split("TAG=\"\"")
            .nth(1)
            .expect("Should have TAG initialization");
        assert!(
            forward_block.contains("SubagentStop") && forward_block.contains("TeammateIdle"),
            "SubagentStop and TeammateIdle should appear in the forward block with verbose gating"
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
    fn test_ensure_claude_status_hook_always_overwrites() {
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
