use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use kild_protocol::{ClientMessage, DaemonMessage};
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
    mut stream: UnixStream,
    request: &ClientMessage,
    operation: &str,
) -> Result<DaemonMessage, ShimError> {
    let msg = serde_json::to_string(request)
        .map_err(|e| ShimError::ipc(format!("{}: serialization failed: {}", operation, e)))?;

    writeln!(stream, "{}", msg)?;
    stream.flush()?;

    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    if line.is_empty() {
        return Err(ShimError::ipc(format!(
            "{}: empty response from daemon",
            operation
        )));
    }

    let response: DaemonMessage = serde_json::from_str(&line)
        .map_err(|e| ShimError::ipc(format!("{}: invalid JSON response: {}", operation, e)))?;

    if let DaemonMessage::Error { code, message, .. } = &response {
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

    let request = ClientMessage::CreateSession {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        working_directory: working_directory.to_string(),
        command: command.to_string(),
        args: args.to_vec(),
        env_vars: env_vars.clone(),
        rows,
        cols,
        use_login_shell,
    };

    let stream = connect()?;
    let response = send_request(stream, &request, "create_session")?;

    let daemon_session_id = match response {
        DaemonMessage::SessionCreated { session, .. } => session.id,
        _ => {
            return Err(ShimError::ipc(
                "create_session: expected SessionCreated response",
            ));
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

    let encoded = base64::engine::general_purpose::STANDARD.encode(data);

    let request = ClientMessage::WriteStdin {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        data: encoded,
    };

    let stream = connect()?;
    send_request(stream, &request, "write_stdin")?;

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
        session_id: session_id.to_string(),
        force,
    };

    let stream = connect()?;
    send_request(stream, &request, "destroy_session")?;

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
        session_id: session_id.to_string(),
    };

    let stream = connect()?;
    let response = send_request(stream, &request, "read_scrollback")?;

    let decoded = match response {
        DaemonMessage::ScrollbackContents { data, .. } => base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| ShimError::ipc(format!("read_scrollback: base64 decode failed: {}", e)))?,
        _ => {
            return Err(ShimError::ipc(
                "read_scrollback: expected ScrollbackContents response",
            ));
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
        session_id: session_id.to_string(),
        rows,
        cols,
    };

    let stream = connect()?;
    send_request(stream, &request, "resize_pty")?;

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
            let response = r#"{"type":"error","id":"1","code":"session_not_found","message":"no such session"}"#;
            writeln!(stream, "{}", response).unwrap();
            stream.flush().unwrap();
        });

        // Connect to mock server
        let stream = UnixStream::connect(&sock_path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let request = ClientMessage::Ping {
            id: "test".to_string(),
        };
        let result = send_request(stream, &request, "test_op");
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

        let stream = UnixStream::connect(&sock_path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let request = ClientMessage::Ping {
            id: "test".to_string(),
        };
        let result = send_request(stream, &request, "test_op");
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

        let stream = UnixStream::connect(&sock_path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let request = ClientMessage::Ping {
            id: "test".to_string(),
        };
        let result = send_request(stream, &request, "test_op");
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
            let response = r#"{"type":"ack","id":"test-123"}"#;
            writeln!(stream, "{}", response).unwrap();
            stream.flush().unwrap();
        });

        let stream = UnixStream::connect(&sock_path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let request = ClientMessage::Ping {
            id: "test".to_string(),
        };
        let result = send_request(stream, &request, "test_op");
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(matches!(response, DaemonMessage::Ack { .. }));

        handle.join().unwrap();
    }

    #[test]
    fn test_read_scrollback_success() {
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
            // Known-good base64 for b"hello world"
            let response =
                r#"{"type":"scrollback_contents","id":"test","data":"aGVsbG8gd29ybGQ="}"#;
            writeln!(stream, "{}", response).unwrap();
            stream.flush().unwrap();
        });

        let stream = UnixStream::connect(&sock_path).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let request = ClientMessage::ReadScrollback {
            id: "test".to_string(),
            session_id: "test-session".to_string(),
        };
        let response = send_request(stream, &request, "read_scrollback").unwrap();

        if let DaemonMessage::ScrollbackContents { data, .. } = response {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(data)
                .unwrap();
            assert_eq!(&decoded, b"hello world");
        } else {
            panic!("expected ScrollbackContents, got: {:?}", response);
        }

        handle.join().unwrap();
    }
}
