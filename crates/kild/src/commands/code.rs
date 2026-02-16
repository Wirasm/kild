use clap::ArgMatches;
use tracing::{error, info};

use kild_core::editor::EditorError;

use super::helpers::{self, load_config_with_warning, shorten_home_path};
use crate::color;

pub(crate) fn handle_code_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let editor_override = matches.get_one::<String>("editor").cloned();

    info!(
        event = "cli.code_started",
        branch = branch,
        editor_override = ?editor_override
    );

    // 1. Load config
    let config = load_config_with_warning();

    // 2. Look up the session to get worktree path
    let session = helpers::require_session(branch, "cli.code_failed")?;

    // 3. Open editor via kild-core editor backend
    match kild_core::editor::open_editor(
        &session.worktree_path,
        editor_override.as_deref(),
        &config,
    ) {
        Ok(()) => {
            println!(
                "{} '{}' in editor.",
                color::aurora("Opening"),
                color::ice(branch)
            );
            println!(
                "  {} {}",
                color::muted("Path:"),
                shorten_home_path(&session.worktree_path)
            );
            info!(
                event = "cli.code_completed",
                branch = branch,
                worktree_path = %session.worktree_path.display()
            );
            Ok(())
        }
        Err(e) => {
            if let EditorError::EditorNotFound { editor } = &e {
                eprintln!("{} '{}' not found.", color::error("Editor"), editor);
                eprintln!(
                    "  {} Install '{}' or configure a different editor:",
                    color::hint("Hint:"),
                    editor
                );
                eprintln!(
                    "    {}            (CLI override)",
                    color::hint("--editor <name>")
                );
                eprintln!(
                    "    {}   (config file)",
                    color::hint("[editor] default = \"...\"")
                );
                eprintln!(
                    "    {}          (environment)",
                    color::hint("export EDITOR=...")
                );
            } else if matches!(e, EditorError::NoEditorFound) {
                eprintln!("{}", color::error("No supported editor found."));
                eprintln!(
                    "  {} Install one of: zed, code (VS Code), vim/nvim",
                    color::hint("Hint:")
                );
                eprintln!(
                    "  {} configure a custom editor in ~/.kild/config.toml",
                    color::hint("Or")
                );
            } else {
                eprintln!("{}", color::error(&e.to_string()));
            }

            error!(event = "cli.code_failed", branch = branch, error = %e);
            Err(e.into())
        }
    }
}
