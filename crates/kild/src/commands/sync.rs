use clap::ArgMatches;
use tracing::{error, info};

use kild_core::session_ops;

use super::helpers::{
    self, FailedOperation, format_partial_failure_error, is_valid_branch_name,
    load_config_with_warning,
};

pub(crate) fn handle_sync_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    if matches.get_flag("all") {
        let base_override = matches.get_one::<String>("base").cloned();
        return handle_sync_all(base_override);
    }

    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;

    if !is_valid_branch_name(branch) {
        eprintln!("Invalid branch name: {}", branch);
        error!(event = "cli.sync_invalid_branch", branch = branch);
        return Err("Invalid branch name".into());
    }

    let config = load_config_with_warning();
    let base_branch = match matches.get_one::<String>("base") {
        Some(s) => s.as_str(),
        None => config.git.base_branch(),
    };
    let remote = config.git.remote();

    info!(
        event = "cli.sync_started",
        branch = branch,
        base = base_branch,
        remote = remote
    );

    let session = helpers::require_session(branch, "cli.sync_failed")?;

    // Fetch from remote — use the project repo path (worktrees share the same .git)
    let project = kild_core::git::detect_project()?;
    if let Err(e) = kild_core::git::remote::fetch_remote(&project.path, remote, base_branch) {
        error!(
            event = "cli.sync_fetch_failed",
            branch = branch,
            remote = remote,
            error = %e
        );
        eprintln!("Fetch failed from remote '{}': {}", remote, e);
        eprintln!("  Cannot sync without fetching. Check your network and remote config.");
        eprintln!(
            "  Hint: Use 'kild rebase {}' to rebase onto local state without fetching.",
            branch
        );
        return Err(e.into());
    }

    match kild_core::git::remote::rebase_worktree(&session.worktree_path, base_branch) {
        Ok(()) => {
            println!(
                "{}: synced (fetched + rebased onto {})",
                branch, base_branch
            );
            info!(
                event = "cli.sync_completed",
                branch = branch,
                base = base_branch
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("{}: {}", branch, e);
            error!(
                event = "cli.sync_failed",
                branch = branch,
                base = base_branch,
                path = %session.worktree_path.display(),
                error = %e
            );
            Err(format!("Sync failed for '{}'", branch).into())
        }
    }
}

fn handle_sync_all(base_override: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.sync_all_started", base_override = ?base_override);

    let config = load_config_with_warning();
    let base_branch = match base_override.as_deref() {
        Some(base) => base,
        None => config.git.base_branch(),
    };
    let remote = config.git.remote();

    // Fetch once at repo level (all worktrees share the same .git)
    let project = kild_core::git::detect_project()?;
    if let Err(e) = kild_core::git::remote::fetch_remote(&project.path, remote, base_branch) {
        error!(
            event = "cli.sync_all_fetch_failed",
            remote = remote,
            error = %e
        );
        eprintln!("Fetch failed from remote '{}': {}", remote, e);
        eprintln!("  Cannot sync kilds without fetching. Check your network and remote config.");
        eprintln!(
            "  Hint: Use 'kild rebase --all' to rebase all kilds onto local state without fetching."
        );
        return Err(e.into());
    }

    info!(
        event = "cli.sync_all_fetch_completed",
        remote = remote,
        base = base_branch
    );

    let sessions = session_ops::list_sessions()?;

    if sessions.is_empty() {
        println!("No kilds to sync.");
        info!(event = "cli.sync_all_completed", synced = 0, failed = 0);
        return Ok(());
    }

    let mut synced: Vec<String> = Vec::new();
    let mut errors: Vec<FailedOperation> = Vec::new();

    for session in &sessions {
        match kild_core::git::remote::rebase_worktree(&session.worktree_path, base_branch) {
            Ok(()) => {
                println!("{}: rebased onto {}", session.branch, base_branch);
                info!(
                    event = "cli.sync_completed",
                    branch = %session.branch,
                    base = base_branch
                );
                synced.push(session.branch.to_string());
            }
            Err(e) => {
                eprintln!("{}: {}", session.branch, e);
                error!(
                    event = "cli.sync_failed",
                    branch = %session.branch,
                    base = base_branch,
                    path = %session.worktree_path.display(),
                    error = %e
                );
                errors.push((session.branch.to_string(), e.to_string()));
            }
        }
    }

    info!(
        event = "cli.sync_all_completed",
        synced = synced.len(),
        failed = errors.len()
    );

    if !errors.is_empty() {
        let total = synced.len() + errors.len();
        return Err(format_partial_failure_error("sync", errors.len(), total).into());
    }

    Ok(())
}
