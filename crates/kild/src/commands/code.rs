use clap::ArgMatches;
use tracing::{error, info};

use kild_core::events;
use kild_core::session_ops as session_handler;

use super::helpers::load_config_with_warning;

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
    let session = match session_handler::get_session(branch) {
        Ok(session) => session,
        Err(e) => {
            eprintln!("❌ Failed to find kild '{}': {}", branch, e);
            error!(event = "cli.code_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    // 3. Determine editor
    let editor = config.editor.resolve_editor(editor_override.as_deref());

    info!(
        event = "cli.code_editor_selected",
        branch = branch,
        editor = editor,
        terminal = config.editor.terminal(),
        flags = ?config.editor.flags()
    );

    // 4. Spawn editor
    if config.editor.terminal() {
        let editor_command = config
            .editor
            .build_terminal_command(&editor, &session.worktree_path);

        match kild_core::terminal_ops::spawn_terminal(
            &session.worktree_path,
            &editor_command,
            &config,
            None,
            None,
        ) {
            Ok(_) => {
                println!("✅ Opening '{}' in {} (terminal)", branch, editor);
                println!("   Path: {}", session.worktree_path.display());
                info!(
                    event = "cli.code_completed",
                    branch = branch,
                    editor = editor,
                    terminal = true,
                    worktree_path = %session.worktree_path.display()
                );
                Ok(())
            }
            Err(e) => {
                eprintln!("❌ Failed to open editor '{}' in terminal: {}", editor, e);
                error!(
                    event = "cli.code_failed",
                    branch = branch,
                    editor = editor,
                    terminal = true,
                    error = %e
                );
                Err(e.into())
            }
        }
    } else {
        let mut cmd = config
            .editor
            .build_gui_command(&editor, &session.worktree_path);

        match cmd.spawn() {
            Ok(_) => {
                println!("✅ Opening '{}' in {}", branch, editor);
                println!("   Path: {}", session.worktree_path.display());
                info!(
                    event = "cli.code_completed",
                    branch = branch,
                    editor = editor,
                    worktree_path = %session.worktree_path.display()
                );
                Ok(())
            }
            Err(e) => {
                eprintln!("❌ Failed to open editor '{}': {}", editor, e);
                eprintln!(
                    "   Hint: Make sure '{}' is installed and in your PATH",
                    editor
                );
                error!(
                    event = "cli.code_failed",
                    branch = branch,
                    editor = editor,
                    error = %e
                );
                Err(e.into())
            }
        }
    }
}
