use std::collections::HashMap;

use base64::Engine;
use kild_paths::KildPaths;
use kild_protocol::{ClientMessage, DaemonMessage, ErrorCode, SessionId, SessionStatus};
use tracing::{debug, warn};

use crate::errors::ShimError;

/// Take a connection from the pool, or create a fresh one.
///
/// Delegates to `kild_protocol::pool` for thread-local connection caching.
/// Critical for `write_stdin()` which is called per-keystroke — avoids
/// creating a new socket connection for every key press.
fn get_or_connect() -> Result<kild_protocol::IpcConnection, ShimError> {
    let paths = KildPaths::resolve().map_err(|e| ShimError::state(e.to_string()))?;
    let (conn, reused) = kild_protocol::pool::take(&paths.daemon_socket())?;
    if reused {
        debug!(event = "shim.ipc.connection_reused");
    } else {
        debug!(event = "shim.ipc.connection_created");
    }
    Ok(conn)
}

/// Return a connection to the pool for reuse.
///
/// Delegates to `kild_protocol::pool::release` which re-validates liveness
/// before caching.
fn return_conn(conn: kild_protocol::IpcConnection) {
    if kild_protocol::pool::release(conn) {
        debug!(event = "shim.ipc.connection_cached");
    } else {
        debug!(event = "shim.ipc.connection_dropped_on_return");
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_session(
    session_id: &str,
    working_directory: &str,
    command: &str,
    args: &[String],
    env_vars: &HashMap<String, String>,
    rows: u16,
    cols: u16,
    use_login_shell: bool,
) -> Result<String, ShimError> {
    debug!(
        event = "shim.ipc.create_session_started",
        session_id = session_id,
        command = command,
    );

    let request = ClientMessage::CreateSession {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: SessionId::new(session_id),
        working_directory: working_directory.to_string(),
        command: command.to_string(),
        args: args.to_vec(),
        env_vars: env_vars.clone(),
        rows,
        cols,
        use_login_shell,
    };

    let mut conn = get_or_connect()?;
    match conn.send(&request) {
        Ok(DaemonMessage::SessionCreated { session, .. }) => {
            let daemon_session_id = session.id.into_inner();
            return_conn(conn);
            debug!(
                event = "shim.ipc.create_session_completed",
                daemon_session_id = daemon_session_id.as_str(),
            );
            Ok(daemon_session_id)
        }
        Ok(_) => Err(ShimError::ipc(
            "create_session: expected SessionCreated response",
        )),
        Err(e) => {
            warn!(
                event = "shim.ipc.create_session_failed",
                session_id = session_id,
                error = %e,
            );
            Err(e.into())
        }
    }
}

pub fn write_stdin(session_id: &str, data: &[u8]) -> Result<(), ShimError> {
    debug!(
        event = "shim.ipc.write_stdin_started",
        session_id = session_id,
        bytes = data.len(),
    );

    // Pre-size the base64 buffer: base64 output is ceil(input_len / 3) * 4
    let encoded_len = data.len().div_ceil(3) * 4;
    let mut encoded = String::with_capacity(encoded_len);
    base64::engine::general_purpose::STANDARD.encode_string(data, &mut encoded);

    let request = ClientMessage::WriteStdin {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: SessionId::new(session_id),
        data: encoded,
    };

    let mut conn = get_or_connect()?;
    match conn.send(&request) {
        Ok(_) => {
            return_conn(conn);
            debug!(
                event = "shim.ipc.write_stdin_completed",
                session_id = session_id
            );
            Ok(())
        }
        Err(e) => {
            warn!(
                event = "shim.ipc.write_stdin_failed",
                session_id = session_id,
                bytes = data.len(),
                error = %e,
            );
            Err(e.into())
        }
    }
}

pub fn destroy_session(session_id: &str, force: bool) -> Result<(), ShimError> {
    debug!(
        event = "shim.ipc.destroy_session_started",
        session_id = session_id,
        force = force,
    );

    let request = ClientMessage::DestroySession {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: SessionId::new(session_id),
        force,
    };

    let mut conn = get_or_connect()?;
    match conn.send(&request) {
        Ok(_) => {
            // Don't cache — session is being destroyed
            debug!(
                event = "shim.ipc.destroy_session_completed",
                session_id = session_id
            );
            Ok(())
        }
        Err(e) => {
            warn!(
                event = "shim.ipc.destroy_session_failed",
                session_id = session_id,
                error = %e,
            );
            Err(e.into())
        }
    }
}

pub fn read_scrollback(session_id: &str) -> Result<Vec<u8>, ShimError> {
    debug!(
        event = "shim.ipc.read_scrollback_started",
        session_id = session_id,
    );

    let request = ClientMessage::ReadScrollback {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: SessionId::new(session_id),
    };

    let mut conn = get_or_connect()?;
    match conn.send(&request) {
        Ok(DaemonMessage::ScrollbackContents { data, .. }) => {
            return_conn(conn);
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|e| {
                    ShimError::ipc(format!("read_scrollback: base64 decode failed: {}", e))
                })?;
            debug!(
                event = "shim.ipc.read_scrollback_completed",
                session_id = session_id,
                bytes = decoded.len(),
            );
            Ok(decoded)
        }
        Ok(_) => Err(ShimError::ipc(
            "read_scrollback: expected ScrollbackContents response",
        )),
        Err(e) => {
            warn!(
                event = "shim.ipc.read_scrollback_failed",
                session_id = session_id,
                error = %e,
            );
            Err(e.into())
        }
    }
}

/// Query session status and PID from the daemon.
///
/// Returns `(status, pty_pid, exit_code)`. On failure (daemon down, session not found),
/// returns `(Stopped, None, None)` as a safe default — a missing session is effectively dead.
pub fn get_session_status(session_id: &str) -> (SessionStatus, Option<u32>, Option<i32>) {
    let request = ClientMessage::GetSession {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: SessionId::new(session_id),
    };

    let mut conn = match get_or_connect() {
        Ok(c) => c,
        Err(e) => {
            warn!(
                event = "shim.ipc.get_session_status_failed",
                session_id = session_id,
                error = %e,
            );
            return (SessionStatus::Stopped, None, None);
        }
    };

    match conn.send(&request) {
        Ok(DaemonMessage::SessionInfo { session, .. }) => {
            let result = (session.status, session.pty_pid, session.exit_code);
            return_conn(conn);
            result
        }
        Ok(DaemonMessage::Error { code, message, .. }) => {
            // SessionNotFound is expected (session cleaned up) — debug only
            if matches!(code, ErrorCode::SessionNotFound) {
                debug!(
                    event = "shim.ipc.get_session_status_not_found",
                    session_id = session_id,
                );
            } else {
                warn!(
                    event = "shim.ipc.get_session_status_failed",
                    session_id = session_id,
                    error_code = %code,
                    error = %message,
                );
            }
            return_conn(conn);
            (SessionStatus::Stopped, None, None)
        }
        Ok(unexpected) => {
            // Unexpected response type — log and return healthy connection
            warn!(
                event = "shim.ipc.get_session_status_failed",
                session_id = session_id,
                reason = "unexpected_response_type",
                response = ?unexpected,
            );
            return_conn(conn);
            (SessionStatus::Stopped, None, None)
        }
        Err(e) => {
            // IPC error — connection is broken, don't return it
            warn!(
                event = "shim.ipc.get_session_status_failed",
                session_id = session_id,
                error = %e,
            );
            (SessionStatus::Stopped, None, None)
        }
    }
}

#[allow(dead_code)]
pub fn resize_pty(session_id: &str, rows: u16, cols: u16) -> Result<(), ShimError> {
    debug!(
        event = "shim.ipc.resize_pty_started",
        session_id = session_id,
        rows = rows,
        cols = cols,
    );

    let request = ClientMessage::ResizePty {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: SessionId::new(session_id),
        rows,
        cols,
    };

    let mut conn = get_or_connect()?;
    match conn.send(&request) {
        Ok(_) => {
            return_conn(conn);
            debug!(
                event = "shim.ipc.resize_pty_completed",
                session_id = session_id
            );
            Ok(())
        }
        Err(e) => {
            warn!(
                event = "shim.ipc.resize_pty_failed",
                session_id = session_id,
                error = %e,
            );
            Err(e.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn daemon_socket_path() -> std::path::PathBuf {
        KildPaths::resolve().unwrap().daemon_socket()
    }

    #[test]
    fn test_connect_daemon_not_running() {
        // With no daemon socket file, connect should return DaemonNotRunning.
        // Skip if daemon happens to be running.
        let path = daemon_socket_path();
        if path.exists() {
            // Daemon might be running — can't reliably test DaemonNotRunning
            return;
        }

        let result = create_session(
            "test-session",
            "/tmp",
            "/bin/sh",
            &[],
            &HashMap::new(),
            24,
            80,
            true,
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            ShimError::DaemonNotRunning => {} // expected
            other => panic!("expected DaemonNotRunning, got: {:?}", other),
        }
    }

    #[test]
    fn test_write_stdin_daemon_not_running() {
        let path = daemon_socket_path();
        if path.exists() {
            return;
        }

        let result = write_stdin("test-session", b"hello");
        assert!(result.is_err());
        match result.unwrap_err() {
            ShimError::DaemonNotRunning => {}
            other => panic!("expected DaemonNotRunning, got: {:?}", other),
        }
    }

    #[test]
    fn test_destroy_session_daemon_not_running() {
        let path = daemon_socket_path();
        if path.exists() {
            return;
        }

        let result = destroy_session("test-session", false);
        assert!(result.is_err());
        match result.unwrap_err() {
            ShimError::DaemonNotRunning => {}
            other => panic!("expected DaemonNotRunning, got: {:?}", other),
        }
    }
}
