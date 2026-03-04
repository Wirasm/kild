//! ACP runtime thread and Send+Sync handle.
//!
//! The ACP SDK produces `!Send` futures, so we run it on a dedicated OS thread
//! with a single-threaded tokio runtime + `LocalSet`. External callers
//! communicate through mpsc channels, making the public API `Send + Sync`.

use std::thread;

use agent_client_protocol as acp;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::connection;
use crate::errors::AcpError;
use crate::types::{AcpCommand, AcpEvent, AcpSpawnConfig};

/// `Send + Sync` handle to a running ACP agent connection.
///
/// Commands are sent to the ACP runtime thread via the `cmd_tx` channel.
/// Events (notifications, permission requests, process exit) are received
/// via the `event_rx` channel.
pub struct AcpHandle {
    cmd_tx: mpsc::UnboundedSender<AcpCommand>,
    event_rx: mpsc::UnboundedReceiver<AcpEvent>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

// SAFETY: AcpHandle only holds channel endpoints and a JoinHandle,
// all of which are Send + Sync.
unsafe impl Send for AcpHandle {}
unsafe impl Sync for AcpHandle {}

impl AcpHandle {
    /// Receive the next event from the ACP agent.
    ///
    /// Returns `None` when the connection is closed and no more events
    /// will be produced.
    pub async fn recv(&mut self) -> Option<AcpEvent> {
        self.event_rx.recv().await
    }

    /// Try to receive an event without blocking.
    pub fn try_recv(&mut self) -> Option<AcpEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Request the ACP connection to shut down gracefully.
    pub fn shutdown(&self) {
        let _ = self.cmd_tx.send(AcpCommand::Shutdown);
    }

    /// Check if the runtime thread is still alive.
    pub fn is_alive(&self) -> bool {
        self.thread_handle
            .as_ref()
            .is_some_and(|h| !h.is_finished())
    }
}

impl Drop for AcpHandle {
    fn drop(&mut self) {
        self.shutdown();
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Spawn a new ACP agent process on a dedicated runtime thread.
///
/// Returns a `Send + Sync` handle for communicating with the agent.
/// The agent process is spawned immediately; events start flowing once
/// the ACP handshake completes.
pub fn spawn(config: AcpSpawnConfig) -> Result<AcpHandle, AcpError> {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    let thread_handle = thread::Builder::new()
        .name("kild-acp-runtime".to_string())
        .spawn({
            let event_tx = event_tx.clone();
            move || {
                run_acp_thread(config, cmd_rx, event_tx);
            }
        })
        .map_err(|e| AcpError::RuntimeFailed {
            message: format!("failed to spawn ACP runtime thread: {}", e),
        })?;

    Ok(AcpHandle {
        cmd_tx,
        event_rx,
        thread_handle: Some(thread_handle),
    })
}

/// Entry point for the ACP runtime thread.
///
/// Creates a single-threaded tokio runtime with a `LocalSet` for `!Send` futures.
fn run_acp_thread(
    config: AcpSpawnConfig,
    mut cmd_rx: mpsc::UnboundedReceiver<AcpCommand>,
    event_tx: mpsc::UnboundedSender<AcpEvent>,
) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            error!(
                event = "acp.runtime.build_failed",
                error = %e,
            );
            let _ = event_tx.send(AcpEvent::Error {
                message: format!("failed to build tokio runtime: {}", e),
            });
            return;
        }
    };

    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async move {
        // Spawn and connect to the agent
        let (conn, mut child) = match connection::spawn_and_connect(&config, event_tx.clone()).await
        {
            Ok(result) => result,
            Err(e) => {
                error!(
                    event = "acp.runtime.connect_failed",
                    error = %e,
                );
                let _ = event_tx.send(AcpEvent::Error {
                    message: e.to_string(),
                });
                return;
            }
        };

        // Initialize the ACP protocol handshake
        if let Err(e) = initialize_connection(&conn).await {
            error!(
                event = "acp.runtime.init_failed",
                error = %e,
            );
            let _ = event_tx.send(AcpEvent::Error {
                message: format!("ACP initialization failed: {}", e),
            });
            return;
        }

        info!(event = "acp.runtime.ready");

        // Wait for either a command or process exit (single dispatch, not a loop)
        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(AcpCommand::Shutdown) | None => {
                        info!(event = "acp.runtime.shutdown_requested");
                        // Kill the child process
                        if let Err(e) = child.kill().await {
                            warn!(
                                event = "acp.runtime.kill_failed",
                                error = %e,
                            );
                        }
                    }
                }
            }
            status = child.wait() => {
                let exit_code = match status {
                    Ok(s) => s.code(),
                    Err(e) => {
                        warn!(
                            event = "acp.runtime.wait_failed",
                            error = %e,
                        );
                        None
                    }
                };
                info!(
                    event = "acp.runtime.process_exited",
                    exit_code = ?exit_code,
                );
                let _ = event_tx.send(AcpEvent::ProcessExited { exit_code });
            }
        }
    });
}

/// Perform the ACP protocol initialization handshake.
async fn initialize_connection(conn: &acp::ClientSideConnection) -> Result<(), AcpError> {
    use acp::Agent;

    let init_request = acp::InitializeRequest::new(acp::ProtocolVersion::V1)
        .client_info(acp::Implementation::new("kild", env!("CARGO_PKG_VERSION")).title("KILD"));

    conn.initialize(init_request)
        .await
        .map_err(|e| AcpError::InitFailed {
            message: e.to_string(),
        })?;

    info!(event = "acp.connection.initialized");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_handle_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AcpHandle>();
    }

    #[test]
    fn acp_event_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<AcpEvent>();
    }

    #[test]
    fn acp_spawn_config_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<AcpSpawnConfig>();
    }
}
