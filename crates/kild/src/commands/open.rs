use clap::ArgMatches;
use tracing::{error, info};

use kild_core::SessionStatus;
use kild_core::events;
use kild_core::session_ops;

use super::helpers::{
    FailedOperation, OpenedKild, format_partial_failure_error, resolve_explicit_runtime_mode,
    resolve_open_mode,
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
            match mode {
                kild_core::OpenMode::BareShell => {
                    println!("✅ Opened bare terminal in kild '{}'", branch);
                    println!("   Agent: (none - bare shell)");
                }
                _ => {
                    if resume {
                        println!("✅ Resumed agent in kild '{}'", branch);
                    } else {
                        println!("✅ Opened new agent in kild '{}'", branch);
                    }
                    println!("   Agent: {}", session.agent);
                }
            }
            if let Some(pid) = session.latest_agent().and_then(|a| a.process_id()) {
                println!("   PID: {}", pid);
            }
            info!(
                event = "cli.open_completed",
                branch = branch,
                session_id = session.id
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to open kild '{}': {}", branch, e);
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

    let mut opened: Vec<OpenedKild> = Vec::new();
    let mut errors: Vec<FailedOperation> = Vec::new();

    for session in stopped {
        match session_ops::open_session(&session.branch, mode.clone(), runtime_mode.clone(), resume)
        {
            Ok(s) => {
                info!(
                    event = "cli.open_completed",
                    branch = s.branch,
                    session_id = s.id
                );
                opened.push((s.branch, s.agent, s.runtime_mode.clone()));
            }
            Err(e) => {
                error!(
                    event = "cli.open_failed",
                    branch = session.branch,
                    error = %e
                );
                events::log_app_error(&e);
                errors.push((session.branch, e.to_string()));
            }
        }
    }

    // Report successes
    if !opened.is_empty() {
        println!("Opened {} kild(s):", opened.len());
        for (branch, agent, runtime_mode) in &opened {
            let mode_label = match runtime_mode {
                Some(kild_core::RuntimeMode::Daemon) => " [daemon]",
                Some(kild_core::RuntimeMode::Terminal) => " [terminal]",
                None => "",
            };
            println!("   {} ({}){}", branch, agent, mode_label);
        }
    }

    // Report failures
    if !errors.is_empty() {
        eprintln!("Failed to open {} kild(s):", errors.len());
        for (branch, err) in &errors {
            eprintln!("   {}: {}", branch, err);
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
