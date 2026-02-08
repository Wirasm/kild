use std::path::Path;

use tracing::{error, info};

use crate::config::KildConfig;
use crate::editor::errors::EditorError;
use crate::editor::traits::EditorBackend;
use crate::terminal::common::escape::shell_escape;
use crate::terminal::handler as terminal_ops;

pub struct VimBackend;

impl EditorBackend for VimBackend {
    fn name(&self) -> &'static str {
        "vim"
    }

    fn display_name(&self) -> &'static str {
        "Vim"
    }

    fn is_available(&self) -> bool {
        which::which("vim").is_ok() || which::which("nvim").is_ok()
    }

    fn is_terminal_editor(&self) -> bool {
        true
    }

    fn open(&self, path: &Path, flags: &[String], config: &KildConfig) -> Result<(), EditorError> {
        self.open_with_command("vim", path, flags, config)
    }
}

impl VimBackend {
    /// Open with a specific editor command (vim, nvim, helix, etc.).
    ///
    /// This is called by the registry with the resolved editor command name,
    /// allowing the same backend to handle vim, nvim, and helix.
    pub fn open_with_command(
        &self,
        editor_cmd: &str,
        path: &Path,
        flags: &[String],
        config: &KildConfig,
    ) -> Result<(), EditorError> {
        info!(
            event = "core.editor.open_started",
            editor = editor_cmd,
            path = %path.display(),
            terminal = true
        );

        let escaped_path = shell_escape(&path.display().to_string());
        let command = if flags.is_empty() {
            format!("{} {}", editor_cmd, escaped_path)
        } else {
            format!("{} {} {}", editor_cmd, flags.join(" "), escaped_path)
        };

        match terminal_ops::spawn_terminal(path, &command, config, None, None) {
            Ok(_) => {
                info!(
                    event = "core.editor.open_completed",
                    editor = editor_cmd,
                    terminal = true
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    event = "core.editor.open_failed",
                    editor = editor_cmd,
                    error = %e,
                    terminal = true
                );
                Err(EditorError::TerminalSpawnFailed { source: e })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vim_backend_identity() {
        let backend = VimBackend;
        assert_eq!(backend.name(), "vim");
        assert_eq!(backend.display_name(), "Vim");
        assert!(backend.is_terminal_editor());
    }
}
