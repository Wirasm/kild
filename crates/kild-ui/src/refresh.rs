//! Background refresh logic for status dashboard.
//!
//! Provides auto-refresh functionality with a hybrid approach:
//! - File watcher (notify) for instant updates when CLI modifies session files
//! - Slow poll fallback for edge cases like direct process termination

use std::time::Duration;

/// Fallback poll interval - file watcher handles most updates.
/// This catches process crashes, external changes, missed events.
pub const POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Debounce interval for file events to avoid rapid refreshes
/// when multiple files change at once (e.g., bulk operations).
pub const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(100);

/// Fast poll interval used when file watching is unavailable.
/// Falls back to previous behavior if watcher fails to initialize.
pub const FAST_POLL_INTERVAL: Duration = Duration::from_secs(5);
