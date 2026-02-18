use std::path::Path;
use std::process::Command;

use tracing::{error, info};

use crate::editor::errors::EditorError;
use crate::editor::traits::EditorBackend;
use kild_config::KildConfig;

pub struct VSCodeBackend;

impl EditorBackend for VSCodeBackend {
    fn name(&self) -> &'static str {
        "code"
    }

    fn display_name(&self) -> &'static str {
        "VS Code"
    }

    fn is_available(&self) -> bool {
        which::which("code").is_ok()
    }

    fn is_terminal_editor(&self) -> bool {
        false
    }

    fn open(&self, path: &Path, flags: &[String], _config: &KildConfig) -> Result<(), EditorError> {
        let mut cmd = Command::new("code");
        for flag in flags {
            cmd.arg(flag);
        }
        cmd.arg(path);

        match cmd.spawn() {
            Ok(_) => {
                info!(event = "core.editor.open_completed", editor = "code");
                Ok(())
            }
            Err(e) => {
                error!(
                    event = "core.editor.open_failed",
                    editor = "code",
                    error = %e
                );
                Err(EditorError::SpawnFailed {
                    message: format!("code: {}", e),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vscode_backend_identity() {
        let backend = VSCodeBackend;
        assert_eq!(backend.name(), "code");
        assert_eq!(backend.display_name(), "VS Code");
        assert!(!backend.is_terminal_editor());
    }
}
