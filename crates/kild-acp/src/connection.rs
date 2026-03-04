//! ACP connection management.
//!
//! Wraps the `ClientSideConnection` from the ACP SDK, handling process
//! spawning and stdio bridging.

use std::process::Stdio;

use agent_client_protocol as acp;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::info;

use crate::client_impl::KildAcpClient;
use crate::errors::AcpError;
use crate::types::{AcpEvent, AcpSpawnConfig};

/// Spawn an ACP agent process and establish the protocol connection.
///
/// This must be called from within a `LocalSet` on a single-threaded
/// tokio runtime, because the ACP SDK produces `!Send` futures.
///
/// Returns the `ClientSideConnection` (which implements the `Agent` trait
/// for sending requests) and the child process handle.
pub(crate) async fn spawn_and_connect(
    config: &AcpSpawnConfig,
    event_tx: mpsc::UnboundedSender<AcpEvent>,
) -> Result<(acp::ClientSideConnection, Child), AcpError> {
    // Build the command, unsetting CLAUDECODE to avoid nesting detection
    let mut cmd = Command::new(&config.binary);
    cmd.args(&config.args)
        .current_dir(&config.working_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .env_remove("CLAUDECODE");

    for (key, value) in &config.env_vars {
        cmd.env(key, value);
    }

    info!(
        event = "acp.connection.spawn_started",
        binary = %config.binary,
        args = ?config.args,
        working_directory = %config.working_directory.display(),
    );

    let mut child = cmd.spawn().map_err(|e| AcpError::SpawnFailed {
        message: format!("{}: {}", config.binary, e),
    })?;

    let stdin = child.stdin.take().ok_or_else(|| AcpError::SpawnFailed {
        message: "failed to capture agent stdin".to_string(),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| AcpError::SpawnFailed {
        message: "failed to capture agent stdout".to_string(),
    })?;

    // Bridge tokio streams to futures_io (ACP SDK uses futures_io traits)
    let stdin_compat = stdin.compat_write();
    let stdout_compat = stdout.compat();

    let client = KildAcpClient::new(event_tx);

    // Wrap spawn_local to match the expected signature: Fn(LocalBoxFuture<'static, ()>) -> ()
    // tokio::task::spawn_local returns JoinHandle<()>, but ACP SDK expects ()
    let spawn_fn = |fut: futures::future::LocalBoxFuture<'static, ()>| {
        drop(tokio::task::spawn_local(fut));
    };

    let (conn, io_future) =
        acp::ClientSideConnection::new(client, stdin_compat, stdout_compat, spawn_fn);

    // Drive the I/O future — this runs the underlying JSON-RPC transport
    tokio::task::spawn_local(async move {
        let _ = io_future.await;
    });

    info!(event = "acp.connection.spawn_completed");

    Ok((conn, child))
}
