//! Spike 1: Async daemon client using smol on GPUI's BackgroundExecutor.
//!
//! Validates that `smol::Async<UnixStream>` works when polled by GPUI's
//! GCD-based task scheduler. Sends a Ping to the kild daemon and reads
//! back an Ack — the simplest possible roundtrip.

use std::os::unix::net::UnixStream;

use kild_protocol::{ClientMessage, DaemonMessage};
use smol::Async;
use smol::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info, warn};

/// Async ping to the kild daemon via smol.
///
/// Returns `Ok(true)` if daemon responded with Ack, `Ok(false)` if daemon
/// is not running (socket missing or connection refused), `Err` for
/// unexpected failures.
pub async fn ping_daemon_async() -> Result<bool, String> {
    let socket_path = kild_core::daemon::socket_path();

    debug!(event = "ui.daemon.ping_async_started");

    if !socket_path.exists() {
        info!(
            event = "ui.daemon.ping_async_completed",
            result = "socket_missing"
        );
        return Ok(false);
    }

    let mut stream = match Async::<UnixStream>::connect(&socket_path).await {
        Ok(s) => s,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::ConnectionRefused {
                info!(
                    event = "ui.daemon.ping_async_completed",
                    result = "connection_refused"
                );
                return Ok(false);
            }
            error!(
                event = "ui.daemon.ping_async_failed",
                error = %e,
            );
            return Err(format!("connect failed: {}", e));
        }
    };

    let request = ClientMessage::Ping {
        id: "spike1-ping".to_string(),
    };

    // Write Ping as JSONL
    let json = serde_json::to_string(&request).map_err(|e| format!("serialize failed: {}", e))?;
    stream
        .write_all(json.as_bytes())
        .await
        .map_err(|e| format!("write failed: {}", e))?;
    stream
        .write_all(b"\n")
        .await
        .map_err(|e| format!("write newline failed: {}", e))?;
    stream
        .flush()
        .await
        .map_err(|e| format!("flush failed: {}", e))?;

    // Read Ack response (hand ownership to BufReader — done writing)
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let bytes_read = reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("read failed: {}", e))?;
    if bytes_read == 0 {
        return Err("daemon closed connection (EOF)".to_string());
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err("daemon sent empty line".to_string());
    }
    let response: DaemonMessage = serde_json::from_str(trimmed)
        .map_err(|e| format!("invalid JSON from daemon: {}: {}", e, trimmed))?;

    match response {
        DaemonMessage::Ack { .. } => {
            info!(event = "ui.daemon.ping_async_completed", result = "ack");
            Ok(true)
        }
        other => {
            warn!(
                event = "ui.daemon.ping_async_completed",
                result = "unexpected_response",
                response = ?other,
            );
            Err(format!("unexpected response: {:?}", other))
        }
    }
}
