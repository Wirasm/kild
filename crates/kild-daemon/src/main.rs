use tracing::{error, info};

fn main() {
    kild_core::init_logging(false);
    info!(event = "daemon.start_started");

    let exit_code = match run() {
        Ok(()) => {
            info!(event = "daemon.start_completed");
            0
        }
        Err(e) => {
            error!(event = "daemon.start_failed", error = %e);
            eprintln!("kild-daemon: {}", e);
            1
        }
    };
    std::process::exit(exit_code);
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!(
        "Starting daemon in foreground (PID: {})...",
        std::process::id()
    );

    let config = kild_daemon::load_daemon_config().map_err(|e| {
        error!(event = "daemon.config_load_failed", error = %e);
        e
    })?;

    let rt = tokio::runtime::Runtime::new().map_err(|e| {
        error!(event = "daemon.runtime_init_failed", error = %e);
        e
    })?;

    rt.block_on(async {
        kild_daemon::run_server(config).await.map_err(|e| {
            error!(event = "daemon.server_failed", error = %e);
            e
        })
    })?;

    Ok(())
}
