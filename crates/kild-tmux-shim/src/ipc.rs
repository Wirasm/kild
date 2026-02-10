use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use tracing::debug;

use crate::errors::ShimError;

fn socket_path() -> Result<PathBuf, ShimError> {
    let home = dirs::home_dir()
        .ok_or_else(|| ShimError::state("home directory not found - $HOME not set"))?;
    Ok(home.join(".kild").join("daemon.sock"))
}

fn connect() -> Result<UnixStream, ShimError> {
    let path = socket_path()?;
    if !path.exists() {
        return Err(ShimError::DaemonNotRunning);
    }

    let stream = UnixStream::connect(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::ConnectionRefused {
            ShimError::DaemonNotRunning
        } else {
            ShimError::ipc(format!("connection failed: {}", e))
        }
    })?;

    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    Ok(stream)
}

fn send_request(
    stream: &mut UnixStream,
    request: serde_json::Value,
    operation: &str,
) -> Result<serde_json::Value, ShimError> {
    let msg = serde_json::to_string(&request)
        .map_err(|e| ShimError::ipc(format!("{}: serialization failed: {}", operation, e)))?;

    writeln!(stream, "{}", msg)?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    if line.is_empty() {
        return Err(ShimError::ipc(format!(
            "{}: empty response from daemon",
            operation
        )));
    }

    let response: serde_json::Value = serde_json::from_str(&line)
        .map_err(|e| ShimError::ipc(format!("{}: invalid JSON response: {}", operation, e)))?;

    if response.get("type").and_then(|t| t.as_str()) == Some("error") {
        let code = response
            .get("code")
            .and_then(|c| c.as_str())
            .unwrap_or("unknown");
        let message = response
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown daemon error");
        return Err(ShimError::ipc(format!(
            "{}: [{}] {}",
            operation, code, message
        )));
    }

    Ok(response)
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

    let env_map: serde_json::Map<String, serde_json::Value> = env_vars
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();

    let request = serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "type": "create_session",
        "session_id": session_id,
        "working_directory": working_directory,
        "command": command,
        "args": args,
        "env_vars": env_map,
        "rows": rows,
        "cols": cols,
        "use_login_shell": use_login_shell,
    });

    let mut stream = connect()?;
    let response = send_request(&mut stream, request, "create_session")?;

    let daemon_session_id = response
        .get("session")
        .and_then(|s| s.get("id"))
        .and_then(|id| id.as_str())
        .ok_or_else(|| ShimError::ipc("create_session: response missing session.id"))?
        .to_string();

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

    let encoded = base64::engine::general_purpose::STANDARD.encode(data);

    let request = serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "type": "write_stdin",
        "session_id": session_id,
        "data": encoded,
    });

    let mut stream = connect()?;
    send_request(&mut stream, request, "write_stdin")?;

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

    let request = serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "type": "destroy_session",
        "session_id": session_id,
        "force": force,
    });

    let mut stream = connect()?;
    send_request(&mut stream, request, "destroy_session")?;

    debug!(
        event = "shim.ipc.destroy_session_completed",
        session_id = session_id
    );
    Ok(())
}

#[allow(dead_code)]
pub fn resize_pty(session_id: &str, rows: u16, cols: u16) -> Result<(), ShimError> {
    debug!(
        event = "shim.ipc.resize_pty_started",
        session_id = session_id,
        rows = rows,
        cols = cols,
    );

    let request = serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "type": "resize_pty",
        "session_id": session_id,
        "rows": rows,
        "cols": cols,
    });

    let mut stream = connect()?;
    send_request(&mut stream, request, "resize_pty")?;

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

    #[test]
    fn test_send_request_error_response() {
        use std::os::unix::net::UnixListener;

        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        // Spawn a mock server that returns an error response
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(&stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap(); // read the request

            use std::io::Write;
            let response =
                r#"{"type":"error","code":"session_not_found","message":"no such session"}"#;
            writeln!(stream, "{}", response).unwrap();
            stream.flush().unwrap();
        });

        // Connect to mock server
        let mut stream = UnixStream::connect(&sock_path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let request = serde_json::json!({"type": "test"});
        let result = send_request(&mut stream, request, "test_op");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("session_not_found"), "got: {}", err);

        handle.join().unwrap();
    }

    #[test]
    fn test_send_request_empty_response() {
        use std::os::unix::net::UnixListener;

        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(&stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap(); // read request
            // Close without sending response
            drop(stream);
        });

        let mut stream = UnixStream::connect(&sock_path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let request = serde_json::json!({"type": "test"});
        let result = send_request(&mut stream, request, "test_op");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("empty response"), "got: {}", err);

        handle.join().unwrap();
    }

    #[test]
    fn test_send_request_invalid_json() {
        use std::os::unix::net::UnixListener;

        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(&stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();

            use std::io::Write;
            writeln!(stream, "not-json{{").unwrap();
            stream.flush().unwrap();
        });

        let mut stream = UnixStream::connect(&sock_path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let request = serde_json::json!({"type": "test"});
        let result = send_request(&mut stream, request, "test_op");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid JSON"), "got: {}", err);

        handle.join().unwrap();
    }

    #[test]
    fn test_send_request_success() {
        use std::os::unix::net::UnixListener;

        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(&stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();

            use std::io::Write;
            let response = r#"{"type":"ok","session":{"id":"test-123"}}"#;
            writeln!(stream, "{}", response).unwrap();
            stream.flush().unwrap();
        });

        let mut stream = UnixStream::connect(&sock_path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let request = serde_json::json!({"type": "test"});
        let result = send_request(&mut stream, request, "test_op");
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val["type"], "ok");
        assert_eq!(val["session"]["id"], "test-123");

        handle.join().unwrap();
    }
}
