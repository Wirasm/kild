//! Integration tests for the kild-daemon client-server roundtrip.
//!
//! These tests start a real server on a temp socket, connect via `DaemonClient`,
//! and exercise the full IPC protocol.

use std::collections::HashMap;
use std::time::Duration;

use kild_daemon::client::DaemonClient;
use kild_daemon::types::DaemonConfig;

/// Create a DaemonConfig pointing at a temp directory for test isolation.
fn test_config(dir: &std::path::Path) -> DaemonConfig {
    DaemonConfig {
        socket_path: dir.join("daemon.sock"),
        pid_path: dir.join("daemon.pid"),
        scrollback_buffer_size: 4096,
        pty_output_batch_ms: 4,
        client_buffer_size: 65536,
        shutdown_timeout_secs: 2,
    }
}

#[tokio::test]
async fn test_ping_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let socket_path = config.socket_path.clone();

    // Start server in background
    let server_handle = tokio::spawn(async move { kild_daemon::run_server(config).await });

    // Wait for server to be ready
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Connect client
    let mut client = DaemonClient::connect(&socket_path).await.unwrap();

    // List sessions (should be empty)
    let sessions = client.list_sessions(None).await.unwrap();
    assert!(sessions.is_empty());

    // Shutdown
    client.shutdown().await.unwrap();

    // Wait for server to exit
    let result = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_session_and_list() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let socket_path = config.socket_path.clone();

    let server_handle = tokio::spawn(async move { kild_daemon::run_server(config).await });

    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut client = DaemonClient::connect(&socket_path).await.unwrap();

    // Create a session running /bin/sh
    let session = client
        .create_session(
            "test-session",
            "/tmp",
            "/bin/sh",
            &[],
            &HashMap::new(),
            24,
            80,
        )
        .await
        .unwrap();

    assert_eq!(session.id, "test-session");
    assert_eq!(session.command, "/bin/sh");
    assert_eq!(session.status, "running");

    // List sessions
    let sessions = client.list_sessions(None).await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "test-session");

    // Get specific session
    let info = client.get_session("test-session").await.unwrap();
    assert_eq!(info.command, "/bin/sh");

    // Stop the session
    client.stop_session("test-session").await.unwrap();

    // Shutdown
    client.shutdown().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_attach_and_read_output() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let socket_path = config.socket_path.clone();

    let server_handle = tokio::spawn(async move { kild_daemon::run_server(config).await });

    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut client = DaemonClient::connect(&socket_path).await.unwrap();

    // Create a session running /bin/sh
    let working_dir = dir.path().to_string_lossy().to_string();
    let _session = client
        .create_session(
            "echo-test",
            &working_dir,
            "/bin/sh",
            &[],
            &HashMap::new(),
            24,
            80,
        )
        .await
        .unwrap();

    // Attach
    client.attach("echo-test", 24, 80).await.unwrap();

    // Write a command to stdin
    client
        .write_stdin("echo-test", b"echo hello\n")
        .await
        .unwrap();

    // Read some output (with timeout)
    let read_result = tokio::time::timeout(Duration::from_secs(2), async {
        let mut got_output = false;
        for _ in 0..10 {
            match client.read_next().await {
                Ok(Some(msg)) => {
                    if let kild_daemon::DaemonMessage::PtyOutput { data, .. } = &msg {
                        if !data.is_empty() {
                            got_output = true;
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
        got_output
    })
    .await;

    assert!(
        read_result.unwrap_or(false),
        "Should have received PTY output"
    );

    // We need a fresh connection for further requests since the current one
    // is in streaming mode
    let mut client2 = DaemonClient::connect(&socket_path).await.unwrap();

    // Stop and destroy
    client2.stop_session("echo-test").await.unwrap();
    client2.shutdown().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_session_not_found_error() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let socket_path = config.socket_path.clone();

    let server_handle = tokio::spawn(async move { kild_daemon::run_server(config).await });

    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut client = DaemonClient::connect(&socket_path).await.unwrap();

    // Try to get a non-existent session
    let result = client.get_session("nonexistent").await;
    assert!(result.is_err());

    // Try to stop a non-existent session
    let result = client.stop_session("nonexistent").await;
    assert!(result.is_err());

    client.shutdown().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    assert!(result.is_ok());
}
