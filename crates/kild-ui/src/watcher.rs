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
        let mut found_relevant_event = false;

        loop {
            match self.receiver.try_recv() {
                Ok(Ok(event)) => {
                    if Self::is_relevant_event(&event) && !found_relevant_event {
                        tracing::debug!(
                            event = "ui.watcher.event_detected",
                            kind = ?event.kind,
                            paths = ?event.paths
                        );
                        found_relevant_event = true;
                    }
                    // Continue draining to prevent queue buildup
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        event = "ui.watcher.event_error",
                        error = %e
                    );
                    // Continue checking - errors are non-fatal
                }
                Err(TryRecvError::Empty) => {
                    return found_relevant_event;
                }
                Err(TryRecvError::Disconnected) => {
                    tracing::warn!(event = "ui.watcher.channel_disconnected");
                    return found_relevant_event;
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

        // Only care about .json (session files) and .status (agent status sidecar files)
        event.paths.iter().any(|p| {
            p.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "json" || ext == "status")
        })
    }
}

/// Watches the shim pane registry directory (`~/.kild/shim/`) for teammate changes.
///
/// Detects modifications to `panes.json` files to trigger teammate refresh.
pub struct ShimWatcher {
    _watcher: RecommendedWatcher,
    receiver: Receiver<Result<Event, notify::Error>>,
}

impl ShimWatcher {
    /// Create a new watcher for the shim directory.
    ///
    /// Returns `None` if the directory doesn't exist or the watcher can't be created.
    pub fn new() -> Option<Self> {
        let home = dirs::home_dir()?;

        let shim_dir = home.join(".kild").join("shim");
        if !shim_dir.exists() {
            tracing::debug!(
                event = "ui.shim_watcher.dir_missing",
                path = %shim_dir.display(),
                "Shim directory doesn't exist yet"
            );
            return None;
        }

        let (tx, rx) = mpsc::channel();

        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(
                    event = "ui.shim_watcher.create_failed",
                    error = %e,
                );
                return None;
            }
        };

        // Watch recursively — panes.json files are in subdirectories
        if let Err(e) = watcher.watch(&shim_dir, RecursiveMode::Recursive) {
            tracing::warn!(
                event = "ui.shim_watcher.watch_failed",
                path = %shim_dir.display(),
                error = %e,
            );
            return None;
        }

        tracing::info!(
            event = "ui.shim_watcher.started",
            path = %shim_dir.display()
        );

        Some(Self {
            _watcher: watcher,
            receiver: rx,
        })
    }

    /// Check for pending shim events and return session IDs that changed.
    ///
    /// Extracts the session ID from the path: `~/.kild/shim/<session_id>/panes.json`
    pub fn drain_changed_sessions(&self) -> Vec<String> {
        let mut changed = Vec::new();

        loop {
            match self.receiver.try_recv() {
                Ok(Ok(event)) => {
                    if !matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    ) {
                        continue;
                    }

                    for path in &event.paths {
                        if path
                            .file_name()
                            .and_then(|f| f.to_str())
                            .is_some_and(|f| f == "panes.json")
                            && let Some(session_id) = path
                                .parent()
                                .and_then(|p| p.file_name())
                                .and_then(|f| f.to_str())
                            && !changed.contains(&session_id.to_string())
                        {
                            changed.push(session_id.to_string());
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        event = "ui.shim_watcher.event_error",
                        error = %e,
                    );
                    continue;
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }

        if !changed.is_empty() {
            tracing::debug!(
                event = "ui.shim_watcher.sessions_changed",
                count = changed.len(),
                sessions = ?changed,
            );
        }

        changed
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

    // --- Unit tests for is_relevant_event ---

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

    #[test]
    fn test_is_relevant_event_with_multiple_paths_mixed() {
        // If ANY path is .json, should return true
        let event = make_event(
            EventKind::Create(CreateKind::File),
            vec![
                PathBuf::from("/sessions/test.txt"),
                PathBuf::from("/sessions/test.json"),
            ],
        );
        assert!(
            SessionWatcher::is_relevant_event(&event),
            "Should return true if ANY path is .json"
        );
    }

    #[test]
    fn test_is_relevant_event_status_sidecar() {
        let event = make_event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/sessions/test_branch.status")],
        );
        assert!(
            SessionWatcher::is_relevant_event(&event),
            "Should return true for .status sidecar files"
        );
    }

    #[test]
    fn test_is_relevant_event_with_empty_paths() {
        let event = make_event(EventKind::Create(CreateKind::File), vec![]);
        assert!(
            !SessionWatcher::is_relevant_event(&event),
            "Should return false for empty paths"
        );
    }

    // --- Integration tests for SessionWatcher::new ---

    #[test]
    fn test_session_watcher_new_returns_none_for_nonexistent_directory() {
        let nonexistent = PathBuf::from("/nonexistent/path/that/will/never/exist");
        let watcher = SessionWatcher::new(&nonexistent);
        assert!(
            watcher.is_none(),
            "Should return None when directory doesn't exist"
        );
    }

    #[test]
    fn test_session_watcher_new_succeeds_for_existing_directory() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let watcher = SessionWatcher::new(temp_dir.path());
        assert!(
            watcher.is_some(),
            "Should successfully create watcher for valid directory"
        );
    }

    // --- Integration tests for has_pending_events ---

    #[test]
    fn test_has_pending_events_returns_false_when_no_events() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let watcher = SessionWatcher::new(temp_dir.path()).unwrap();

        // No events yet
        assert!(
            !watcher.has_pending_events(),
            "Should return false with no events"
        );
    }

    #[test]
    fn test_has_pending_events_returns_true_after_json_file_creation() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let watcher = SessionWatcher::new(temp_dir.path()).unwrap();

        // Create a .json file (relevant event)
        let test_file = temp_dir.path().join("test.json");
        std::fs::File::create(&test_file).unwrap();

        // Give notify time to detect the change (file events are async)
        std::thread::sleep(std::time::Duration::from_millis(200));

        assert!(
            watcher.has_pending_events(),
            "Should detect .json file creation"
        );

        // Second call should return false (events drained)
        assert!(
            !watcher.has_pending_events(),
            "Should return false after draining events"
        );
    }

    #[test]
    fn test_has_pending_events_ignores_non_json_files() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let watcher = SessionWatcher::new(temp_dir.path()).unwrap();

        // Create a .txt file (irrelevant event)
        std::fs::File::create(temp_dir.path().join("test.txt")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));

        assert!(
            !watcher.has_pending_events(),
            "Should ignore non-.json files"
        );
    }

    #[test]
    fn test_has_pending_events_drains_multiple_events() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let watcher = SessionWatcher::new(temp_dir.path()).unwrap();

        // Create multiple .json files rapidly
        for i in 0..5 {
            std::fs::File::create(temp_dir.path().join(format!("test{}.json", i))).unwrap();
        }

        std::thread::sleep(std::time::Duration::from_millis(200));

        // First call should return true and drain ALL events
        assert!(watcher.has_pending_events());
        // Second call should see no pending events
        assert!(!watcher.has_pending_events());
    }
}
