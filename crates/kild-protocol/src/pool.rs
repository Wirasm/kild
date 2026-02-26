//! Thread-local IPC connection pool.
//!
//! Caches at most one [`IpcConnection`] per thread to avoid creating a new
//! socket connection for every operation. Critical for high-frequency callers
//! like keystroke forwarding in the tmux shim.
//!
//! Both `kild-core` and `kild-tmux-shim` delegate to this module instead of
//! maintaining their own connection caches.
//!
//! **Single-path invariant:** the pool does not track which socket path a
//! cached connection belongs to. Each process is expected to call [`take()`]
//! with the same `socket_path` for the lifetime of the thread. This holds
//! in practice — both callers resolve one daemon socket per process.

use std::cell::RefCell;
use std::path::Path;

use crate::{IpcConnection, IpcError};

thread_local! {
    static CACHED: RefCell<Option<IpcConnection>> = const { RefCell::new(None) };
}

/// Take a connection from the pool, or create a fresh one.
///
/// Three possible paths:
/// 1. A cached connection exists and is still alive — returns it (reused).
/// 2. A cached connection exists but is dead — evicts it and connects fresh.
/// 3. The pool is empty — connects to `socket_path`.
///
/// Returns `(connection, reused)` where `reused` is `true` for path 1 and
/// `false` for paths 2 and 3. Callers use this to emit their own tracing
/// events (the pool itself has no `tracing` dependency).
///
/// The returned connection has exclusive ownership — call [`release()`]
/// after successful use to make it available for the next caller.
pub fn take(socket_path: &Path) -> Result<(IpcConnection, bool), IpcError> {
    CACHED.with(|cell| {
        let mut cached = cell.borrow_mut();
        if let Some(conn) = cached.take()
            && conn.is_alive()
        {
            return Ok((conn, true));
        }
        let conn = IpcConnection::connect(socket_path)?;
        Ok((conn, false))
    })
}

/// Return a connection to the pool for reuse.
///
/// Re-validates liveness before caching. Broken connections are silently
/// dropped. Returns `true` if the connection was cached, `false` if it was
/// dropped due to a failed liveness check. Callers use this to emit their
/// own tracing events.
pub fn release(conn: IpcConnection) -> bool {
    if !conn.is_alive() {
        return false;
    }
    CACHED.with(|cell| {
        *cell.borrow_mut() = Some(conn);
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, Write};
    use std::os::unix::net::UnixListener;

    #[test]
    fn test_take_creates_fresh_connection() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let _listener = UnixListener::bind(&sock_path).unwrap();

        let (conn, reused) = take(&sock_path).unwrap();
        assert!(conn.is_alive());
        assert!(!reused, "First take should be a fresh connection");
    }

    #[test]
    fn test_take_returns_missing_socket_error() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("nonexistent.sock");

        let result = take(&sock_path);
        assert!(matches!(result.unwrap_err(), IpcError::NotRunning { .. }));
    }

    #[test]
    fn test_release_and_reuse() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        // Take a connection, use it, release it
        let (mut conn, reused) = take(&sock_path).unwrap();
        assert!(!reused);

        // Accept on server side and send a response so we can verify the connection works
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut reader = std::io::BufReader::new(stream);
            // Handle two requests (one per take)
            for _ in 0..2 {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                writeln!(writer, r#"{{"type":"ack","id":"1"}}"#).unwrap();
                writer.flush().unwrap();
            }
        });

        let request = crate::ClientMessage::Ping {
            id: "1".to_string(),
        };
        conn.send(&request).unwrap();
        assert!(release(conn), "Live connection should be cached");

        // Second take should reuse the cached connection (same socket, no new accept)
        let (mut conn2, reused2) = take(&sock_path).unwrap();
        assert!(reused2, "Second take should reuse cached connection");
        let response = conn2.send(&request).unwrap();
        assert!(matches!(response, crate::DaemonMessage::Ack { .. }));

        handle.join().unwrap();
    }

    #[test]
    fn test_release_drops_dead_connection() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let (conn, _) = take(&sock_path).unwrap();

        // Accept and immediately close server side
        let (server_stream, _) = listener.accept().unwrap();
        drop(server_stream);
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Connection is dead — release should drop it
        assert!(!release(conn), "Dead connection should not be cached");

        // Verify pool is empty
        CACHED.with(|cell| {
            assert!(
                cell.borrow().is_none(),
                "Dead connection should not be cached"
            );
        });
    }

    #[test]
    fn test_take_evicts_stale_cached_connection() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");

        // First: create a connection and release it while alive
        {
            let listener = UnixListener::bind(&sock_path).unwrap();
            let (conn, _) = take(&sock_path).unwrap();
            let (_server_stream, _) = listener.accept().unwrap();
            assert!(release(conn), "Should cache while alive");
            // listener and _server_stream drop here — peer closes
        }

        std::thread::sleep(std::time::Duration::from_millis(50));

        // Remove stale socket file so we can re-bind
        std::fs::remove_file(&sock_path).ok();
        let _listener2 = UnixListener::bind(&sock_path).unwrap();

        // take() should detect the stale cached connection, evict it, and reconnect
        let (conn, reused) = take(&sock_path).unwrap();
        assert!(!reused, "Stale connection should be evicted, not reused");
        assert!(conn.is_alive());
    }
}
