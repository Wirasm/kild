pub mod connection;
pub mod shutdown;

use std::path::Path;
use std::sync::Arc;

use std::time::Duration;

use tokio::net::{TcpListener, UnixListener};
use tokio::sync::RwLock;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::errors::DaemonError;
use crate::pid;
use crate::session::manager::SessionManager;
use crate::tls;
use crate::types::DaemonConfig;

/// Run the daemon server.
///
/// This is the main entrypoint called by `kild daemon start`. It:
/// 1. Checks for an existing daemon (PID file)
/// 2. Writes a PID file
/// 3. Binds a Unix socket
/// 4. Optionally binds a TLS-wrapped TCP listener (when `bind_tcp` is configured)
/// 5. Accepts client connections in a loop
/// 6. Handles graceful shutdown on SIGTERM/SIGINT
pub async fn run_server(config: DaemonConfig) -> Result<(), DaemonError> {
    let pid_path = config.pid_path.clone();
    let socket_path = config.socket_path.clone();

    // Install ring crypto provider once — required by rustls 0.23.
    // Using try_install so tests that call run_server multiple times don't panic.
    let _ = rustls::crypto::ring::default_provider().install_default();

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

    let session_manager = Arc::new(RwLock::new(SessionManager::new(
        config.clone(),
        pty_exit_tx,
    )));
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

    // Optionally bind TCP+TLS listener and spawn its accept loop in a separate task.
    // Using a separate task keeps the main Unix accept loop simple and avoids
    // select! complexity for the optional TCP arm.
    if let Some(bind_addr) = config.bind_tcp {
        let kild_paths = kild_paths::KildPaths::resolve().unwrap_or_else(|e| {
            error!(
                event = "daemon.server.paths_resolve_failed",
                error = %e,
                "Cannot resolve ~/.kild path; TLS certs will be written to /tmp/.kild — \
                 this path is ephemeral and certs will be lost on reboot, breaking \
                 remote client fingerprint pinning."
            );
            kild_paths::KildPaths::from_dir(std::path::PathBuf::from("/tmp/.kild"))
        });
        let cert_path = config
            .tls_cert_path
            .clone()
            .unwrap_or_else(|| kild_paths.tls_cert_path());
        let key_path = config
            .tls_key_path
            .clone()
            .unwrap_or_else(|| kild_paths.tls_key_path());

        let (certs, key) = tls::load_or_generate_cert(&cert_path, &key_path)?;
        let tls_config = tls::build_server_config(certs, key)?;
        let acceptor = TlsAcceptor::from(tls_config);

        let tcp_listener = TcpListener::bind(bind_addr).await?;
        info!(event = "daemon.server.tcp_listening", addr = %bind_addr);

        let mgr_clone = session_manager.clone();
        let shutdown_clone = shutdown.clone();
        tokio::spawn(tcp_accept_loop(
            tcp_listener,
            acceptor,
            mgr_clone,
            shutdown_clone,
        ));
    }

    // Accept loop (Unix socket)
    loop {
        tokio::select! {
            accept = listener.accept() => {
                match accept {
                    Ok((stream, _addr)) => {
                        let mgr = session_manager.clone();
                        let shutdown_token = shutdown.clone();
                        tokio::spawn(connection::handle_connection(
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

/// Accept loop for TCP+TLS connections.
///
/// Runs as a separate task from the Unix accept loop. Each incoming TCP
/// connection is handed to a spawned task for TLS handshake + handling
/// to avoid blocking new connections on slow handshakes.
async fn tcp_accept_loop(
    listener: TcpListener,
    acceptor: TlsAcceptor,
    session_manager: Arc<RwLock<SessionManager>>,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            accept = listener.accept() => {
                match accept {
                    Ok((tcp_stream, addr)) => {
                        let acceptor = acceptor.clone();
                        let mgr = session_manager.clone();
                        let shutdown_clone = shutdown.clone();
                        tokio::spawn(async move {
                            match acceptor.accept(tcp_stream).await {
                                Ok(tls_stream) => {
                                    info!(
                                        event = "daemon.server.tls_connection_accepted",
                                        addr = %addr,
                                    );
                                    connection::handle_connection(tls_stream, mgr, shutdown_clone).await;
                                }
                                Err(e) => {
                                    warn!(
                                        event = "daemon.server.tls_handshake_failed",
                                        addr = %addr,
                                        error = %e,
                                    );
                                }
                            }
                        });
                    }
                    Err(e) => {
                        error!(
                            event = "daemon.server.tcp_accept_failed",
                            error = %e,
                        );
                        // Brief sleep to avoid tight spin on fatal accept errors
                        // (EMFILE, ENOMEM) that cannot be resolved immediately.
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
            _ = shutdown.cancelled() => {
                info!(event = "daemon.server.tcp_listener_shutdown");
                break;
            }
        }
    }
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
