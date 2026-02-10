use clap::ArgMatches;
use tracing::{error, info};

use kild_core::CreateSessionRequest;
use kild_core::events;
use kild_core::session_ops;

use super::helpers::{ensure_daemon_running, load_config_with_warning, resolve_runtime_mode};

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

    // Ensure daemon is running if daemon mode is requested
    if runtime_mode == kild_core::RuntimeMode::Daemon {
        ensure_daemon_running(&config)?;
    }

    let request = CreateSessionRequest::new(branch.clone(), agent_mode, note)
        .with_base_branch(base_branch)
        .with_no_fetch(no_fetch)
        .with_runtime_mode(runtime_mode);

    match session_ops::create_session(request, &config) {
        Ok(session) => {
            println!("✅ KILD created successfully!");
            println!("   Branch: {}", session.branch);
            if session.agent == "shell" {
                println!("   Agent: (none - bare shell)");
            } else {
                println!("   Agent: {}", session.agent);
            }
            println!("   Worktree: {}", session.worktree_path.display());
            println!(
                "   Port Range: {}-{}",
                session.port_range_start, session.port_range_end
            );
            println!("   Status: {:?}", session.status);

            info!(
                event = "cli.create_completed",
                session_id = session.id,
                branch = session.branch
            );

            Ok(())
        }
        Err(e) => {
            // Surface actionable hint for fetch failures
            let err_str = e.to_string();
            if err_str.contains("Failed to fetch") {
                eprintln!("❌ Failed to create kild: {}", e);
                eprintln!(
                    "   Hint: Use --no-fetch to skip fetching, or check your network/remote config."
                );
            } else {
                eprintln!("❌ Failed to create kild: {}", e);
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
