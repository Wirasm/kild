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
            false,
        )
        .await
        .unwrap();

    assert_eq!(session.id, "test-session");
    assert_eq!(session.command, "/bin/sh");
    assert_eq!(session.status, kild_protocol::SessionStatus::Running);

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
            false,
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

#[tokio::test]
async fn test_duplicate_session_id_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let socket_path = config.socket_path.clone();

    let server_handle = tokio::spawn(async move { kild_daemon::run_server(config).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut client = DaemonClient::connect(&socket_path).await.unwrap();

    // Create first session
    client
        .create_session(
            "dup-test",
            "/tmp",
            "/bin/sh",
            &[],
            &HashMap::new(),
            24,
            80,
            false,
        )
        .await
        .unwrap();

    // Try to create a session with the same ID
    let result = client
        .create_session(
            "dup-test",
            "/tmp",
            "/bin/sh",
            &[],
            &HashMap::new(),
            24,
            80,
            false,
        )
        .await;
    assert!(result.is_err());

    // Clean up
    client.stop_session("dup-test").await.unwrap();
    client.shutdown().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_session_with_invalid_command() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let socket_path = config.socket_path.clone();

    let server_handle = tokio::spawn(async move { kild_daemon::run_server(config).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut client = DaemonClient::connect(&socket_path).await.unwrap();

    // Try to create a session with a non-existent command
    let result = client
        .create_session(
            "bad-cmd",
            "/tmp",
            "/nonexistent/command/that/does/not/exist",
            &[],
            &HashMap::new(),
            24,
            80,
            false,
        )
        .await;
    assert!(result.is_err(), "Should fail with invalid command");

    client.shutdown().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_multiple_clients_attach_to_session() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let socket_path = config.socket_path.clone();

    let server_handle = tokio::spawn(async move { kild_daemon::run_server(config).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Create session with first client
    let mut client1 = DaemonClient::connect(&socket_path).await.unwrap();
    client1
        .create_session(
            "multi-attach",
            "/tmp",
            "/bin/sh",
            &[],
            &HashMap::new(),
            24,
            80,
            false,
        )
        .await
        .unwrap();

    // Attach first client
    client1.attach("multi-attach", 24, 80).await.unwrap();

    // Attach second client
    let mut client2 = DaemonClient::connect(&socket_path).await.unwrap();
    client2.attach("multi-attach", 24, 80).await.unwrap();

    // Attach third client
    let mut client3 = DaemonClient::connect(&socket_path).await.unwrap();
    client3.attach("multi-attach", 24, 80).await.unwrap();

    // Verify session still running with a fresh connection
    let mut admin_client = DaemonClient::connect(&socket_path).await.unwrap();
    let info = admin_client.get_session("multi-attach").await.unwrap();
    assert_eq!(info.status, kild_protocol::SessionStatus::Running);
    assert_eq!(info.client_count, Some(3));

    // Clean up
    admin_client.stop_session("multi-attach").await.unwrap();
    admin_client.shutdown().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_pty_exit_transitions_session_to_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let socket_path = config.socket_path.clone();

    let server_handle = tokio::spawn(async move { kild_daemon::run_server(config).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut client = DaemonClient::connect(&socket_path).await.unwrap();

    // Create a session that will exit immediately (run `true` which exits 0)
    client
        .create_session(
            "exit-test",
            "/tmp",
            "/usr/bin/true",
            &[],
            &HashMap::new(),
            24,
            80,
            false,
        )
        .await
        .unwrap();

    // Wait for the process to exit and the daemon to handle it
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Session should have transitioned to stopped
    let info = client.get_session("exit-test").await.unwrap();
    assert_eq!(
        info.status,
        kild_protocol::SessionStatus::Stopped,
        "Session should be stopped after PTY exit"
    );

    client.shutdown().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_destroy_nonexistent_session_ok() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let socket_path = config.socket_path.clone();

    let server_handle = tokio::spawn(async move { kild_daemon::run_server(config).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut client = DaemonClient::connect(&socket_path).await.unwrap();

    // Destroy should succeed for non-existent session (idempotent)
    // or return an error — either way it should not crash the server
    let _result = client.destroy_session("nonexistent", false).await;

    // Server should still be responsive
    let sessions = client.list_sessions(None).await.unwrap();
    assert!(sessions.is_empty());

    client.shutdown().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_stop_session_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let socket_path = config.socket_path.clone();

    let server_handle = tokio::spawn(async move { kild_daemon::run_server(config).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut client = DaemonClient::connect(&socket_path).await.unwrap();

    // Create a session
    client
        .create_session(
            "idempotent-stop",
            "/tmp",
            "/bin/sh",
            &[],
            &HashMap::new(),
            24,
            80,
            false,
        )
        .await
        .unwrap();

    // First stop should succeed
    client.stop_session("idempotent-stop").await.unwrap();

    // Verify stopped
    let info = client.get_session("idempotent-stop").await.unwrap();
    assert_eq!(info.status, kild_protocol::SessionStatus::Stopped);

    // Second stop should also succeed (idempotent)
    client.stop_session("idempotent-stop").await.unwrap();

    // Still stopped
    let info = client.get_session("idempotent-stop").await.unwrap();
    assert_eq!(info.status, kild_protocol::SessionStatus::Stopped);

    client.shutdown().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_destroy_running_session() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let socket_path = config.socket_path.clone();

    let server_handle = tokio::spawn(async move { kild_daemon::run_server(config).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut client = DaemonClient::connect(&socket_path).await.unwrap();

    // Create a running session
    let session = client
        .create_session(
            "destroy-running",
            "/tmp",
            "/bin/sh",
            &[],
            &HashMap::new(),
            24,
            80,
            false,
        )
        .await
        .unwrap();
    assert_eq!(session.status, kild_protocol::SessionStatus::Running);

    // Destroy it while running (force=true)
    client
        .destroy_session("destroy-running", true)
        .await
        .unwrap();

    // Session should be gone
    let sessions = client.list_sessions(None).await.unwrap();
    assert!(
        sessions.is_empty(),
        "Destroyed session should not appear in list"
    );

    // Getting it should fail
    let result = client.get_session("destroy-running").await;
    assert!(result.is_err(), "Destroyed session should not be found");

    client.shutdown().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_session_with_login_shell() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let socket_path = config.socket_path.clone();

    let server_handle = tokio::spawn(async move { kild_daemon::run_server(config).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut client = DaemonClient::connect(&socket_path).await.unwrap();

    // Create session with login shell mode (bare shell)
    let working_dir = dir.path().to_string_lossy().to_string();
    let session = client
        .create_session(
            "shell-test",
            &working_dir,
            "", // Command is ignored in login shell mode
            &[],
            &HashMap::new(),
            24,
            80,
            true, // use_login_shell=true
        )
        .await
        .unwrap();

    assert_eq!(session.id, "shell-test");
    assert_eq!(session.status, kild_protocol::SessionStatus::Running);

    // Verify session is listed
    let sessions = client.list_sessions(None).await.unwrap();
    assert_eq!(sessions.len(), 1);

    // Cleanup
    client.stop_session("shell-test").await.unwrap();
    client.shutdown().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_destroy_then_recreate_same_session_id() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let socket_path = config.socket_path.clone();

    let server_handle = tokio::spawn(async move { kild_daemon::run_server(config).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut client = DaemonClient::connect(&socket_path).await.unwrap();

    // Create session
    client
        .create_session(
            "reopen-test",
            "/tmp",
            "/bin/sh",
            &[],
            &HashMap::new(),
            24,
            80,
            false,
        )
        .await
        .unwrap();

    // Destroy session (simulates what kild stop should do for daemon sessions)
    client.destroy_session("reopen-test", false).await.unwrap();

    // Re-create with same ID should succeed (was failing before #309 fix)
    let session = client
        .create_session(
            "reopen-test",
            "/tmp",
            "/bin/sh",
            &[],
            &HashMap::new(),
            24,
            80,
            false,
        )
        .await
        .unwrap();
    assert_eq!(session.id, "reopen-test");
    assert_eq!(session.status, kild_protocol::SessionStatus::Running);

    // Clean up
    client.destroy_session("reopen-test", false).await.unwrap();
    client.shutdown().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_invalid_json_does_not_crash_server() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let socket_path = config.socket_path.clone();

    let server_handle = tokio::spawn(async move { kild_daemon::run_server(config).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send raw garbage over a unix socket connection
    {
        use tokio::io::AsyncWriteExt;
        let mut raw_stream = tokio::net::UnixStream::connect(&socket_path).await.unwrap();
        raw_stream.write_all(b"this is not json\n").await.unwrap();
        raw_stream.flush().await.unwrap();
        // Drop the connection
    }

    // Give the server a moment to process the bad input
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Server should still be responsive to valid clients
    let mut client = DaemonClient::connect(&socket_path).await.unwrap();
    let sessions = client.list_sessions(None).await.unwrap();
    assert!(sessions.is_empty());

    client.shutdown().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(3), server_handle).await;
    assert!(result.is_ok());
}
