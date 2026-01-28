//! File watcher for session changes.
//!
//! Watches the sessions directory for file system events (create, modify, remove)
//! to trigger immediate UI refresh when CLI operations occur.

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// Watches the sessions directory for changes.
///
/// Uses platform-native file watching (FSEvents on macOS, inotify on Linux)
/// for efficient event-driven updates instead of polling.
pub struct SessionWatcher {
    /// The underlying notify watcher. Must be kept alive.
    _watcher: RecommendedWatcher,
    /// Channel receiver for file events.
    receiver: Receiver<Result<Event, notify::Error>>,
}

impl SessionWatcher {
    /// Create a new watcher for the given sessions directory.
    ///
    /// Returns `None` if the watcher cannot be created (e.g., platform not supported,
    /// permissions issue, or directory doesn't exist yet).
    pub fn new(sessions_dir: &Path) -> Option<Self> {
        let (tx, rx) = mpsc::channel();

        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(
                    event = "ui.watcher.create_failed",
                    error = %e,
                    "File watcher unavailable - falling back to polling"
                );
                return None;
            }
        };

        // Watch directory non-recursively (sessions are flat .json files)
        if let Err(e) = watcher.watch(sessions_dir, RecursiveMode::NonRecursive) {
            tracing::warn!(
                event = "ui.watcher.watch_failed",
                path = %sessions_dir.display(),
                error = %e,
                "Cannot watch sessions directory - falling back to polling"
            );
            return None;
        }

        tracing::info!(
            event = "ui.watcher.started",
            path = %sessions_dir.display()
        );

        Some(Self {
            _watcher: watcher,
            receiver: rx,
        })
    }

    /// Check for pending file events (non-blocking).
    ///
    /// Returns `true` if any relevant events (create/modify/remove of .json files)
    /// were detected since the last call.
    pub fn has_pending_events(&self) -> bool {
        loop {
            match self.receiver.try_recv() {
                Ok(Ok(event)) => {
                    if Self::is_relevant_event(&event) {
                        tracing::debug!(
                            event = "ui.watcher.event_detected",
                            kind = ?event.kind,
                            paths = ?event.paths
                        );
                        // Drain remaining events and return true
                        while self.receiver.try_recv().is_ok() {}
                        return true;
                    }
                    // Not relevant, continue checking
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        event = "ui.watcher.event_error",
                        error = %e
                    );
                    // Continue checking - errors are non-fatal
                }
                Err(TryRecvError::Empty) => {
                    // No more events
                    return false;
                }
                Err(TryRecvError::Disconnected) => {
                    tracing::warn!(event = "ui.watcher.channel_disconnected");
                    return false;
                }
            }
        }
    }

    /// Check if an event is relevant (create/modify/remove of .json files).
    fn is_relevant_event(event: &Event) -> bool {
        // Only care about create, modify, remove events
        let is_relevant_kind = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        );

        if !is_relevant_kind {
            return false;
        }

        // Only care about .json files (session files)
        event.paths.iter().any(|p| {
            p.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "json")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, ModifyKind, RemoveKind};
    use std::path::PathBuf;

    fn make_event(kind: EventKind, paths: Vec<PathBuf>) -> Event {
        Event {
            kind,
            paths,
            attrs: Default::default(),
        }
    }

    #[test]
    fn test_is_relevant_event_create_json() {
        let event = make_event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/sessions/test.json")],
        );
        assert!(SessionWatcher::is_relevant_event(&event));
    }

    #[test]
    fn test_is_relevant_event_modify_json() {
        let event = make_event(
            EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content)),
            vec![PathBuf::from("/sessions/test.json")],
        );
        assert!(SessionWatcher::is_relevant_event(&event));
    }

    #[test]
    fn test_is_relevant_event_remove_json() {
        let event = make_event(
            EventKind::Remove(RemoveKind::File),
            vec![PathBuf::from("/sessions/test.json")],
        );
        assert!(SessionWatcher::is_relevant_event(&event));
    }

    #[test]
    fn test_is_relevant_event_ignores_non_json() {
        let event = make_event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/sessions/test.txt")],
        );
        assert!(!SessionWatcher::is_relevant_event(&event));
    }

    #[test]
    fn test_is_relevant_event_ignores_ds_store() {
        let event = make_event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/sessions/.DS_Store")],
        );
        assert!(!SessionWatcher::is_relevant_event(&event));
    }

    #[test]
    fn test_is_relevant_event_ignores_access_events() {
        let event = make_event(
            EventKind::Access(notify::event::AccessKind::Read),
            vec![PathBuf::from("/sessions/test.json")],
        );
        assert!(!SessionWatcher::is_relevant_event(&event));
    }
}
