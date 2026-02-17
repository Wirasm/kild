use std::collections::HashMap;
use std::path::PathBuf;

use base64::Engine;
use kild_paths::KildPaths;
use kild_protocol::{ClientMessage, DaemonMessage, IpcConnection, SessionId};
use tracing::debug;

use crate::errors::ShimError;

fn socket_path() -> Result<PathBuf, ShimError> {
    let paths = KildPaths::resolve().map_err(|e| ShimError::state(e.to_string()))?;
    Ok(paths.daemon_socket())
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

    let mut conn = IpcConnection::connect(&socket_path()?)?;
    let response = conn.send(&request)?;

    let daemon_session_id = match response {
        DaemonMessage::SessionCreated { session, .. } => session.id.into_inner(),
        other => {
            return Err(ShimError::ipc(format!(
                "create_session for {}: expected SessionCreated, got {:?}",
                session_id, other
            )));
        }
    };

    debug!(
        event = "shim.ipc.create_session_completed",
        daemon_session_id = daemon_session_id,
    );

    Ok(daemon_session_id)
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

    let mut conn = IpcConnection::connect(&socket_path()?)?;
    conn.send(&request)?;

    debug!(
        event = "shim.ipc.write_stdin_completed",
        session_id = session_id
    );
    Ok(())
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

    let mut conn = IpcConnection::connect(&socket_path()?)?;
    conn.send(&request)?;

    debug!(
        event = "shim.ipc.destroy_session_completed",
        session_id = session_id
    );
    Ok(())
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

    let mut conn = IpcConnection::connect(&socket_path()?)?;
    let response = conn.send(&request)?;

    let decoded = match response {
        DaemonMessage::ScrollbackContents { data, .. } => base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| {
                ShimError::ipc(format!(
                    "read_scrollback for {}: base64 decode failed: {}",
                    session_id, e
                ))
            })?,
        other => {
            return Err(ShimError::ipc(format!(
                "read_scrollback for {}: expected ScrollbackContents, got {:?}",
                session_id, other
            )));
        }
    };

    debug!(
        event = "shim.ipc.read_scrollback_completed",
        session_id = session_id,
        bytes = decoded.len(),
    );

    Ok(decoded)
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

    let mut conn = IpcConnection::connect(&socket_path()?)?;
    conn.send(&request)?;

    debug!(
        event = "shim.ipc.resize_pty_completed",
        session_id = session_id
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_daemon_not_running() {
        // With no daemon socket file, connect should return DaemonNotRunning.
        // Skip if daemon happens to be running.
        let path = socket_path().unwrap();
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
        let path = socket_path().unwrap();
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
        let path = socket_path().unwrap();
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
