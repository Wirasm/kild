//! ACP process spawning and I/O relay.
//!
//! Spawns an ACP agent subprocess via `tokio::process::Command` with stdio pipes.
//! Provides a background stdout reader task that broadcasts output to clients,
//! and an mpsc channel for writing to stdin.

use std::path::Path;
use std::sync::{Arc, RwLock};

use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

use crate::pty::output::{PtyExitEvent, ScrollbackBuffer};

/// Result of spawning an ACP agent process.
pub struct AcpSpawnResult {
    /// Sender for writing bytes to the agent's stdin.
    pub stdin_tx: mpsc::Sender<Vec<u8>>,
    /// Broadcast sender for agent stdout output (shared with scrollback + clients).
    pub output_tx: broadcast::Sender<Bytes>,
    /// PID of the spawned agent process.
    pub pid: Option<u32>,
}

/// Spawn an ACP agent subprocess and set up I/O relay channels.
///
/// Spawns three background tasks:
/// - **stdout reader**: reads agent stdout, broadcasts to clients, feeds scrollback
/// - **stdin writer**: receives bytes from mpsc channel, writes to agent stdin
/// - **exit monitor**: detects process exit, sends `PtyExitEvent`
///
/// The `Child` is consumed by the exit monitor — callers don't hold process handles.
#[allow(clippy::too_many_arguments)]
pub fn spawn_acp_process(
    session_id: &str,
    command: &str,
    args: &[String],
    working_directory: &Path,
    env_vars: &[(String, String)],
    broadcast_capacity: usize,
    scrollback: Arc<RwLock<ScrollbackBuffer>>,
    exit_tx: mpsc::UnboundedSender<PtyExitEvent>,
) -> Result<AcpSpawnResult, crate::errors::DaemonError> {
    info!(
        event = "daemon.acp.spawn_started",
        session_id = session_id,
        command = command,
    );

    let mut cmd = Command::new(command);
    cmd.args(args)
        .current_dir(working_directory)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        // Prevent nesting detection in Claude Code
        .env_remove("CLAUDECODE");

    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    let mut child = cmd.spawn().map_err(|e| {
        crate::errors::DaemonError::PtyError(format!(
            "Failed to spawn ACP process '{}': {}",
            command, e
        ))
    })?;

    let pid = child.id();

    // Take ownership of stdin/stdout
    let child_stdin = child.stdin.take().ok_or_else(|| {
        crate::errors::DaemonError::PtyError("Failed to capture ACP process stdin".to_string())
    })?;
    let child_stdout = child.stdout.take().ok_or_else(|| {
        crate::errors::DaemonError::PtyError("Failed to capture ACP process stdout".to_string())
    })?;

    // Create broadcast channel for stdout output
    let (output_tx, _) = broadcast::channel(broadcast_capacity);

    // Create mpsc channel for stdin writes
    let (stdin_tx, stdin_rx) = mpsc::channel::<Vec<u8>>(256);

    // Spawn stdout reader task
    let reader_tx = output_tx.clone();
    let sid = session_id.to_string();
    tokio::spawn(async move {
        acp_stdout_reader(sid, child_stdout, reader_tx, scrollback).await;
    });

    // Spawn stdin writer task
    let sid = session_id.to_string();
    tokio::spawn(async move {
        acp_stdin_writer(sid, child_stdin, stdin_rx).await;
    });

    // Spawn exit monitor task (consumes child)
    let sid = session_id.to_string();
    tokio::spawn(async move {
        acp_exit_monitor(sid, child, exit_tx).await;
    });

    info!(
        event = "daemon.acp.spawn_completed",
        session_id = session_id,
        pid = ?pid,
    );

    Ok(AcpSpawnResult {
        stdin_tx,
        output_tx,
        pid,
    })
}

/// Read stdout from the ACP agent and broadcast to clients + scrollback.
async fn acp_stdout_reader(
    session_id: String,
    mut stdout: tokio::process::ChildStdout,
    output_tx: broadcast::Sender<Bytes>,
    scrollback: Arc<RwLock<ScrollbackBuffer>>,
) {
    let mut buf = vec![0u8; 4096];
    loop {
        match stdout.read(&mut buf).await {
            Ok(0) => {
                debug!(
                    event = "daemon.acp.stdout_eof",
                    session_id = session_id.as_str(),
                );
                break;
            }
            Ok(n) => {
                let chunk = Bytes::copy_from_slice(&buf[..n]);

                // Feed scrollback buffer
                if let Ok(mut sb) = scrollback.write() {
                    sb.push(&chunk);
                }

                // Broadcast to attached clients (ignore send errors — no subscribers is OK)
                let _ = output_tx.send(chunk);
            }
            Err(e) => {
                warn!(
                    event = "daemon.acp.stdout_read_failed",
                    session_id = session_id.as_str(),
                    error = %e,
                );
                break;
            }
        }
    }
}

/// Write bytes from the mpsc channel to the ACP agent's stdin.
async fn acp_stdin_writer(
    session_id: String,
    mut stdin: tokio::process::ChildStdin,
    mut rx: mpsc::Receiver<Vec<u8>>,
) {
    while let Some(data) = rx.recv().await {
        if let Err(e) = stdin.write_all(&data).await {
            warn!(
                event = "daemon.acp.stdin_write_failed",
                session_id = session_id.as_str(),
                error = %e,
            );
            break;
        }
        if let Err(e) = stdin.flush().await {
            warn!(
                event = "daemon.acp.stdin_flush_failed",
                session_id = session_id.as_str(),
                error = %e,
            );
            break;
        }
    }
    debug!(
        event = "daemon.acp.stdin_writer_stopped",
        session_id = session_id.as_str(),
    );
}

/// Wait for the ACP agent process to exit and notify the session manager.
///
/// Reuses `PtyExitEvent` since the daemon treats PTY and ACP exits uniformly.
async fn acp_exit_monitor(
    session_id: String,
    mut child: tokio::process::Child,
    exit_tx: mpsc::UnboundedSender<PtyExitEvent>,
) {
    let status = child.wait().await;
    let exit_code = match status {
        Ok(s) => s.code(),
        Err(e) => {
            error!(
                event = "daemon.acp.wait_failed",
                session_id = session_id.as_str(),
                error = %e,
            );
            None
        }
    };

    info!(
        event = "daemon.acp.process_exited",
        session_id = session_id.as_str(),
        exit_code = ?exit_code,
    );

    let _ = exit_tx.send(PtyExitEvent {
        session_id: session_id.clone(),
    });
}
