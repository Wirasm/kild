use std::path::Path;
use std::process::Command;

use tracing::{error, info};

use crate::editor::errors::EditorError;
use crate::editor::traits::EditorBackend;
use kild_config::KildConfig;

pub struct ZedBackend;

impl EditorBackend for ZedBackend {
    fn name(&self) -> &'static str {
        "zed"
    }

    fn display_name(&self) -> &'static str {
        "Zed"
    }

    fn is_available(&self) -> bool {
        which::which("zed").is_ok()
    }

    fn is_terminal_editor(&self) -> bool {
        false
    }

    fn open(&self, path: &Path, flags: &[String], _config: &KildConfig) -> Result<(), EditorError> {
        let mut cmd = Command::new("zed");
        for flag in flags {
            cmd.arg(flag);
        }
        cmd.arg(path);

        match cmd.spawn() {
            Ok(_) => {
                info!(event = "core.editor.open_completed", editor = "zed");
                Ok(())
            }
            Err(e) => {
                error!(
                    event = "core.editor.open_failed",
                    editor = "zed",
                    error = %e
                );
                Err(EditorError::SpawnFailed {
                    message: format!("zed: {}", e),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zed_backend_identity() {
        let backend = ZedBackend;
        assert_eq!(backend.name(), "zed");
        assert_eq!(backend.display_name(), "Zed");
        assert!(!backend.is_terminal_editor());
    }
}
