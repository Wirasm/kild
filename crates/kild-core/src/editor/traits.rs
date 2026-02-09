use std::path::Path;

use crate::config::KildConfig;

use super::errors::EditorError;

/// Trait defining the interface for editor backends.
///
/// Each supported editor (Zed, VS Code, Vim/Neovim, etc.) implements this trait
/// to provide editor-specific spawning behavior.
pub trait EditorBackend: Send + Sync {
    /// The canonical name of this editor (e.g., "zed", "code", "vim").
    fn name(&self) -> &'static str;

    /// The user-facing display name (e.g., "Zed", "VS Code", "Vim").
    fn display_name(&self) -> &'static str;

    /// Whether this editor is available on the system.
    fn is_available(&self) -> bool;

    /// Whether this editor runs inside a terminal (e.g., vim, nvim, helix).
    ///
    /// INVARIANT: If this returns `true`, `open()` MUST delegate to
    /// `terminal_ops::spawn_terminal()`. If `false`, `open()` MUST spawn
    /// the editor process directly via `Command::new()`.
    fn is_terminal_editor(&self) -> bool;

    /// Open a path in this editor.
    ///
    /// For GUI editors, spawns a new process directly.
    /// For terminal editors, delegates to the terminal backend via `config`.
    fn open(&self, path: &Path, flags: &[String], config: &KildConfig) -> Result<(), EditorError>;

    /// Open with an override command name.
    ///
    /// Used for editors where multiple command names map to the same backend
    /// (e.g., vim/nvim/helix all use VimBackend). The default implementation
    /// ignores the override and calls `open()`.
    fn open_with_command(
        &self,
        _command_override: &str,
        path: &Path,
        flags: &[String],
        config: &KildConfig,
    ) -> Result<(), EditorError> {
        self.open(path, flags, config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockBackend;

    impl EditorBackend for MockBackend {
        fn name(&self) -> &'static str {
            "mock"
        }

        fn display_name(&self) -> &'static str {
            "Mock Editor"
        }

        fn is_available(&self) -> bool {
            true
        }

        fn is_terminal_editor(&self) -> bool {
            false
        }

        fn open(
            &self,
            _path: &Path,
            _flags: &[String],
            _config: &KildConfig,
        ) -> Result<(), EditorError> {
            Ok(())
        }
    }

    #[test]
    fn test_editor_backend_basic_methods() {
        let backend = MockBackend;
        assert_eq!(backend.name(), "mock");
        assert_eq!(backend.display_name(), "Mock Editor");
        assert!(backend.is_available());
        assert!(!backend.is_terminal_editor());
    }

    #[test]
    fn test_editor_backend_open() {
        let backend = MockBackend;
        let config = KildConfig::default();
        let result = backend.open(Path::new("/tmp"), &[], &config);
        assert!(result.is_ok());
    }
}
