use clap::ArgMatches;
use tracing::{error, info};

use kild_core::CreateSessionRequest;
use kild_core::events;
use kild_core::session_ops;
use kild_core::sessions::daemon_helpers::spawn_attach_window;

use super::helpers::{load_config_with_warning, resolve_runtime_mode, shorten_home_path};

pub(crate) fn handle_create_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let branch = matches
        .get_one::<String>("branch")
        .ok_or("Branch argument is required")?;
    let note = matches.get_one::<String>("note").cloned();

    let mut config = load_config_with_warning();
    let no_agent = matches.get_flag("no-agent");

    // Determine agent mode from CLI flags
    let agent_mode = if no_agent {
        kild_core::AgentMode::BareShell
    } else if let Some(agent) = matches.get_one::<String>("agent").cloned() {
        config.agent.default = agent.clone();
        kild_core::AgentMode::Agent(agent)
    } else {
        kild_core::AgentMode::DefaultAgent
    };

    if let Some(terminal) = matches.get_one::<String>("terminal") {
        config.terminal.preferred = Some(terminal.clone());
    }
    if !no_agent {
        if let Some(startup_command) = matches.get_one::<String>("startup-command") {
            config.agent.startup_command = Some(startup_command.clone());
        }
        if let Some(flags) = matches.get_one::<String>("flags") {
            config.agent.flags = Some(flags.clone());
        }
    }

    info!(
        event = "cli.create_started",
        branch = branch,
        agent_mode = ?agent_mode,
        note = ?note
    );

    let base_branch = matches.get_one::<String>("base").cloned();
    let no_fetch = matches.get_flag("no-fetch");

    let daemon_flag = matches.get_flag("daemon");
    let no_daemon_flag = matches.get_flag("no-daemon");
    let runtime_mode = resolve_runtime_mode(daemon_flag, no_daemon_flag, &config);

    let request = CreateSessionRequest::new(branch.clone(), agent_mode, note)
        .with_base_branch(base_branch)
        .with_no_fetch(no_fetch)
        .with_runtime_mode(runtime_mode);

    match session_ops::create_session(request, &config) {
        Ok(session) => {
            // Auto-attach: open a terminal window for daemon sessions
            if session.runtime_mode == Some(kild_core::RuntimeMode::Daemon) {
                let spawn_id = session
                    .latest_agent()
                    .map(|a| a.spawn_id().to_string())
                    .unwrap_or_default();
                spawn_attach_window(&session.branch, &spawn_id, &session.worktree_path, &config);
            }

            println!("Kild created.");
            println!("  Branch:   {}", session.branch);
            if session.agent == "shell" {
                println!("  Agent:    (none)");
            } else {
                println!("  Agent:    {}", session.agent);
            }
            println!("  Worktree: {}", shorten_home_path(&session.worktree_path));
            println!(
                "  Ports:    {}-{}",
                session.port_range_start, session.port_range_end
            );
            println!("  Status:   {:?}", session.status);

            info!(
                event = "cli.create_completed",
                session_id = %session.id,
                branch = %session.branch
            );

            Ok(())
        }
        Err(e) => {
            // Surface actionable hint for fetch failures
            let err_str = e.to_string();
            if err_str.contains("Failed to fetch") {
                eprintln!("{}", e);
                eprintln!(
                    "  Hint: Use --no-fetch to skip fetching, or check your network/remote config."
                );
            } else {
                eprintln!("{}", e);
            }

            error!(
                event = "cli.create_failed",
                branch = branch,
                error = %e
            );

            events::log_app_error(&e);
            Err(e.into())
        }
    }
}
