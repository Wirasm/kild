use clap::ArgMatches;
use tracing::{error, info};

use kild_core::events;
use kild_core::session_ops;

use super::helpers::{
    FailedOperation, format_partial_failure_error, is_valid_branch_name, load_config_with_warning,
};

pub(crate) fn handle_rebase_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    if matches.get_flag("all") {
        let base_override = matches.get_one::<String>("base").cloned();
        return handle_rebase_all(base_override);
    }

    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;

    if !is_valid_branch_name(branch) {
        eprintln!("Invalid branch name: {}", branch);
        error!(event = "cli.rebase_invalid_branch", branch = branch);
        return Err("Invalid branch name".into());
    }

    let config = load_config_with_warning();
    let base_branch = match matches.get_one::<String>("base") {
        Some(s) => s.as_str(),
        None => config.git.base_branch(),
    };

    info!(
        event = "cli.rebase_started",
        branch = branch,
        base = base_branch
    );

    let session = match session_ops::get_session(branch) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Failed to find kild '{}': {}", branch, e);
            error!(event = "cli.rebase_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            return Err(e.into());
        }
    };

    match kild_core::git::remote::rebase_worktree(&session.worktree_path, base_branch) {
        Ok(()) => {
            println!("✅ {}: rebased onto {}", branch, base_branch);
            info!(
                event = "cli.rebase_completed",
                branch = branch,
                base = base_branch
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("⚠️  {}: {}", branch, e);
            error!(
                event = "cli.rebase_failed",
                branch = branch,
                base = base_branch,
                path = %session.worktree_path.display(),
                error = %e
            );
            Err(format!("Rebase failed for '{}'", branch).into())
        }
    }
}

fn handle_rebase_all(base_override: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.rebase_all_started", base_override = ?base_override);

    let config = load_config_with_warning();
    let base_branch = match base_override.as_deref() {
        Some(base) => base,
        None => config.git.base_branch(),
    };

    let sessions = session_ops::list_sessions()?;

    if sessions.is_empty() {
        println!("No kilds to rebase.");
        info!(event = "cli.rebase_all_completed", rebased = 0, failed = 0);
        return Ok(());
    }

    let mut rebased: Vec<String> = Vec::new();
    let mut errors: Vec<FailedOperation> = Vec::new();

    for session in &sessions {
        match kild_core::git::remote::rebase_worktree(&session.worktree_path, base_branch) {
            Ok(()) => {
                println!("✅ {}: rebased onto {}", session.branch, base_branch);
                info!(
                    event = "cli.rebase_completed",
                    branch = session.branch,
                    base = base_branch
                );
                rebased.push(session.branch.clone());
            }
            Err(e) => {
                eprintln!("⚠️  {}: {}", session.branch, e);
                error!(
                    event = "cli.rebase_failed",
                    branch = session.branch,
                    base = base_branch,
                    path = %session.worktree_path.display(),
                    error = %e
                );
                errors.push((session.branch.clone(), e.to_string()));
            }
        }
    }

    info!(
        event = "cli.rebase_all_completed",
        rebased = rebased.len(),
        failed = errors.len()
    );

    if !errors.is_empty() {
        let total = rebased.len() + errors.len();
        return Err(format_partial_failure_error("rebase", errors.len(), total).into());
    }

    Ok(())
}
