use std::collections::VecDeque;
use std::io::Read;
use std::sync::{Arc, Mutex};

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
    pub fn push(&mut self, data: &[u8]) {
        for &byte in data {
            if self.buffer.len() >= self.capacity {
                self.buffer.pop_front();
            }
            self.buffer.push_back(byte);
        }
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

/// Holds the broadcast sender for PTY output distribution and the scrollback buffer.
pub struct PtyOutputBroadcaster {
    /// Broadcast channel sender for live output distribution.
    tx: broadcast::Sender<Vec<u8>>,
    /// Ring buffer for scrollback replay.
    scrollback: ScrollbackBuffer,
}

impl PtyOutputBroadcaster {
    pub fn new(scrollback_capacity: usize, broadcast_capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(broadcast_capacity);
        Self {
            tx,
            scrollback: ScrollbackBuffer::new(scrollback_capacity),
        }
    }

    /// Subscribe to receive live PTY output.
    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.tx.subscribe()
    }

    /// Get scrollback buffer contents for replay on attach.
    pub fn scrollback_contents(&self) -> Vec<u8> {
        self.scrollback.contents()
    }

    /// Feed bytes into the broadcaster: stores in scrollback and sends to subscribers.
    pub fn feed(&mut self, data: &[u8]) {
        self.scrollback.push(data);
        if self.tx.send(data.to_vec()).is_err() {
            debug!(
                event = "daemon.pty.broadcast_no_receivers",
                "No receivers attached, output buffered in scrollback only"
            );
        }
    }

    /// Number of currently subscribed receivers.
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
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
    output_tx: broadcast::Sender<Vec<u8>>,
    scrollback: Arc<Mutex<ScrollbackBuffer>>,
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
                    let data = buf[..n].to_vec();
                    // Feed scrollback buffer for replay on attach
                    match scrollback.lock() {
                        Ok(mut sb) => sb.push(&data),
                        Err(e) => {
                            error!(
                                event = "daemon.pty.scrollback_lock_poisoned",
                                session_id = session_id,
                                error = %e,
                                "Mutex poisoned, clearing scrollback to avoid corrupt data",
                            );
                            let mut sb = e.into_inner();
                            sb.clear();
                            sb.push(&data);
                        }
                    }
                    if output_tx.send(data).is_err() {
                        debug!(
                            event = "daemon.pty.reader_channel_closed",
                            session_id = session_id,
                            "Output channel closed, stopping reader",
                        );
                        break;
                    }
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
    fn test_broadcaster_basic() {
        let broadcaster = PtyOutputBroadcaster::new(1024, 16);
        assert_eq!(broadcaster.receiver_count(), 0);
        assert!(broadcaster.scrollback_contents().is_empty());
    }

    #[test]
    fn test_broadcaster_feed_and_scrollback() {
        let mut broadcaster = PtyOutputBroadcaster::new(1024, 16);
        broadcaster.feed(b"hello ");
        broadcaster.feed(b"world");
        assert_eq!(broadcaster.scrollback_contents(), b"hello world");
    }

    #[test]
    fn test_broadcaster_subscribe_and_receive() {
        let mut broadcaster = PtyOutputBroadcaster::new(1024, 16);
        let mut rx = broadcaster.subscribe();
        assert_eq!(broadcaster.receiver_count(), 1);

        broadcaster.feed(b"test data");

        let received = rx.try_recv().unwrap();
        assert_eq!(received, b"test data");
    }

    #[test]
    fn test_broadcaster_multiple_subscribers() {
        let mut broadcaster = PtyOutputBroadcaster::new(1024, 16);
        let mut rx1 = broadcaster.subscribe();
        let mut rx2 = broadcaster.subscribe();
        assert_eq!(broadcaster.receiver_count(), 2);

        broadcaster.feed(b"shared data");

        assert_eq!(rx1.try_recv().unwrap(), b"shared data");
        assert_eq!(rx2.try_recv().unwrap(), b"shared data");
    }

    #[test]
    fn test_broadcaster_no_receivers_ok() {
        let mut broadcaster = PtyOutputBroadcaster::new(1024, 16);
        // Feed with no receivers should not panic
        broadcaster.feed(b"no one listening");
        assert_eq!(broadcaster.scrollback_contents(), b"no one listening");
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
    #[should_panic(expected = "ScrollbackBuffer capacity must be non-zero")]
    fn test_scrollback_buffer_zero_capacity_panics() {
        ScrollbackBuffer::new(0);
    }
}
