use std::path::Path;
use std::process::Command;

use tracing::{error, info};

use crate::config::KildConfig;
use crate::editor::errors::EditorError;
use crate::editor::traits::EditorBackend;
use crate::terminal::common::escape::shell_escape;
use crate::terminal::handler as terminal_ops;

/// Fallback backend for editors not covered by specific backends.
///
/// Unlike the known backends (Zed, VSCode, Vim), GenericBackend holds state
/// (the command name and terminal flag) and is constructed dynamically by the
/// registry when no known backend matches.
///
/// The `terminal` flag takes precedence over `config.editor.terminal()` since
/// GenericBackend is constructed with the resolved terminal mode by the registry.
pub struct GenericBackend {
    command: String,
    terminal: bool,
}

impl GenericBackend {
    pub fn new(command: String, terminal: bool) -> Self {
        Self { command, terminal }
    }
}

impl EditorBackend for GenericBackend {
    fn name(&self) -> &'static str {
        // GenericBackend is dynamically created, so we can't return the command
        // as &'static str. Return a generic label instead.
        "generic"
    }

    fn display_name(&self) -> &'static str {
        "Generic"
    }

    fn is_available(&self) -> bool {
        which::which(&self.command).is_ok()
    }

    fn is_terminal_editor(&self) -> bool {
        self.terminal
    }

    fn open(&self, path: &Path, flags: &[String], config: &KildConfig) -> Result<(), EditorError> {
        if self.terminal {
            let escaped_path = shell_escape(&path.display().to_string());
            let escaped_flags: Vec<String> = flags.iter().map(|f| shell_escape(f)).collect();

            let mut parts = vec![self.command.clone()];
            parts.extend(escaped_flags);
            parts.push(escaped_path);
            let command = parts.join(" ");

            match terminal_ops::spawn_terminal(path, &command, config, None, None) {
                Ok(_) => {
                    info!(
                        event = "core.editor.open_completed",
                        editor = %self.command,
                        terminal = true
                    );
                    Ok(())
                }
                Err(e) => {
                    error!(
                        event = "core.editor.open_failed",
                        editor = %self.command,
                        error = %e,
                        terminal = true
                    );
                    Err(EditorError::TerminalSpawnFailed { source: e })
                }
            }
        } else {
            let mut cmd = Command::new(&self.command);
            for flag in flags {
                cmd.arg(flag);
            }
            cmd.arg(path);

            match cmd.spawn() {
                Ok(_) => {
                    info!(
                        event = "core.editor.open_completed",
                        editor = %self.command
                    );
                    Ok(())
                }
                Err(e) => {
                    error!(
                        event = "core.editor.open_failed",
                        editor = %self.command,
                        error = %e
                    );
                    Err(EditorError::SpawnFailed {
                        message: format!("{}: {}", self.command, e),
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generic_backend_gui() {
        let backend = GenericBackend::new("my-editor".to_string(), false);
        assert_eq!(backend.name(), "generic");
        assert_eq!(backend.display_name(), "Generic");
        assert!(!backend.is_terminal_editor());
        // my-editor is not installed, so is_available should be false
        assert!(!backend.is_available());
    }

    #[test]
    fn test_generic_backend_terminal() {
        let backend = GenericBackend::new("my-term-editor".to_string(), true);
        assert!(backend.is_terminal_editor());
        assert!(!backend.is_available());
    }

    #[test]
    fn test_generic_backend_open_gui_unavailable() {
        let backend = GenericBackend::new("fake-editor-xyz".to_string(), false);
        let config = KildConfig::default();
        let path = std::env::temp_dir();
        let result = backend.open(&path, &[], &config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            EditorError::SpawnFailed { .. }
        ));
    }

    #[test]
    fn test_generic_backend_shell_escapes_flags() {
        use crate::terminal::common::escape::shell_escape;
        // Verify that flags with metacharacters get escaped
        let flag = "--flag; rm -rf /";
        let escaped = shell_escape(flag);
        assert!(
            escaped.contains("'"),
            "flags with shell metacharacters should be quoted"
        );
    }
}
