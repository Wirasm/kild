use std::path::Path;

use kild_config::KildConfig;
use kild_paths::KildPaths;
use tracing::{info, warn};

use kild_protocol::DEFAULT_HOOKS_PORT;

/// Resolve the configured hooks port from kild-config.
fn resolve_hooks_port() -> u16 {
    match KildConfig::load_hierarchy() {
        Ok(c) => c.daemon.hooks_port(),
        Err(e) => {
            warn!(
                event = "core.session.integrations.claude.config_load_failed",
                error = %e,
                "Falling back to default hooks port {}",
                DEFAULT_HOOKS_PORT,
            );
            DEFAULT_HOOKS_PORT
        }
    }
}

/// Ensure the Claude Code status hook script is installed at `~/.kild/hooks/claude-status`.
///
/// This script handles events that cannot use HTTP hooks:
/// - **TeammateIdle** / **TaskCompleted**: require exit-code blocking (exit 2), HTTP hooks cannot block these
/// - **Notification**: Claude Code does not support HTTP hooks for this event
///
/// Stop and SubagentStop are handled by the daemon's HTTP hook endpoint instead.
///
/// Always overwrites to pick up updated hook content.
fn ensure_claude_status_hook_with_paths(paths: &KildPaths) -> Result<(), String> {
    let hooks_dir = paths.hooks_dir();
    let hook_path = paths.claude_status_hook();

    std::fs::create_dir_all(&hooks_dir)
        .map_err(|e| format!("failed to create {}: {}", hooks_dir.display(), e))?;

    let script = r#"#!/bin/sh
# KILD Claude Code status hook — auto-generated, do not edit.
# Handles events that cannot use HTTP hooks: TeammateIdle, TaskCompleted, Notification.
# Stop and SubagentStop are handled by the daemon HTTP hook endpoint.
INPUT=$(cat)
BRANCH="${KILD_SESSION_BRANCH:-unknown}"
EVENT=$(echo "$INPUT" | grep -o '"hook_event_name":"[^"]*"' | head -1 | sed 's/"hook_event_name":"//;s/"//')
NTYPE=$(echo "$INPUT" | grep -o '"notification_type":"[^"]*"' | head -1 | sed 's/"notification_type":"//;s/"//')
case "$EVENT" in
  TeammateIdle)
    kild agent-status --self idle --notify
    ;;
  TaskCompleted)
    kild agent-status --self idle --notify
    ;;
  Notification)
    case "$NTYPE" in
      permission_prompt) kild agent-status --self waiting --notify ;;
      idle_prompt)       kild agent-status --self idle --notify ;;
    esac
    ;;
esac
# Brain forwarding for events handled here (TeammateIdle, TaskCompleted, Notification).
# Only forward primary events by default; set KILD_HOOK_VERBOSE=1 for all.
LAST_MSG=$(echo "$INPUT" | grep -o '"transcript_summary":"[^"]*"' | head -1 | sed 's/"transcript_summary":"//;s/"//')
TAG=""
FORWARD=""
case "$EVENT" in
  TeammateIdle)   TAG="teammate.idle";  [ "${KILD_HOOK_VERBOSE:-0}" = "1" ] && FORWARD=1 ;;
  TaskCompleted)  TAG="task.completed"; [ "${KILD_HOOK_VERBOSE:-0}" = "1" ] && FORWARD=1 ;;
  Notification)
    case "$NTYPE" in
      permission_prompt) TAG="agent.waiting"; FORWARD=1 ;;
      idle_prompt)       TAG="agent.idle";    FORWARD=1 ;;
    esac
    ;;
esac
if [ -n "$FORWARD" ]; then
  MSG="[EVENT] $BRANCH $TAG${LAST_MSG:+: $LAST_MSG}"
  if [ "$BRANCH" != "honryu" ] && \
     [ "$BRANCH" != "unknown" ] && \
     kild list --json 2>/dev/null | jq -e '.sessions[] | select(.branch == "honryu" and .status == "active")' > /dev/null 2>&1; then
    kild inject honryu "$MSG" 2>/dev/null || true
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

/// Ensure Claude Code settings.json has KILD hooks configured.
///
/// Patches `~/.claude/settings.json` with:
/// - **HTTP hooks** for Stop and SubagentStop → daemon HTTP endpoint
/// - **Command hooks** for TeammateIdle, TaskCompleted, Notification → shell script
///
/// Preserves all existing settings and hooks.
/// Idempotent: skips if hooks already reference our script/URL.
fn ensure_claude_settings_with_home(home: &Path, paths: &KildPaths) -> Result<(), String> {
    let claude_dir = home.join(".claude");
    let settings_path = claude_dir.join("settings.json");
    let hook_path = paths.claude_status_hook();
    let hook_path_str = hook_path.display().to_string();
    let hooks_port = resolve_hooks_port();
    let hooks_url = format!("http://127.0.0.1:{}/hooks", hooks_port);

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

    // Helper: check if a hook array already contains our command script or HTTP URL.
    let has_our_hook = |entries: &serde_json::Value| -> bool {
        if let Some(arr) = entries.as_array() {
            arr.iter().any(|entry| {
                if let Some(serde_json::Value::Array(hook_list)) = entry.get("hooks") {
                    hook_list.iter().any(|h| {
                        h.get("command").and_then(|c| c.as_str()) == Some(&hook_path_str)
                            || h.get("url").and_then(|u| u.as_str()) == Some(hooks_url.as_str())
                    })
                } else {
                    false
                }
            })
        } else {
            false
        }
    };

    let hooks = settings
        .as_object_mut()
        .ok_or("settings.json root is not an object")?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    let hooks_obj = hooks
        .as_object_mut()
        .ok_or("\"hooks\" field in settings.json is not an object")?;

    let mut added = 0;

    // --- HTTP hooks for Stop and SubagentStop (daemon processes these in Rust) ---
    let http_hook = serde_json::json!({
        "type": "http",
        "url": hooks_url,
        "timeout": 5
    });

    for event in &["Stop", "SubagentStop"] {
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
            "hooks": [http_hook.clone()]
        }));
        added += 1;
    }

    // --- Command hooks for TeammateIdle, TaskCompleted (need exit-code blocking) ---
    let command_hook = serde_json::json!({
        "type": "command",
        "command": hook_path_str,
        "timeout": 10
    });

    for event in &["TeammateIdle", "TaskCompleted"] {
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
            "hooks": [command_hook.clone()]
        }));
        added += 1;
    }

    // --- Notification: command hook with matcher (HTTP not supported for this event) ---
    let notification_entries = hooks_obj
        .entry("Notification")
        .or_insert_with(|| serde_json::json!([]));

    if !has_our_hook(notification_entries) {
        let arr = notification_entries
            .as_array_mut()
            .ok_or("\"Notification\" field in settings.json is not an array")?;
        arr.push(serde_json::json!({
            "matcher": "permission_prompt|idle_prompt",
            "hooks": [command_hook.clone()]
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
        // The script now only handles TeammateIdle, TaskCompleted, Notification
        assert!(
            content.contains("TeammateIdle"),
            "Script should handle TeammateIdle"
        );
        assert!(
            content.contains("TaskCompleted"),
            "Script should handle TaskCompleted"
        );
        assert!(
            content.contains("Notification"),
            "Script should handle Notification"
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
        // Brain forwarding
        assert!(
            content.contains(r#"BRANCH" != "honryu""#),
            "Script must guard against brain injecting into itself"
        );
        assert!(
            content.contains(r#".status == "active""#),
            "Script must check honryu is active"
        );
        // Event tagging
        assert!(
            content.contains("transcript_summary"),
            "Script should extract transcript_summary"
        );
        assert!(
            content.contains("[EVENT] $BRANCH $TAG"),
            "Script should use unified event format"
        );
        assert!(
            content.contains(r#"TAG="teammate.idle""#),
            "TeammateIdle should be tagged"
        );
        assert!(
            content.contains(r#"TAG="task.completed""#),
            "TaskCompleted should be tagged"
        );
        assert!(
            content.contains(r#"TAG="agent.waiting""#),
            "Notification(permission_prompt) should be tagged"
        );
        assert!(
            content.contains(r#"TAG="agent.idle""#),
            "Notification(idle_prompt) should be tagged"
        );
        // Stop and SubagentStop should NOT be in the command hook (moved to HTTP)
        assert!(
            !content.contains("Stop|SubagentStop|TeammateIdle|TaskCompleted"),
            "Old combined case pattern should not exist"
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

        let paths = KildPaths::from_dir(temp_home.join(".kild"));
        let result = ensure_claude_status_hook_with_paths(&paths);
        assert!(result.is_ok());
        let hook_path = temp_home.join(".kild").join("hooks").join("claude-status");

        // Write stale/outdated content to simulate an old hook version
        fs::write(&hook_path, "#!/bin/sh\n# outdated hook\n").unwrap();

        // Second call should overwrite with current content
        let result = ensure_claude_status_hook_with_paths(&paths);
        assert!(result.is_ok());
        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(
            content.contains("TeammateIdle"),
            "Should have been overwritten with current hook content"
        );
        assert!(
            !content.contains("outdated hook"),
            "Stale content should be gone"
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

        let hooks = parsed["hooks"].as_object().unwrap();

        // HTTP hooks for Stop and SubagentStop
        assert!(hooks.contains_key("Stop"), "Should have Stop hooks");
        assert!(
            hooks.contains_key("SubagentStop"),
            "Should have SubagentStop hooks"
        );

        // Command hooks for TeammateIdle, TaskCompleted, Notification
        assert!(
            hooks.contains_key("TeammateIdle"),
            "Should have TeammateIdle hooks"
        );
        assert!(
            hooks.contains_key("TaskCompleted"),
            "Should have TaskCompleted hooks"
        );
        assert!(
            hooks.contains_key("Notification"),
            "Should have Notification hooks"
        );

        // Verify Stop has HTTP hook
        let stop_entries = parsed["hooks"]["Stop"].as_array().unwrap();
        assert!(
            !stop_entries.is_empty(),
            "Stop should have at least one hook entry"
        );

        // Verify HTTP hook type
        let has_http = stop_entries.iter().any(|e| {
            e["hooks"]
                .as_array()
                .is_some_and(|h| h.iter().any(|hook| hook["type"] == "http"))
        });
        assert!(has_http, "Stop should have an HTTP hook");

        // Verify SubagentStop is HTTP type
        let subagent_hooks = &parsed["hooks"]["SubagentStop"][0]["hooks"][0];
        assert_eq!(subagent_hooks["type"], "http");

        // Verify TeammateIdle is command type
        let teammate_hooks = &parsed["hooks"]["TeammateIdle"][0]["hooks"][0];
        assert_eq!(teammate_hooks["type"], "command");

        // Verify Notification has matcher
        let notification = parsed["hooks"]["Notification"][0].as_object().unwrap();
        assert_eq!(
            notification["matcher"], "permission_prompt|idle_prompt",
            "Notification should have matcher"
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

        let result = ensure_claude_settings_with_home(
            &temp_home,
            &KildPaths::from_dir(temp_home.join(".kild")),
        );
        assert!(result.is_ok());
        let content1 = fs::read_to_string(claude_dir.join("settings.json")).unwrap();

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

        let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
        assert_eq!(
            content, "{invalid json\n",
            "Malformed file should not be modified"
        );

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

        // Verify Stop has HTTP hook (no matcher)
        let stop_entries = parsed["hooks"]["Stop"].as_array().unwrap();
        // Find the HTTP hook entry
        let http_entry = stop_entries
            .iter()
            .find(|e| {
                e["hooks"]
                    .as_array()
                    .is_some_and(|h| h.iter().any(|hook| hook["type"] == "http"))
            })
            .expect("Stop should have an HTTP hook entry");
        assert!(
            !http_entry.as_object().unwrap().contains_key("matcher"),
            "Stop HTTP hook should not have matcher"
        );
        let http_hooks = http_entry["hooks"].as_array().unwrap();
        assert_eq!(http_hooks.len(), 1);
        assert_eq!(http_hooks[0]["type"], "http");
        assert!(http_hooks[0]["url"].as_str().unwrap().contains("127.0.0.1"));
        assert_eq!(http_hooks[0]["timeout"], 5);

        // Verify Notification hook entry structure (with matcher)
        let notif_entries = parsed["hooks"]["Notification"].as_array().unwrap();
        assert_eq!(notif_entries.len(), 1);
        let notif_entry = notif_entries[0].as_object().unwrap();
        assert_eq!(notif_entry["matcher"], "permission_prompt|idle_prompt");
        let notif_hooks = notif_entry["hooks"].as_array().unwrap();
        assert_eq!(notif_hooks.len(), 1);
        assert_eq!(notif_hooks[0]["type"], "command");

        let _ = fs::remove_dir_all(&temp_home);
    }
}
