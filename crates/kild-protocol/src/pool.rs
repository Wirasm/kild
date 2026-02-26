//! Thread-local IPC connection pool.
//!
//! Caches at most one [`IpcConnection`] per thread to avoid creating a new
//! socket connection for every operation. Critical for high-frequency callers
//! like keystroke forwarding in the tmux shim.
//!
//! Both `kild-core` and `kild-tmux-shim` delegate to this module instead of
//! maintaining their own connection caches.

use std::cell::RefCell;
use std::path::Path;

use crate::{IpcConnection, IpcError};

thread_local! {
    static CACHED: RefCell<Option<IpcConnection>> = const { RefCell::new(None) };
}

/// Take a connection from the pool, or create a fresh one.
///
/// If a cached connection exists and is still alive, returns it.
/// Otherwise connects to `socket_path` and returns a new connection.
/// The returned connection has exclusive ownership — call [`release()`]
/// after successful use to make it available for the next caller.
pub fn take(socket_path: &Path) -> Result<IpcConnection, IpcError> {
    CACHED.with(|cell| {
        let mut cached = cell.borrow_mut();
        if let Some(conn) = cached.take()
            && conn.is_alive()
        {
            return Ok(conn);
        }
        IpcConnection::connect(socket_path)
    })
}

/// Return a connection to the pool for reuse.
///
/// Re-validates liveness before caching. Broken connections are silently dropped.
pub fn release(conn: IpcConnection) {
    if !conn.is_alive() {
        return;
    }
    CACHED.with(|cell| {
        *cell.borrow_mut() = Some(conn);
    });
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

        let conn = take(&sock_path).unwrap();
        assert!(conn.is_alive());
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
        let mut conn = take(&sock_path).unwrap();

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
        release(conn);

        // Second take should reuse the cached connection (same socket, no new accept)
        let mut conn2 = take(&sock_path).unwrap();
        let response = conn2.send(&request).unwrap();
        assert!(matches!(response, crate::DaemonMessage::Ack { .. }));

        handle.join().unwrap();
    }

    #[test]
    fn test_release_drops_dead_connection() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let conn = take(&sock_path).unwrap();

        // Accept and immediately close server side
        let (server_stream, _) = listener.accept().unwrap();
        drop(server_stream);
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Connection is dead — release should drop it
        release(conn);

        // Verify pool is empty (next take creates fresh)
        CACHED.with(|cell| {
            assert!(
                cell.borrow().is_none(),
                "Dead connection should not be cached"
            );
        });
    }
}
