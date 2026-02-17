//! # Lock Discipline
//!
//! The pane registry uses file-based locking to prevent races:
//!
//! - **Reads**: Use `load_shared()` to acquire a shared (read-only) lock.
//!   Multiple readers can hold shared locks concurrently.
//! - **Writes**: Use `load()` to acquire an exclusive lock, mutate the registry,
//!   then call `save()` (which re-acquires the exclusive lock).
//!
//! Locks are automatically released when the `Flock` handle is dropped (RAII).

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use kild_paths::KildPaths;
use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};

use crate::errors::ShimError;

enum LockMode {
    Shared,
    Exclusive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneRegistry {
    pub next_pane_id: u32,
    pub session_name: String,
    pub panes: HashMap<String, PaneEntry>,
    pub windows: HashMap<String, WindowEntry>,
    pub sessions: HashMap<String, SessionEntry>,
}

impl PaneRegistry {
    /// Validate referential integrity of the registry.
    ///
    /// Checks that all pane `window_id` references exist in `windows`,
    /// and all window `pane_ids` references exist in `panes`.
    pub fn validate(&self) -> Result<(), ShimError> {
        for (pane_id, pane) in &self.panes {
            if !self.windows.contains_key(&pane.window_id) {
                return Err(ShimError::state(format!(
                    "corrupt registry for session '{}': pane {} references non-existent window {}",
                    self.session_name, pane_id, pane.window_id
                )));
            }
        }
        for (window_id, window) in &self.windows {
            for pane_id in &window.pane_ids {
                if !self.panes.contains_key(pane_id) {
                    return Err(ShimError::state(format!(
                        "corrupt registry for session '{}': window {} references non-existent pane {}",
                        self.session_name, window_id, pane_id
                    )));
                }
            }
        }
        Ok(())
    }

    /// Remove a pane and clean up its window reference.
    pub fn remove_pane(&mut self, pane_id: &str) -> Option<PaneEntry> {
        if let Some(pane) = self.panes.remove(pane_id) {
            if let Some(window) = self.windows.get_mut(&pane.window_id) {
                window.pane_ids.retain(|id| id != pane_id);
            }
            Some(pane)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneEntry {
    pub daemon_session_id: String,
    pub title: String,
    pub border_style: String,
    pub window_id: String,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowEntry {
    pub name: String,
    pub pane_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    pub name: String,
    pub windows: Vec<String>,
}

pub fn state_dir(session_id: &str) -> Result<PathBuf, ShimError> {
    let paths = KildPaths::resolve().map_err(|e| ShimError::state(e.to_string()))?;
    Ok(paths.shim_session_dir(session_id))
}

fn lock_path(session_id: &str) -> Result<PathBuf, ShimError> {
    let paths = KildPaths::resolve().map_err(|e| ShimError::state(e.to_string()))?;
    Ok(paths.shim_lock_file(session_id))
}

fn panes_path(session_id: &str) -> Result<PathBuf, ShimError> {
    let paths = KildPaths::resolve().map_err(|e| ShimError::state(e.to_string()))?;
    Ok(paths.shim_panes_file(session_id))
}

/// Acquire a file lock for the pane registry.
///
/// Uses flock to prevent race conditions when multiple tmux shim commands
/// run concurrently (common with agent teams). Lock is automatically
/// released when the returned Flock handle is dropped.
fn acquire_lock(session_id: &str, mode: LockMode) -> Result<Flock<fs::File>, ShimError> {
    let lock = lock_path(session_id)?;
    if let Some(parent) = lock.parent() {
        fs::create_dir_all(parent).map_err(|e| ShimError::StateError {
            message: format!(
                "failed to create state directory {}: {}",
                parent.display(),
                e
            ),
        })?;
    }
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock)
        .map_err(|e| ShimError::StateError {
            message: format!(
                "failed to open lock file {} for session {}: {}",
                lock.display(),
                session_id,
                e
            ),
        })?;

    let (arg, lock_type) = match mode {
        LockMode::Shared => (FlockArg::LockShared, "shared"),
        LockMode::Exclusive => (FlockArg::LockExclusive, "exclusive"),
    };
    Flock::lock(lock_file, arg).map_err(|(_, e)| ShimError::StateError {
        message: format!(
            "failed to acquire {} lock for session {}: {}",
            lock_type, session_id, e
        ),
    })
}

/// Load the registry with a shared (read-only) lock.
/// Multiple readers can hold shared locks concurrently.
pub fn load_shared(session_id: &str) -> Result<PaneRegistry, ShimError> {
    let data_path = panes_path(session_id)?;
    let _lock = acquire_lock(session_id, LockMode::Shared)?;

    let content = fs::read_to_string(&data_path).map_err(|e| ShimError::StateError {
        message: format!("failed to read {}: {}", data_path.display(), e),
    })?;

    let registry: PaneRegistry =
        serde_json::from_str(&content).map_err(|e| ShimError::StateError {
            message: format!("failed to parse pane registry: {}", e),
        })?;

    registry.validate()?;

    Ok(registry)
}

/// Load the registry with an exclusive (write) lock.
pub fn load(session_id: &str) -> Result<PaneRegistry, ShimError> {
    let data_path = panes_path(session_id)?;
    let _lock = acquire_lock(session_id, LockMode::Exclusive)?;

    let content = fs::read_to_string(&data_path).map_err(|e| ShimError::StateError {
        message: format!("failed to read {}: {}", data_path.display(), e),
    })?;

    let registry: PaneRegistry =
        serde_json::from_str(&content).map_err(|e| ShimError::StateError {
            message: format!("failed to parse pane registry: {}", e),
        })?;

    registry.validate()?;

    Ok(registry)
}

pub fn save(session_id: &str, registry: &PaneRegistry) -> Result<(), ShimError> {
    let data_path = panes_path(session_id)?;
    let _lock = acquire_lock(session_id, LockMode::Exclusive)?;

    let content = serde_json::to_string_pretty(registry).map_err(|e| ShimError::StateError {
        message: format!("failed to serialize pane registry: {}", e),
    })?;

    let mut file = fs::File::create(&data_path).map_err(|e| ShimError::StateError {
        message: format!("failed to write {}: {}", data_path.display(), e),
    })?;
    file.write_all(content.as_bytes())?;
    file.flush()?;

    Ok(())
}

pub fn allocate_pane_id(registry: &mut PaneRegistry) -> String {
    let id = format!("%{}", registry.next_pane_id);
    registry.next_pane_id += 1;
    id
}

#[allow(dead_code)]
pub fn init_registry(session_id: &str, daemon_session_id: &str) -> Result<(), ShimError> {
    let dir = state_dir(session_id)?;
    fs::create_dir_all(&dir)?;

    let lock = lock_path(session_id)?;
    fs::File::create(&lock)?;

    let mut panes = HashMap::new();
    panes.insert(
        "%0".to_string(),
        PaneEntry {
            daemon_session_id: daemon_session_id.to_string(),
            title: String::new(),
            border_style: String::new(),
            window_id: "0".to_string(),
            hidden: false,
        },
    );

    let mut windows = HashMap::new();
    windows.insert(
        "0".to_string(),
        WindowEntry {
            name: "main".to_string(),
            pane_ids: vec!["%0".to_string()],
        },
    );

    let mut sessions = HashMap::new();
    sessions.insert(
        "kild_0".to_string(),
        SessionEntry {
            name: "kild_0".to_string(),
            windows: vec!["0".to_string()],
        },
    );

    let registry = PaneRegistry {
        next_pane_id: 1,
        session_name: "kild_0".to_string(),
        panes,
        windows,
        sessions,
    };

    save(session_id, &registry)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_allocate_pane_id() {
        let mut registry = PaneRegistry {
            next_pane_id: 3,
            session_name: "test".to_string(),
            panes: HashMap::new(),
            windows: HashMap::new(),
            sessions: HashMap::new(),
        };

        assert_eq!(allocate_pane_id(&mut registry), "%3");
        assert_eq!(registry.next_pane_id, 4);
        assert_eq!(allocate_pane_id(&mut registry), "%4");
        assert_eq!(registry.next_pane_id, 5);
    }

    #[test]
    fn test_init_and_load_registry() {
        let test_id = format!("test-{}", uuid::Uuid::new_v4());
        let dir = state_dir(&test_id).unwrap();

        init_registry(&test_id, "daemon-abc-123").unwrap();

        let registry = load(&test_id).unwrap();
        assert_eq!(registry.next_pane_id, 1);
        assert_eq!(registry.session_name, "kild_0");
        assert_eq!(registry.panes.len(), 1);
        assert_eq!(registry.panes["%0"].daemon_session_id, "daemon-abc-123");
        assert_eq!(registry.windows.len(), 1);
        assert_eq!(registry.sessions.len(), 1);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let test_id = format!("test-{}", uuid::Uuid::new_v4());
        let dir = state_dir(&test_id).unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::File::create(lock_path(&test_id).unwrap()).unwrap();

        let mut panes = HashMap::new();
        panes.insert(
            "%0".to_string(),
            PaneEntry {
                daemon_session_id: "d-1".to_string(),
                title: "main".to_string(),
                border_style: String::new(),
                window_id: "0".to_string(),
                hidden: false,
            },
        );
        panes.insert(
            "%1".to_string(),
            PaneEntry {
                daemon_session_id: "d-2".to_string(),
                title: "worker".to_string(),
                border_style: "fg=blue".to_string(),
                window_id: "0".to_string(),
                hidden: false,
            },
        );

        let mut windows = HashMap::new();
        windows.insert(
            "0".to_string(),
            WindowEntry {
                name: "main".to_string(),
                pane_ids: vec!["%0".to_string(), "%1".to_string()],
            },
        );

        let registry = PaneRegistry {
            next_pane_id: 2,
            session_name: "kild_0".to_string(),
            panes,
            windows,
            sessions: HashMap::new(),
        };

        save(&test_id, &registry).unwrap();
        let loaded = load(&test_id).unwrap();

        assert_eq!(loaded.next_pane_id, 2);
        assert_eq!(loaded.panes.len(), 2);
        assert_eq!(loaded.panes["%1"].title, "worker");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_state_dir_path() {
        let dir = state_dir("my-session").unwrap();
        assert!(dir.ends_with(".kild/shim/my-session"));
    }

    #[test]
    fn test_load_invalid_json() {
        let test_id = format!("test-{}", uuid::Uuid::new_v4());
        let dir = state_dir(&test_id).unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("panes.json"), "not valid json{{{").unwrap();

        let result = load(&test_id);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("failed to parse pane registry"),
            "got: {}",
            err
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_missing_panes_file() {
        let test_id = format!("test-{}", uuid::Uuid::new_v4());
        let dir = state_dir(&test_id).unwrap();
        fs::create_dir_all(&dir).unwrap();

        let result = load(&test_id);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("failed to read"), "got: {}", err);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_empty_json_file() {
        let test_id = format!("test-{}", uuid::Uuid::new_v4());
        let dir = state_dir(&test_id).unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("panes.json"), "").unwrap();

        let result = load(&test_id);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("failed to parse pane registry"),
            "got: {}",
            err
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_partial_json() {
        let test_id = format!("test-{}", uuid::Uuid::new_v4());
        let dir = state_dir(&test_id).unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("panes.json"), r#"{"next_pane_id": 1}"#).unwrap();

        let result = load(&test_id);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("failed to parse pane registry"),
            "got: {}",
            err
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_save_and_load_without_pre_created_lock() {
        let test_id = format!("test-{}", uuid::Uuid::new_v4());
        let dir = state_dir(&test_id).unwrap();

        let registry = PaneRegistry {
            next_pane_id: 1,
            session_name: "kild_0".to_string(),
            panes: HashMap::new(),
            windows: HashMap::new(),
            sessions: HashMap::new(),
        };

        save(&test_id, &registry).unwrap();

        let loaded = load(&test_id).unwrap();
        assert_eq!(loaded.next_pane_id, 1);
        assert_eq!(loaded.session_name, "kild_0");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_validate_valid_registry() {
        let mut panes = HashMap::new();
        panes.insert(
            "%0".to_string(),
            PaneEntry {
                daemon_session_id: "d-1".to_string(),
                title: String::new(),
                border_style: String::new(),
                window_id: "0".to_string(),
                hidden: false,
            },
        );

        let mut windows = HashMap::new();
        windows.insert(
            "0".to_string(),
            WindowEntry {
                name: "main".to_string(),
                pane_ids: vec!["%0".to_string()],
            },
        );

        let registry = PaneRegistry {
            next_pane_id: 1,
            session_name: "kild_0".to_string(),
            panes,
            windows,
            sessions: HashMap::new(),
        };

        assert!(registry.validate().is_ok());
    }

    #[test]
    fn test_validate_dangling_pane_window_ref() {
        let mut panes = HashMap::new();
        panes.insert(
            "%0".to_string(),
            PaneEntry {
                daemon_session_id: "d-1".to_string(),
                title: String::new(),
                border_style: String::new(),
                window_id: "999".to_string(),
                hidden: false,
            },
        );

        let registry = PaneRegistry {
            next_pane_id: 1,
            session_name: "kild_0".to_string(),
            panes,
            windows: HashMap::new(),
            sessions: HashMap::new(),
        };

        let err = registry.validate().unwrap_err();
        assert!(err.to_string().contains("non-existent window"));
    }

    #[test]
    fn test_validate_dangling_window_pane_ref() {
        let mut windows = HashMap::new();
        windows.insert(
            "0".to_string(),
            WindowEntry {
                name: "main".to_string(),
                pane_ids: vec!["%99".to_string()],
            },
        );

        let registry = PaneRegistry {
            next_pane_id: 1,
            session_name: "kild_0".to_string(),
            panes: HashMap::new(),
            windows,
            sessions: HashMap::new(),
        };

        let err = registry.validate().unwrap_err();
        assert!(err.to_string().contains("non-existent pane"));
    }

    #[test]
    fn test_remove_pane() {
        let mut panes = HashMap::new();
        panes.insert(
            "%0".to_string(),
            PaneEntry {
                daemon_session_id: "d-1".to_string(),
                title: String::new(),
                border_style: String::new(),
                window_id: "0".to_string(),
                hidden: false,
            },
        );
        panes.insert(
            "%1".to_string(),
            PaneEntry {
                daemon_session_id: "d-2".to_string(),
                title: String::new(),
                border_style: String::new(),
                window_id: "0".to_string(),
                hidden: false,
            },
        );

        let mut windows = HashMap::new();
        windows.insert(
            "0".to_string(),
            WindowEntry {
                name: "main".to_string(),
                pane_ids: vec!["%0".to_string(), "%1".to_string()],
            },
        );

        let mut registry = PaneRegistry {
            next_pane_id: 2,
            session_name: "kild_0".to_string(),
            panes,
            windows,
            sessions: HashMap::new(),
        };

        let removed = registry.remove_pane("%1");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().daemon_session_id, "d-2");
        assert!(!registry.panes.contains_key("%1"));
        assert_eq!(registry.windows["0"].pane_ids, vec!["%0".to_string()]);
    }

    #[test]
    fn test_remove_pane_not_found() {
        let mut registry = PaneRegistry {
            next_pane_id: 1,
            session_name: "kild_0".to_string(),
            panes: HashMap::new(),
            windows: HashMap::new(),
            sessions: HashMap::new(),
        };

        assert!(registry.remove_pane("%99").is_none());
    }

    #[test]
    fn test_load_shared_concurrent_reads() {
        use std::sync::Arc;
        use std::thread;

        let test_id = format!("test-{}", uuid::Uuid::new_v4());
        init_registry(&test_id, "daemon-abc-123").unwrap();

        let test_id = Arc::new(test_id);
        let mut handles = vec![];

        // Spawn 5 threads to simulate concurrent agent panes
        for _ in 0..5 {
            let id = Arc::clone(&test_id);
            handles.push(thread::spawn(move || load_shared(&id).unwrap()));
        }

        // All threads should complete without blocking each other
        for handle in handles {
            let registry = handle.join().unwrap();
            assert_eq!(registry.panes.len(), 1);
        }

        let dir = state_dir(&test_id).unwrap();
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_exclusive_lock_blocks_shared_reads() {
        use std::sync::{Arc, Barrier};
        use std::thread;
        use std::time::Duration;

        let test_id = format!("test-{}", uuid::Uuid::new_v4());
        init_registry(&test_id, "daemon-abc-123").unwrap();

        let test_id = Arc::new(test_id);
        let barrier = Arc::new(Barrier::new(2));

        // Thread 1: Hold exclusive lock for 100ms
        let id1 = Arc::clone(&test_id);
        let b1 = Arc::clone(&barrier);
        let writer = thread::spawn(move || {
            let _lock = acquire_lock(&id1, LockMode::Exclusive).unwrap();
            b1.wait(); // Signal that lock is held
            thread::sleep(Duration::from_millis(100));
            // Lock released on drop
        });

        // Thread 2: Wait for writer to hold lock, then try shared read
        let id2 = Arc::clone(&test_id);
        let b2 = Arc::clone(&barrier);
        let reader = thread::spawn(move || {
            b2.wait(); // Wait for writer to acquire lock
            let start = std::time::Instant::now();
            let _registry = load_shared(&id2).unwrap();
            start.elapsed()
        });

        writer.join().unwrap();
        let elapsed = reader.join().unwrap();

        // Reader must have waited for writer to release exclusive lock
        assert!(
            elapsed >= Duration::from_millis(80),
            "Shared read should have blocked on exclusive lock, but completed in {:?}",
            elapsed
        );

        let dir = state_dir(&test_id).unwrap();
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_shared_returns_valid_registry() {
        let test_id = format!("test-{}", uuid::Uuid::new_v4());
        init_registry(&test_id, "daemon-abc-123").unwrap();

        let registry = load_shared(&test_id).unwrap();
        assert_eq!(registry.next_pane_id, 1);
        assert_eq!(registry.panes.len(), 1);
        assert_eq!(registry.panes["%0"].daemon_session_id, "daemon-abc-123");

        let dir = state_dir(&test_id).unwrap();
        fs::remove_dir_all(&dir).ok();
    }
}
