use std::path::Path;

use kild_paths::KildPaths;
use tracing::{debug, info, warn};

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

        // Check top-level notify first. Also check raw content for the hook path to handle
        // cases where a previous append landed under a table (e.g. [notice.model_migrations])
        // rather than at the top level.
        let top_level_ok = matches!(
            parsed.get("notify"),
            Some(toml::Value::Array(arr)) if !arr.is_empty()
        );
        let raw_has_hook = content.contains(hook_path_str.as_str());
        if top_level_ok || raw_has_hook {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
