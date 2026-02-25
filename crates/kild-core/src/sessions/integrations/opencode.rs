use std::path::Path;

use tracing::{debug, info, warn};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
