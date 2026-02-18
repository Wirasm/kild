//! Keybindings configuration for the KILD UI.
//!
//! Loaded from `~/.kild/keybindings.toml` (user-level) and
//! `./.kild/keybindings.toml` (project-level), following the same hierarchy
//! as `config.toml`. Parse errors warn and fall back to defaults so invalid
//! bindings never block app startup.

use serde::{Deserialize, Serialize};
use tracing::warn;

/// Top-level keybindings struct loaded from `keybindings.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Keybindings {
    /// `[terminal]` section — shortcuts used inside the embedded terminal.
    pub terminal: TerminalKeybindings,
    /// `[navigation]` section — shortcuts for kild and workspace navigation.
    pub navigation: NavigationKeybindings,
}

impl Keybindings {
    /// Merge two keybinding configs. `override_config` values take precedence.
    pub fn merge(base: &Self, override_config: &Self) -> Self {
        Self {
            terminal: TerminalKeybindings::merge(&base.terminal, &override_config.terminal),
            navigation: NavigationKeybindings::merge(&base.navigation, &override_config.navigation),
        }
    }
}

/// `[terminal]` section of `keybindings.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalKeybindings {
    /// Move keyboard focus from the terminal pane to the sidebar.
    /// Default: `"ctrl+escape"`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_escape: Option<String>,

    /// Copy selection to clipboard, or send SIGINT if no selection.
    /// Default: `"cmd+c"`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy: Option<String>,

    /// Paste clipboard contents to the terminal.
    /// Default: `"cmd+v"`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paste: Option<String>,
}

impl TerminalKeybindings {
    /// Returns the focus_escape binding, defaulting to `"ctrl+escape"`.
    pub fn focus_escape(&self) -> &str {
        self.focus_escape.as_deref().unwrap_or("ctrl+escape")
    }

    /// Returns the copy binding, defaulting to `"cmd+c"`.
    pub fn copy(&self) -> &str {
        self.copy.as_deref().unwrap_or("cmd+c")
    }

    /// Returns the paste binding, defaulting to `"cmd+v"`.
    pub fn paste(&self) -> &str {
        self.paste.as_deref().unwrap_or("cmd+v")
    }

    /// Merge two terminal keybinding configs. Override takes precedence for set fields.
    pub fn merge(base: &Self, override_config: &Self) -> Self {
        Self {
            focus_escape: override_config
                .focus_escape
                .clone()
                .or(base.focus_escape.clone()),
            copy: override_config.copy.clone().or(base.copy.clone()),
            paste: override_config.paste.clone().or(base.paste.clone()),
        }
    }
}

/// `[navigation]` section of `keybindings.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NavigationKeybindings {
    /// Navigate to the next kild in the sidebar list.
    /// Default: `"cmd+j"`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_kild: Option<String>,

    /// Navigate to the previous kild in the sidebar list.
    /// Default: `"cmd+k"`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_kild: Option<String>,

    /// Modifier key(s) for index jumping (modifier+1-9).
    /// Valid values: `"ctrl"`, `"alt"`, `"cmd+shift"`
    /// Default: `"ctrl"`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump_modifier: Option<String>,

    /// Toggle between Control and Dashboard view.
    /// Default: `"cmd+d"`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle_view: Option<String>,

    /// Cycle to the next workspace.
    /// Default: `"cmd+shift+]"`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_workspace: Option<String>,

    /// Cycle to the previous workspace.
    /// Default: `"cmd+shift+["`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_workspace: Option<String>,
}

impl NavigationKeybindings {
    /// Returns the next_kild binding, defaulting to `"cmd+j"`.
    pub fn next_kild(&self) -> &str {
        self.next_kild.as_deref().unwrap_or("cmd+j")
    }

    /// Returns the prev_kild binding, defaulting to `"cmd+k"`.
    pub fn prev_kild(&self) -> &str {
        self.prev_kild.as_deref().unwrap_or("cmd+k")
    }

    /// Returns the jump_modifier binding, defaulting to `"ctrl"`.
    pub fn jump_modifier(&self) -> &str {
        self.jump_modifier.as_deref().unwrap_or("ctrl")
    }

    /// Returns the toggle_view binding, defaulting to `"cmd+d"`.
    pub fn toggle_view(&self) -> &str {
        self.toggle_view.as_deref().unwrap_or("cmd+d")
    }

    /// Returns the next_workspace binding, defaulting to `"cmd+shift+]"`.
    pub fn next_workspace(&self) -> &str {
        self.next_workspace.as_deref().unwrap_or("cmd+shift+]")
    }

    /// Returns the prev_workspace binding, defaulting to `"cmd+shift+["`.
    pub fn prev_workspace(&self) -> &str {
        self.prev_workspace.as_deref().unwrap_or("cmd+shift+[")
    }

    /// Merge two navigation keybinding configs. Override takes precedence for set fields.
    pub fn merge(base: &Self, override_config: &Self) -> Self {
        Self {
            next_kild: override_config.next_kild.clone().or(base.next_kild.clone()),
            prev_kild: override_config.prev_kild.clone().or(base.prev_kild.clone()),
            jump_modifier: override_config
                .jump_modifier
                .clone()
                .or(base.jump_modifier.clone()),
            toggle_view: override_config
                .toggle_view
                .clone()
                .or(base.toggle_view.clone()),
            next_workspace: override_config
                .next_workspace
                .clone()
                .or(base.next_workspace.clone()),
            prev_workspace: override_config
                .prev_workspace
                .clone()
                .or(base.prev_workspace.clone()),
        }
    }
}

/// Load keybindings from the user/project hierarchy.
///
/// Returns `Keybindings::default()` if no files are found or on any error.
/// File-not-found errors are silent. Parse errors emit a `warn!` and fall back.
///
/// This intentionally returns `Keybindings` (not `Result`) — keybinding errors
/// must never block app startup.
pub fn load_hierarchy() -> Keybindings {
    let mut keybindings = Keybindings::default();

    // Load user keybindings (~/.kild/keybindings.toml)
    match kild_paths::KildPaths::resolve() {
        Ok(paths) => {
            if let Some(user_kb) = try_load_keybindings_file(&paths.user_keybindings()) {
                keybindings = Keybindings::merge(&keybindings, &user_kb);
            }
        }
        Err(e) => {
            warn!(
                event = "core.keybindings.paths_resolve_failed",
                error = %e,
                "Could not determine home directory; user keybindings not loaded"
            );
        }
    }

    // Load project keybindings (./.kild/keybindings.toml)
    match std::env::current_dir() {
        Ok(project_root) => {
            let path = kild_paths::KildPaths::project_keybindings(&project_root);
            if let Some(project_kb) = try_load_keybindings_file(&path) {
                keybindings = Keybindings::merge(&keybindings, &project_kb);
            }
        }
        Err(e) => {
            warn!(
                event = "core.keybindings.cwd_failed",
                error = %e,
                "Could not determine current directory; project keybindings not loaded"
            );
        }
    }

    keybindings
}

/// Try to load a keybindings file. Returns `None` on file-not-found (silent)
/// or on parse error (warns). Returns `Some(Keybindings)` on success.
fn try_load_keybindings_file(path: &std::path::Path) -> Option<Keybindings> {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            warn!(
                event = "core.keybindings.read_failed",
                path = %path.display(),
                error = %e,
            );
            None
        }
        Ok(content) => match toml::from_str(&content) {
            Ok(kb) => Some(kb),
            Err(e) => {
                warn!(
                    event = "core.keybindings.parse_failed",
                    path = %path.display(),
                    error = %e,
                );
                None
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_defaults_round_trip_through_serde() {
        let kb = Keybindings::default();
        let toml_str = toml::to_string(&kb).unwrap();
        let parsed: Keybindings = toml::from_str(&toml_str).unwrap();
        // Defaults are None, so serialized as empty — accessors return hardcoded values
        assert_eq!(parsed.terminal.focus_escape(), "ctrl+escape");
        assert_eq!(parsed.terminal.copy(), "cmd+c");
        assert_eq!(parsed.terminal.paste(), "cmd+v");
        assert_eq!(parsed.navigation.next_kild(), "cmd+j");
        assert_eq!(parsed.navigation.prev_kild(), "cmd+k");
        assert_eq!(parsed.navigation.jump_modifier(), "ctrl");
        assert_eq!(parsed.navigation.toggle_view(), "cmd+d");
        assert_eq!(parsed.navigation.next_workspace(), "cmd+shift+]");
        assert_eq!(parsed.navigation.prev_workspace(), "cmd+shift+[");
    }

    #[test]
    fn test_merge_user_overrides_base() {
        let base = Keybindings::default();
        let mut override_config = Keybindings::default();
        override_config.navigation.next_kild = Some("alt+j".to_string());
        override_config.terminal.copy = Some("ctrl+c".to_string());

        let merged = Keybindings::merge(&base, &override_config);
        assert_eq!(merged.navigation.next_kild(), "alt+j");
        assert_eq!(merged.terminal.copy(), "ctrl+c");
        // Unset fields fall back to defaults
        assert_eq!(merged.navigation.prev_kild(), "cmd+k");
        assert_eq!(merged.terminal.paste(), "cmd+v");
    }

    #[test]
    fn test_merge_base_preserved_when_override_none() {
        let mut base = Keybindings::default();
        base.navigation.next_kild = Some("alt+j".to_string());

        let override_config = Keybindings::default(); // all None
        let merged = Keybindings::merge(&base, &override_config);
        assert_eq!(merged.navigation.next_kild(), "alt+j");
    }

    #[test]
    fn test_merge_both_none_returns_defaults() {
        let base = Keybindings::default();
        let override_config = Keybindings::default();
        let merged = Keybindings::merge(&base, &override_config);
        assert_eq!(merged.navigation.jump_modifier(), "ctrl");
    }

    #[test]
    fn test_missing_file_returns_defaults() {
        let result =
            try_load_keybindings_file(std::path::Path::new("/nonexistent/path/keybindings.toml"));
        assert!(result.is_none());
    }

    #[test]
    fn test_invalid_toml_warns_and_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keybindings.toml");
        std::fs::File::create(&path)
            .unwrap()
            .write_all(b"invalid toml [[[")
            .unwrap();

        let result = try_load_keybindings_file(&path);
        assert!(result.is_none());
    }

    #[test]
    fn test_valid_toml_loads_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keybindings.toml");
        std::fs::File::create(&path)
            .unwrap()
            .write_all(b"[navigation]\nnext_kild = \"alt+j\"\nprev_kild = \"alt+k\"\n")
            .unwrap();

        let result = try_load_keybindings_file(&path);
        assert!(result.is_some());
        let kb = result.unwrap();
        assert_eq!(kb.navigation.next_kild(), "alt+j");
        assert_eq!(kb.navigation.prev_kild(), "alt+k");
    }

    #[test]
    fn test_load_hierarchy_returns_defaults_when_no_files() {
        // In the test environment, no keybindings.toml exists → defaults
        let kb = load_hierarchy();
        // We only verify it doesn't panic and returns valid Keybindings
        assert_eq!(kb.navigation.jump_modifier(), "ctrl");
    }
}
