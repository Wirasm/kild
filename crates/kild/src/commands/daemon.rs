use clap::ArgMatches;
use tracing::{error, info};

pub(crate) fn handle_daemon_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    match matches.subcommand() {
        Some(("start", sub)) => handle_daemon_start(sub),
        Some(("stop", _)) => handle_daemon_stop(),
        Some(("status", sub)) => handle_daemon_status(sub),
        _ => Err("Unknown daemon subcommand".into()),
    }
}

fn handle_daemon_start(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let foreground = matches.get_flag("foreground");

    info!(event = "cli.daemon.start_started", foreground = foreground);

    // Check if already running
    if kild_core::daemon::client::ping_daemon().unwrap_or(false) {
        let pid = read_daemon_pid()?;
        println!("Daemon already running (PID: {})", pid);
        return Ok(());
    }

    if foreground {
        // Run the server inline (blocks until shutdown)
        println!(
            "Starting daemon in foreground (PID: {})...",
            std::process::id()
        );
        let config = kild_daemon::load_daemon_config()?;
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            if let Err(e) = kild_daemon::run_server(config).await {
                error!(event = "cli.daemon.start_failed", error = %e);
                eprintln!("Daemon error: {}", e);
            }
        });
        info!(event = "cli.daemon.start_completed");
    } else {
        // Spawn daemon as a detached background process
        let daemon_binary = std::env::current_exe()?;
        std::process::Command::new(&daemon_binary)
            .args(["daemon", "start", "--foreground"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .stdin(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start daemon: {}", e))?;

        // Wait for socket to become available
        let socket_path = kild_core::daemon::socket_path();
        let timeout = std::time::Duration::from_secs(5);
        let start = std::time::Instant::now();

        loop {
            if socket_path.exists() && kild_core::daemon::client::ping_daemon().unwrap_or(false) {
                break;
            }
            if start.elapsed() > timeout {
                eprintln!("Daemon started but socket not available after 5s.");
                eprintln!("Try: kild daemon start --foreground  (to see startup errors)");
                eprintln!("Try: ps aux | grep 'kild daemon'     (to check process status)");
                return Err("Daemon socket not available after 5s".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        let pid = read_daemon_pid().unwrap_or(0);
        println!("Daemon started (PID: {})", pid);
        info!(event = "cli.daemon.start_completed", pid = pid);
    }

    Ok(())
}

fn handle_daemon_stop() -> Result<(), Box<dyn std::error::Error>> {
    info!(event = "cli.daemon.stop_started");

    match kild_core::daemon::client::request_shutdown() {
        Ok(()) => {
            // Wait for daemon to exit (poll PID file removal)
            let pid_file = kild_core::daemon::pid_file_path();
            let timeout = std::time::Duration::from_secs(5);
            let start = std::time::Instant::now();

            loop {
                if !pid_file.exists() {
                    println!("Daemon stopped");
                    info!(event = "cli.daemon.stop_completed");
                    return Ok(());
                }
                if start.elapsed() > timeout {
                    eprintln!("Daemon did not stop gracefully after 5s");
                    return Err("Daemon stop timed out".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        Err(kild_core::daemon::client::DaemonClientError::NotRunning { .. }) => {
            println!("Daemon is not running");
            Ok(())
        }
        Err(e) => {
            error!(event = "cli.daemon.stop_failed", error = %e);
            Err(e.into())
        }
    }
}

fn handle_daemon_status(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let json = matches.get_flag("json");
    info!(event = "cli.daemon.status_started");

    let running = kild_core::daemon::client::ping_daemon().unwrap_or(false);

    if json {
        let status = if running {
            let pid = read_daemon_pid().unwrap_or(0);
            serde_json::json!({
                "running": true,
                "pid": pid,
                "socket": kild_core::daemon::socket_path().display().to_string(),
            })
        } else {
            serde_json::json!({
                "running": false,
            })
        };
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else if running {
        let pid = read_daemon_pid().unwrap_or(0);
        println!("Daemon: running (PID: {})", pid);
        println!("Socket: {}", kild_core::daemon::socket_path().display());
    } else {
        println!("Daemon: stopped");
    }

    Ok(())
}

fn read_daemon_pid() -> Result<u32, Box<dyn std::error::Error>> {
    let pid_file = kild_core::daemon::pid_file_path();
    let content = std::fs::read_to_string(&pid_file)
        .map_err(|e| format!("Cannot read daemon PID file: {}", e))?;
    content
        .trim()
        .parse::<u32>()
        .map_err(|e| format!("Invalid PID in daemon PID file: {}", e).into())
}
