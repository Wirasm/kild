use tokio_util::sync::CancellationToken;
#[allow(unused_imports)]
use tracing::{error, info};

/// Wait for a shutdown signal (SIGTERM or SIGINT/Ctrl-C).
///
/// When the signal is received, cancels the provided token to notify
/// all tasks to drain gracefully.
pub async fn wait_for_shutdown_signal(token: CancellationToken) -> Result<(), std::io::Error> {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

        tokio::select! {
            _ = ctrl_c => {
                info!(event = "daemon.server.signal_received", signal = "SIGINT");
            }
            _ = sigterm.recv() => {
                info!(event = "daemon.server.signal_received", signal = "SIGTERM");
            }
        }
    }

    #[cfg(not(unix))]
    {
        match ctrl_c.await {
            Ok(()) => {
                info!(event = "daemon.server.signal_received", signal = "SIGINT");
            }
            Err(e) => {
                error!(
                    event = "daemon.server.signal_handler_failed",
                    error = %e,
                    "Ctrl-C signal handler failed, initiating shutdown anyway",
                );
            }
        }
    }

    token.cancel();
    Ok(())
}
