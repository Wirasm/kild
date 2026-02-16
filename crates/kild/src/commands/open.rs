use clap::ArgMatches;
use tracing::{error, info};

use kild_core::SessionStatus;
use kild_core::events;
use kild_core::session_ops;
use kild_core::sessions::daemon_helpers::spawn_attach_window;

use super::helpers::{
    FailedOperation, OpenedKild, format_count, format_partial_failure_error,
    load_config_with_warning, resolve_explicit_runtime_mode, resolve_open_mode,
};

pub(crate) fn handle_open_command(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let mode = resolve_open_mode(matches);
    let daemon_flag = matches.get_flag("daemon");
    let no_daemon_flag = matches.get_flag("no-daemon");
    let runtime_mode = resolve_explicit_runtime_mode(daemon_flag, no_daemon_flag);
    let resume = matches.get_flag("resume");

    // Check for --all flag first
    if matches.get_flag("all") {
        return handle_open_all(mode, runtime_mode, resume);
    }

    // Single branch operation
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required (or use --all)")?;

    info!(event = "cli.open_started", branch = branch, mode = ?mode);

    match session_ops::open_session(branch, mode.clone(), runtime_mode, resume) {
        Ok(session) => {
            // Auto-attach: open a terminal window for daemon sessions
            if session.runtime_mode == Some(kild_core::RuntimeMode::Daemon) {
                let config = load_config_with_warning();
                let spawn_id = session
                    .latest_agent()
                    .map(|a| a.spawn_id().to_string())
                    .unwrap_or_default();
                spawn_attach_window(&session.branch, &spawn_id, &session.worktree_path, &config);
            }

            match mode {
                kild_core::OpenMode::BareShell => {
                    println!("Opened bare terminal for '{}'.", branch);
                    println!("  Agent: (none)");
                }
                _ => {
                    if resume {
                        println!("Resumed agent for '{}'.", branch);
                    } else {
                        println!("Opened agent for '{}'.", branch);
                    }
                    // Show the agent that was actually spawned, not the session's
                    // stored creation agent (which may be "shell" for --no-agent sessions).
                    let display_agent = session
                        .latest_agent()
                        .map(|a| a.agent().to_string())
                        .unwrap_or_else(|| session.agent.clone());
                    println!("  Agent: {}", display_agent);
                }
            }
            if let Some(pid) = session.latest_agent().and_then(|a| a.process_id()) {
                println!("  PID:   {}", pid);
            }
            info!(
                event = "cli.open_completed",
                branch = branch,
                session_id = %session.id
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Could not open '{}': {}", branch, e);
            error!(event = "cli.open_failed", branch = branch, error = %e);
            events::log_app_error(&e);
            Err(e.into())
        }
    }
}

/// Handle `kild open --all` - open agents in all stopped kilds
fn handle_open_all(
    mode: kild_core::OpenMode,
    runtime_mode: Option<kild_core::RuntimeMode>,
    resume: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.open_all_started", mode = ?mode);

    let sessions = session_ops::list_sessions()?;
    let stopped: Vec<_> = sessions
        .into_iter()
        .filter(|s| s.status == SessionStatus::Stopped)
        .collect();

    if stopped.is_empty() {
        println!("No stopped kilds to open.");
        info!(event = "cli.open_all_completed", opened = 0, failed = 0);
        return Ok(());
    }

    // Load config once for potential auto-attach calls
    let config = load_config_with_warning();

    let mut opened: Vec<OpenedKild> = Vec::new();
    let mut errors: Vec<FailedOperation> = Vec::new();

    for session in stopped {
        match session_ops::open_session(&session.branch, mode.clone(), runtime_mode.clone(), resume)
        {
            Ok(s) => {
                // Auto-attach: open a terminal window for daemon sessions
                if s.runtime_mode == Some(kild_core::RuntimeMode::Daemon) {
                    let spawn_id = s
                        .latest_agent()
                        .map(|a| a.spawn_id().to_string())
                        .unwrap_or_default();
                    spawn_attach_window(&s.branch, &spawn_id, &s.worktree_path, &config);
                }

                info!(
                    event = "cli.open_completed",
                    branch = %s.branch,
                    session_id = %s.id
                );
                let display_agent = s
                    .latest_agent()
                    .map_or(s.agent.clone(), |a| a.agent().to_string());
                opened.push((s.branch.to_string(), display_agent, s.runtime_mode.clone()));
            }
            Err(e) => {
                error!(
                    event = "cli.open_failed",
                    branch = %session.branch,
                    error = %e
                );
                events::log_app_error(&e);
                errors.push((session.branch.to_string(), e.to_string()));
            }
        }
    }

    // Report successes
    if !opened.is_empty() {
        println!("Opened {}:", format_count(opened.len()));
        for (branch, agent, runtime_mode) in &opened {
            let mode_label = match runtime_mode {
                Some(kild_core::RuntimeMode::Daemon) => " [daemon]",
                Some(kild_core::RuntimeMode::Terminal) => " [terminal]",
                None => "",
            };
            println!("  {} ({}){}", branch, agent, mode_label);
        }
    }

    // Report failures
    if !errors.is_empty() {
        eprintln!("{} failed to open:", format_count(errors.len()));
        for (branch, err) in &errors {
            eprintln!("  {}: {}", branch, err);
        }
    }

    info!(
        event = "cli.open_all_completed",
        opened = opened.len(),
        failed = errors.len()
    );

    // Return error if any failures (for exit code)
    if !errors.is_empty() {
        let total_count = opened.len() + errors.len();
        return Err(format_partial_failure_error("open", errors.len(), total_count).into());
    }

    Ok(())
}
