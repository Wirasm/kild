use clap::ArgMatches;
use tracing::{debug, error, info, warn};

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

    let daemon_binary = kild_core::daemon::find_sibling_binary("kild-daemon")
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

    if foreground {
        // Spawn kild-daemon with inherited stdio (blocks until child exits)
        let status = std::process::Command::new(&daemon_binary)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .stdin(std::process::Stdio::inherit())
            .status()
            .map_err(|e| format!("Failed to start daemon: {}", e))?;

        if !status.success() {
            error!(event = "cli.daemon.start_failed", exit_code = ?status.code());
            return Err(format!("Daemon exited with {}", status).into());
        }
        info!(event = "cli.daemon.start_completed");
    } else {
        // Spawn daemon as a detached background process
        let mut child = std::process::Command::new(&daemon_binary)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .stdin(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to start daemon: {}", e))?;

        debug!(event = "cli.daemon.spawn_completed", pid = child.id());

        // Wait for socket to become available (with crash detection)
        let socket_path = kild_core::daemon::socket_path();
        let timeout = std::time::Duration::from_secs(5);
        let start = std::time::Instant::now();

        loop {
            // Check if daemon crashed before socket was ready
            match child.try_wait() {
                Ok(Some(status)) => {
                    error!(event = "cli.daemon.start_failed", reason = "child_exited", status = %status);
                    return Err(format!(
                        "Daemon exited with {} before becoming ready.\n\
                         Try: kild daemon start --foreground  (to see startup errors)",
                        status
                    )
                    .into());
                }
                Ok(None) => {} // Still running
                Err(e) => {
                    debug!(event = "cli.daemon.child_status_check_failed", error = %e);
                }
            }

            let socket_exists = socket_path.exists();
            let ping_ok =
                socket_exists && kild_core::daemon::client::ping_daemon().unwrap_or(false);

            debug!(
                event = "cli.daemon.socket_check",
                socket_exists = socket_exists,
                ping_ok = ping_ok,
                elapsed_ms = start.elapsed().as_millis() as u64,
            );

            if ping_ok {
                break;
            }
            if start.elapsed() > timeout {
                eprintln!("Daemon started but socket not available after 5s.");
                eprintln!("Try: kild daemon start --foreground  (to see startup errors)");
                eprintln!("Try: ps aux | grep 'kild-daemon'     (to check process status)");
                return Err("Daemon socket not available after 5s".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        match read_daemon_pid() {
            Ok(pid) => {
                println!("Daemon started (PID: {})", pid);
                info!(event = "cli.daemon.start_completed", pid = pid);
            }
            Err(e) => {
                warn!(event = "cli.daemon.pid_read_failed", error = %e);
                println!("Daemon started (PID unknown)");
                info!(event = "cli.daemon.start_completed");
            }
        }
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
            let pid = read_daemon_pid()
                .map_err(|e| {
                    warn!(event = "cli.daemon.pid_read_failed", error = %e);
                    e
                })
                .ok();
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
        match read_daemon_pid() {
            Ok(pid) => println!("Daemon: running (PID: {})", pid),
            Err(e) => {
                warn!(event = "cli.daemon.pid_read_failed", error = %e);
                println!("Daemon: running (PID unknown)");
            }
        }
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
