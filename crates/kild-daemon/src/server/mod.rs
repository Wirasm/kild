pub mod connection;
pub mod shutdown;

use std::path::Path;
use std::sync::Arc;

use tokio::net::UnixListener;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::errors::DaemonError;
use crate::pid;
use crate::session::manager::SessionManager;
use crate::types::DaemonConfig;

/// Run the daemon server.
///
/// This is the main entrypoint called by `kild daemon start`. It:
/// 1. Checks for an existing daemon (PID file)
/// 2. Writes a PID file
/// 3. Binds a Unix socket
/// 4. Accepts client connections in a loop
/// 5. Handles graceful shutdown on SIGTERM/SIGINT
pub async fn run_server(config: DaemonConfig) -> Result<(), DaemonError> {
    let pid_path = config.pid_path.clone();
    let socket_path = config.socket_path.clone();

    // Check if another daemon is already running
    if let Some(existing_pid) = pid::check_daemon_running(&pid_path) {
        return Err(DaemonError::AlreadyRunning(existing_pid));
    }

    // Write PID file
    pid::write_pid_file(&pid_path)?;

    // Clean up stale socket file
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    // Ensure socket directory exists
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Bind Unix socket
    let listener = UnixListener::bind(&socket_path)?;

    info!(
        event = "daemon.server.started",
        pid = std::process::id(),
        socket = %socket_path.display(),
    );

    // Channel for PTY exit notifications from reader tasks
    let (pty_exit_tx, mut pty_exit_rx) = tokio::sync::mpsc::unbounded_channel();

    let session_manager = Arc::new(RwLock::new(SessionManager::new(config, pty_exit_tx)));
    let shutdown = CancellationToken::new();

    // Spawn signal handler
    let signal_shutdown = shutdown.clone();
    tokio::spawn(async move {
        if let Err(e) = shutdown::wait_for_shutdown_signal(signal_shutdown).await {
            tracing::error!(
                event = "daemon.server.signal_handler_failed",
                error = %e,
                "Signal handler failed — SIGTERM/SIGINT will not trigger graceful shutdown. \
                 Use 'kild daemon stop' (IPC) to shut down the daemon instead.",
            );
        }
    });

    // Accept loop
    loop {
        tokio::select! {
            accept = listener.accept() => {
                match accept {
                    Ok((stream, _addr)) => {
                        let mgr = session_manager.clone();
                        let shutdown_token = shutdown.clone();
                        tokio::spawn(connection::route_connection(
                            stream,
                            mgr,
                            shutdown_token,
                        ));
                    }
                    Err(e) => {
                        error!(
                            event = "daemon.server.accept_failed",
                            error = %e,
                        );
                    }
                }
            }
            Some(exit_event) = pty_exit_rx.recv() => {
                // PTY process exited — transition session to Stopped
                let mut mgr = session_manager.write().await;
                if let Some(output_tx) = mgr.handle_pty_exit(&exit_event.session_id) {
                    // Drop the broadcast sender — stream_pty_output tasks will see
                    // RecvError::Closed and exit their streaming loops.
                    drop(output_tx);
                }
            }
            _ = shutdown.cancelled() => {
                info!(event = "daemon.server.shutdown_started");
                break;
            }
        }
    }

    // Graceful shutdown: stop all sessions
    {
        let mut mgr = session_manager.write().await;
        mgr.stop_all();
    }

    // Clean up PID and socket files
    cleanup(&pid_path, &socket_path);

    info!(event = "daemon.server.shutdown_completed");

    Ok(())
}

/// Clean up PID file and socket file on shutdown.
fn cleanup(pid_path: &Path, socket_path: &Path) {
    if let Err(e) = pid::remove_pid_file(pid_path) {
        error!(
            event = "daemon.server.pid_cleanup_failed",
            error = %e,
        );
    }
    if socket_path.exists()
        && let Err(e) = std::fs::remove_file(socket_path)
    {
        error!(
            event = "daemon.server.socket_cleanup_failed",
            error = %e,
        );
    }
}
