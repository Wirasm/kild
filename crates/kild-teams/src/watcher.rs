//! File watcher for team config and shim pane registry changes.
//!
//! Follows the `SessionWatcher` pattern from `kild-ui/src/watcher.rs`.

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// Watches team-related directories for changes.
///
/// Monitors both `~/.claude/teams/` (team configs) and `~/.kild/shim/`
/// (pane registries) for file system events.
pub struct TeamWatcher {
    /// Underlying notify watchers. Must be kept alive.
    _watchers: Vec<RecommendedWatcher>,
    /// Channel receiver for file events.
    receiver: Receiver<Result<Event, notify::Error>>,
}

impl TeamWatcher {
    /// Create a new team watcher.
    ///
    /// Watches `~/.claude/teams/` recursively (config.json is nested in subdirs)
    /// and `~/.kild/shim/` recursively (pane registries are nested).
    ///
    /// Returns `None` if no directories can be watched.
    pub fn new(teams_dir: Option<&Path>, shim_dir: Option<&Path>) -> Option<Self> {
        let (tx, rx) = mpsc::channel();
        let mut watchers = Vec::new();

        if let Some(dir) = teams_dir
            && dir.is_dir()
        {
            match Self::create_watcher(dir, RecursiveMode::Recursive, tx.clone()) {
                Some(w) => {
                    tracing::info!(
                        event = "teams.watcher.watching_teams",
                        path = %dir.display()
                    );
                    watchers.push(w);
                }
                None => {
                    tracing::warn!(
                        event = "teams.watcher.teams_watch_failed",
                        path = %dir.display()
                    );
                }
            }
        }

        if let Some(dir) = shim_dir
            && dir.is_dir()
        {
            match Self::create_watcher(dir, RecursiveMode::Recursive, tx.clone()) {
                Some(w) => {
                    tracing::info!(
                        event = "teams.watcher.watching_shim",
                        path = %dir.display()
                    );
                    watchers.push(w);
                }
                None => {
                    tracing::warn!(
                        event = "teams.watcher.shim_watch_failed",
                        path = %dir.display()
                    );
                }
            }
        }

        if watchers.is_empty() {
            tracing::debug!(
                event = "teams.watcher.no_dirs_available",
                "No directories available to watch"
            );
            return None;
        }

        Some(Self {
            _watchers: watchers,
            receiver: rx,
        })
    }

    /// Create a watcher for default directories.
    ///
    /// Resolves `~/.claude/teams/` and `~/.kild/shim/` via `dirs::home_dir()`.
    pub fn new_default() -> Option<Self> {
        let home = dirs::home_dir()?;
        let teams_dir = home.join(".claude").join("teams");
        let shim_dir = home.join(".kild").join("shim");

        Self::new(Some(&teams_dir), Some(&shim_dir))
    }

    /// Check for pending file events (non-blocking).
    ///
    /// Returns `true` if any relevant events (config.json or panes.json changes)
    /// were detected since the last call. Drains all pending events.
    pub fn has_pending_events(&self) -> bool {
        let mut found_relevant = false;

        loop {
            match self.receiver.try_recv() {
                Ok(Ok(event)) => {
                    if Self::is_relevant_event(&event) && !found_relevant {
                        tracing::debug!(
                            event = "teams.watcher.event_detected",
                            kind = ?event.kind,
                            paths = ?event.paths
                        );
                        found_relevant = true;
                    }
                    // Continue draining
                }
                Ok(Err(e)) => {
                    tracing::warn!(event = "teams.watcher.event_error", error = %e);
                }
                Err(TryRecvError::Empty) => return found_relevant,
                Err(TryRecvError::Disconnected) => {
                    tracing::warn!(event = "teams.watcher.channel_disconnected");
                    return found_relevant;
                }
            }
        }
    }

    fn create_watcher(
        dir: &Path,
        mode: RecursiveMode,
        tx: mpsc::Sender<Result<Event, notify::Error>>,
    ) -> Option<RecommendedWatcher> {
        let mut watcher = notify::recommended_watcher(tx).ok()?;
        watcher.watch(dir, mode).ok()?;
        Some(watcher)
    }

    /// Check if an event is relevant (config.json or panes.json changes).
    fn is_relevant_event(event: &Event) -> bool {
        let is_relevant_kind = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        );

        if !is_relevant_kind {
            return false;
        }

        event.paths.iter().any(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| name == "config.json" || name == "panes.json")
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
    fn test_relevant_event_config_json() {
        let event = make_event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/teams/my-team/config.json")],
        );
        assert!(TeamWatcher::is_relevant_event(&event));
    }

    #[test]
    fn test_relevant_event_panes_json() {
        let event = make_event(
            EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content)),
            vec![PathBuf::from("/shim/session/panes.json")],
        );
        assert!(TeamWatcher::is_relevant_event(&event));
    }

    #[test]
    fn test_ignores_other_json() {
        let event = make_event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/teams/my-team/inboxes/agent.json")],
        );
        assert!(!TeamWatcher::is_relevant_event(&event));
    }

    #[test]
    fn test_ignores_non_json() {
        let event = make_event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/teams/my-team/something.txt")],
        );
        assert!(!TeamWatcher::is_relevant_event(&event));
    }

    #[test]
    fn test_relevant_event_remove() {
        let event = make_event(
            EventKind::Remove(RemoveKind::File),
            vec![PathBuf::from("/teams/old-team/config.json")],
        );
        assert!(TeamWatcher::is_relevant_event(&event));
    }

    #[test]
    fn test_ignores_access_events() {
        let event = make_event(
            EventKind::Access(notify::event::AccessKind::Read),
            vec![PathBuf::from("/teams/team/config.json")],
        );
        assert!(!TeamWatcher::is_relevant_event(&event));
    }

    #[test]
    fn test_new_with_missing_dirs() {
        // Both dirs don't exist — should return None
        let watcher = TeamWatcher::new(
            Some(Path::new("/nonexistent/teams")),
            Some(Path::new("/nonexistent/shim")),
        );
        assert!(watcher.is_none());
    }

    #[test]
    fn test_new_with_existing_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let watcher = TeamWatcher::new(Some(dir.path()), None);
        assert!(watcher.is_some());
    }

    #[test]
    fn test_has_pending_events_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let watcher = TeamWatcher::new(Some(dir.path()), None).unwrap();
        assert!(!watcher.has_pending_events());
    }

    #[test]
    fn test_has_pending_events_detects_config_change() {
        let dir = tempfile::TempDir::new().unwrap();
        let team_dir = dir.path().join("my-team");
        std::fs::create_dir_all(&team_dir).unwrap();

        let watcher = TeamWatcher::new(Some(dir.path()), None).unwrap();

        // Create config.json
        std::fs::write(team_dir.join("config.json"), r#"{"members":[]}"#).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));

        assert!(watcher.has_pending_events());
        // Drained
        assert!(!watcher.has_pending_events());
    }
}
