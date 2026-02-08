use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use tracing::{debug, info};

use crate::config::KildConfig;

use super::backends::{GenericBackend, VSCodeBackend, VimBackend, ZedBackend};
use super::errors::EditorError;
use super::traits::EditorBackend;
use super::types::EditorType;

/// Global registry of all known editor backends.
static REGISTRY: LazyLock<EditorRegistry> = LazyLock::new(EditorRegistry::new);

struct EditorRegistry {
    backends: HashMap<EditorType, Box<dyn EditorBackend>>,
}

impl EditorRegistry {
    fn new() -> Self {
        let mut backends: HashMap<EditorType, Box<dyn EditorBackend>> = HashMap::new();
        backends.insert(EditorType::Zed, Box::new(ZedBackend));
        backends.insert(EditorType::VSCode, Box::new(VSCodeBackend));
        backends.insert(EditorType::Vim, Box::new(VimBackend));
        Self { backends }
    }

    fn get(&self, editor_type: &EditorType) -> Option<&dyn EditorBackend> {
        self.backends.get(editor_type).map(|b| b.as_ref())
    }
}

/// Get a reference to an editor backend by type.
pub fn get_backend(editor_type: &EditorType) -> Option<&'static dyn EditorBackend> {
    REGISTRY.get(editor_type)
}

/// Detect available editor in preference order: Zed > VS Code > Vim.
pub fn detect_editor() -> Result<EditorType, EditorError> {
    debug!(event = "core.editor.detection_started");

    let editors = [EditorType::Zed, EditorType::VSCode, EditorType::Vim];

    for editor_type in editors {
        if let Some(backend) = get_backend(&editor_type)
            && backend.is_available()
        {
            debug!(event = "core.editor.detected", editor = backend.name());
            return Ok(editor_type);
        }
    }

    Err(EditorError::NoEditorFound)
}

/// Resolve which editor to use and return `(command_name, matched_type)`.
///
/// Priority: CLI override > config default > $EDITOR > detect_editor().
/// If the resolved name matches a known EditorType (via FromStr), returns it.
/// Otherwise returns None (the caller should use GenericBackend).
fn resolve_editor(
    cli_override: Option<&str>,
    config: &KildConfig,
) -> Result<(String, Option<EditorType>), EditorError> {
    debug!(
        event = "core.editor.resolve_started",
        cli_override = ?cli_override
    );

    let editor_name = config.editor.resolve_editor(cli_override);
    let editor_type = editor_name.parse::<EditorType>().ok();

    debug!(
        event = "core.editor.resolve_completed",
        editor = %editor_name,
        editor_type = ?editor_type
    );

    Ok((editor_name, editor_type))
}

/// Open a path in the resolved editor.
///
/// This is the primary API for both CLI and UI. It resolves which editor
/// to use, finds or creates the appropriate backend, and opens the path.
pub fn open_editor(
    path: &Path,
    cli_override: Option<&str>,
    config: &KildConfig,
) -> Result<(), EditorError> {
    let (editor_name, editor_type) = resolve_editor(cli_override, config)?;

    // Parse flags from config
    let flags: Vec<String> = config
        .editor
        .flags()
        .map(|f| f.split_whitespace().map(String::from).collect())
        .unwrap_or_default();

    info!(
        event = "core.editor.open_started",
        editor = %editor_name,
        editor_type = ?editor_type,
        path = %path.display()
    );

    match editor_type {
        Some(EditorType::Vim) => {
            // Terminal editors resolve to Vim type but the actual command
            // may be "nvim", "helix", etc. Use VimBackend::open_with_command
            // to pass the resolved command name.
            let backend = VimBackend;
            backend.open_with_command(&editor_name, path, &flags, config)
        }
        Some(et) => {
            let backend = get_backend(&et).ok_or_else(|| EditorError::EditorNotFound {
                editor: editor_name.clone(),
            })?;
            backend.open(path, &flags, config)
        }
        None => {
            // Unknown editor - use GenericBackend
            let terminal = config.editor.terminal();
            let backend = GenericBackend::new(editor_name.clone(), terminal);

            if !backend.is_available() {
                return Err(EditorError::EditorNotFound {
                    editor: editor_name,
                });
            }

            backend.open(path, &flags, config)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_backend_zed() {
        let backend = get_backend(&EditorType::Zed);
        assert!(backend.is_some());
        assert_eq!(backend.unwrap().name(), "zed");
    }

    #[test]
    fn test_get_backend_vscode() {
        let backend = get_backend(&EditorType::VSCode);
        assert!(backend.is_some());
        assert_eq!(backend.unwrap().name(), "code");
    }

    #[test]
    fn test_get_backend_vim() {
        let backend = get_backend(&EditorType::Vim);
        assert!(backend.is_some());
        assert_eq!(backend.unwrap().name(), "vim");
    }

    #[test]
    fn test_detect_editor_does_not_panic() {
        let _result = detect_editor();
    }

    #[test]
    fn test_registry_contains_expected_editors() {
        let expected = [EditorType::Zed, EditorType::VSCode, EditorType::Vim];
        for editor_type in expected {
            let backend = get_backend(&editor_type);
            assert!(
                backend.is_some(),
                "Registry should contain {:?}",
                editor_type
            );
        }
    }

    #[test]
    fn test_all_registered_backends_have_correct_names() {
        let checks = [
            (EditorType::Zed, "zed"),
            (EditorType::VSCode, "code"),
            (EditorType::Vim, "vim"),
        ];
        for (editor_type, expected_name) in checks {
            let backend = get_backend(&editor_type).unwrap();
            assert_eq!(
                backend.name(),
                expected_name,
                "Backend for {:?} should have name '{}'",
                editor_type,
                expected_name
            );
        }
    }

    #[test]
    fn test_resolve_editor_with_cli_override() {
        let config = KildConfig::default();
        let (name, _) = resolve_editor(Some("zed"), &config).unwrap();
        assert_eq!(name, "zed");
    }

    #[test]
    fn test_resolve_editor_unknown_returns_none_type() {
        let config = KildConfig::default();
        let (name, editor_type) = resolve_editor(Some("my-custom-editor"), &config).unwrap();
        assert_eq!(name, "my-custom-editor");
        assert!(editor_type.is_none());
    }
}
