use std::collections::VecDeque;
use std::io::Read;
use std::sync::{Arc, RwLock};

use bytes::Bytes;
use tokio::sync::broadcast;
use tracing::{debug, error, warn};

/// Ring buffer for recent PTY output (scrollback replay on attach).
pub struct ScrollbackBuffer {
    buffer: VecDeque<u8>,
    capacity: usize,
}

impl ScrollbackBuffer {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "ScrollbackBuffer capacity must be non-zero");
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Append bytes to the ring buffer, evicting oldest data if full.
    ///
    /// If `data` alone exceeds capacity, clears buffer and keeps the last `capacity` bytes.
    /// Otherwise drains the minimum oldest bytes needed to fit the new data.
    pub fn push(&mut self, data: &[u8]) {
        if data.len() >= self.capacity {
            // Data alone fills or exceeds capacity — just keep the tail
            self.buffer.clear();
            let start = data.len() - self.capacity;
            self.buffer.extend(&data[start..]);
            return;
        }
        let needed = (self.buffer.len() + data.len()).saturating_sub(self.capacity);
        if needed > 0 {
            self.buffer.drain(..needed);
        }
        self.buffer.extend(data);
    }

    /// Get all buffered bytes as a contiguous slice.
    pub fn contents(&self) -> Vec<u8> {
        self.buffer.iter().copied().collect()
    }

    /// Current number of bytes in the buffer.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

/// Spawn a blocking task that reads from a PTY reader and feeds output
/// to the broadcaster.
///
/// Returns a `JoinHandle` for the reader task. The task exits when the PTY
/// reader returns EOF (child process exited) or on read error.
///
/// `on_exit` is called with the session_id when the reader loop ends.
/// Notification that a PTY reader has exited (child process ended or read error).
pub struct PtyExitEvent {
    pub session_id: String,
}

pub fn spawn_pty_reader(
    session_id: String,
    mut reader: Box<dyn Read + Send>,
    output_tx: broadcast::Sender<Bytes>,
    scrollback: Arc<RwLock<ScrollbackBuffer>>,
    exit_tx: Option<tokio::sync::mpsc::UnboundedSender<PtyExitEvent>>,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    debug!(event = "daemon.pty.reader_eof", session_id = session_id,);
                    break;
                }
                Ok(n) => {
                    // Feed scrollback buffer for replay on attach
                    match scrollback.write() {
                        Ok(mut sb) => sb.push(&buf[..n]),
                        Err(e) => {
                            error!(
                                event = "daemon.pty.scrollback_lock_poisoned",
                                session_id = session_id,
                                error = %e,
                                "RwLock poisoned, clearing scrollback to avoid corrupt data",
                            );
                            let mut sb = e.into_inner();
                            sb.clear();
                            sb.push(&buf[..n]);
                        }
                    }
                    // broadcast::send returns Err when there are no receivers,
                    // which is normal — nobody may be attached yet. The scrollback
                    // buffer already captured the data above for replay on attach.
                    let _ = output_tx.send(Bytes::copy_from_slice(&buf[..n]));
                }
                Err(e) => {
                    error!(
                        event = "daemon.pty.reader_error",
                        session_id = session_id,
                        error = %e,
                    );
                    break;
                }
            }
        }
        // Notify that the PTY reader has exited.
        // Send failure here means the receiver (daemon main loop) has been dropped,
        // which only happens during daemon shutdown. The error log is sufficient
        // since shutdown will clean up all sessions regardless.
        if let Some(tx) = exit_tx
            && tx
                .send(PtyExitEvent {
                    session_id: session_id.clone(),
                })
                .is_err()
        {
            warn!(
                event = "daemon.pty.exit_notification_failed",
                session_id = session_id,
                "PTY exit notification channel closed — daemon may not clean up session",
            );
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scrollback_buffer_basic() {
        let mut buf = ScrollbackBuffer::new(10);
        assert!(buf.is_empty());

        buf.push(b"hello");
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.contents(), b"hello");
    }

    #[test]
    fn test_scrollback_buffer_overflow() {
        let mut buf = ScrollbackBuffer::new(5);
        buf.push(b"hello world");
        // Only last 5 bytes should remain
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.contents(), b"world");
    }

    #[test]
    fn test_scrollback_buffer_exact_capacity() {
        let mut buf = ScrollbackBuffer::new(5);
        buf.push(b"12345");
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.contents(), b"12345");
    }

    #[test]
    fn test_scrollback_buffer_incremental_push() {
        let mut buf = ScrollbackBuffer::new(5);
        buf.push(b"abc");
        buf.push(b"def");
        // "abcdef" → only last 5 → "bcdef"
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.contents(), b"bcdef");
    }

    #[test]
    fn test_scrollback_buffer_clear() {
        let mut buf = ScrollbackBuffer::new(10);
        buf.push(b"test");
        buf.clear();
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_scrollback_buffer_single_byte_capacity() {
        let mut buf = ScrollbackBuffer::new(1);
        buf.push(b"abc");
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.contents(), b"c");
    }

    #[test]
    fn test_scrollback_buffer_push_larger_than_capacity() {
        let mut buf = ScrollbackBuffer::new(3);
        buf.push(b"abcdefghij");
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.contents(), b"hij");
    }

    #[test]
    fn test_scrollback_buffer_exactly_one_byte_over() {
        let mut buf = ScrollbackBuffer::new(5);
        buf.push(b"12345");
        assert_eq!(buf.len(), 5);
        // Add 1 byte — should drain exactly 1 oldest byte
        buf.push(b"6");
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.contents(), b"23456");
    }

    #[test]
    fn test_scrollback_buffer_drain_boundary() {
        let mut buf = ScrollbackBuffer::new(10);
        buf.push(b"12345"); // len=5
        buf.push(b"67890"); // len=10, at capacity
        buf.push(b"abc"); // should drain 3, remain 10
        assert_eq!(buf.len(), 10);
        assert_eq!(buf.contents(), b"4567890abc");
    }

    #[test]
    #[should_panic(expected = "ScrollbackBuffer capacity must be non-zero")]
    fn test_scrollback_buffer_zero_capacity_panics() {
        ScrollbackBuffer::new(0);
    }
}
