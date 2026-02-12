//! Spike 1: Async daemon client using smol on GPUI's BackgroundExecutor.
//!
//! Validates that `smol::Async<UnixStream>` works when polled by GPUI's
//! GCD-based task scheduler. Sends a Ping to the kild daemon and reads
//! back an Ack — the simplest possible roundtrip.

use std::os::unix::net::UnixStream;

use kild_protocol::{ClientMessage, DaemonMessage};
use smol::Async;
use smol::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use thiserror::Error;
use tracing::{debug, error, info, warn};

#[derive(Debug, Error)]
pub enum DaemonClientError {
    #[error("failed to connect to daemon: {0}")]
    Connect(std::io::Error),

    #[error("failed to serialize request: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("daemon closed connection (EOF)")]
    ConnectionClosed,

    #[error("daemon sent empty response")]
    EmptyResponse,

    #[error("invalid JSON from daemon: {source}: {json}")]
    InvalidJson {
        source: serde_json::Error,
        json: String,
    },

    #[error("unexpected response from daemon: {0:?}")]
    UnexpectedResponse(DaemonMessage),
}

/// Async ping to the kild daemon via smol.
///
/// Returns `Ok(true)` if daemon responded with Ack, `Ok(false)` if daemon
/// is not running (socket missing or connection refused), `Err` for
/// unexpected failures.
pub async fn ping_daemon_async() -> Result<bool, DaemonClientError> {
    let socket_path = kild_core::daemon::socket_path();

    debug!(event = "ui.daemon.ping_started");

    if !socket_path.exists() {
        info!(
            event = "ui.daemon.ping_completed",
            result = "socket_missing"
        );
        return Ok(false);
    }

    let mut stream = match Async::<UnixStream>::connect(&socket_path).await {
        Ok(s) => s,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::ConnectionRefused {
                info!(
                    event = "ui.daemon.ping_completed",
                    result = "connection_refused"
                );
                return Ok(false);
            }
            error!(
                event = "ui.daemon.ping_failed",
                error = %e,
            );
            return Err(DaemonClientError::Connect(e));
        }
    };

    let request = ClientMessage::Ping {
        id: "spike1-ping".to_string(),
    };

    // Write Ping as JSONL
    let json = serde_json::to_string(&request)?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;

    // Read Ack response (hand ownership to BufReader — done writing)
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        return Err(DaemonClientError::ConnectionClosed);
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(DaemonClientError::EmptyResponse);
    }
    let response: DaemonMessage =
        serde_json::from_str(trimmed).map_err(|e| DaemonClientError::InvalidJson {
            source: e,
            json: trimmed.to_string(),
        })?;

    match response {
        DaemonMessage::Ack { .. } => {
            info!(event = "ui.daemon.ping_completed", result = "ack");
            Ok(true)
        }
        other => {
            warn!(
                event = "ui.daemon.ping_completed",
                result = "unexpected_response",
                response = ?other,
            );
            Err(DaemonClientError::UnexpectedResponse(other))
        }
    }
}
