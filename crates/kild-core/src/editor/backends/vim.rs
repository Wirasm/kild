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

    fn open_with_command(
        &self,
        editor_cmd: &str,
        path: &Path,
        flags: &[String],
        config: &KildConfig,
    ) -> Result<(), EditorError> {
        let escaped_path = shell_escape(&path.display().to_string());
        let escaped_flags: Vec<String> = flags.iter().map(|f| shell_escape(f)).collect();

        let mut parts = vec![editor_cmd.to_string()];
        parts.extend(escaped_flags);
        parts.push(escaped_path);
        let command = parts.join(" ");

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

    #[test]
    fn test_vim_backend_shell_escapes_path() {
        // Verify shell_escape is applied to paths with metacharacters
        use crate::terminal::common::escape::shell_escape;
        let escaped = shell_escape("/tmp/`whoami`");
        assert!(escaped.contains("'"), "backticks should be quoted");

        let escaped = shell_escape("/tmp/$(rm -rf /)");
        assert!(escaped.contains("'"), "$() should be quoted");

        let escaped = shell_escape("/tmp/; rm -rf /");
        assert!(escaped.contains("'"), "semicolons should be quoted");
    }

    #[test]
    fn test_vim_backend_shell_escapes_flags() {
        use crate::terminal::common::escape::shell_escape;
        let flag = "--flag; rm -rf /";
        let escaped = shell_escape(flag);
        assert!(
            escaped.contains("'"),
            "flags with semicolons should be quoted"
        );
    }
}
