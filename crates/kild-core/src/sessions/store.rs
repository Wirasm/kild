//! Session file utilities.
//!
//! Lightweight functions for querying session files on disk
//! without full deserialization.

use std::path::Path;

/// Count session files on disk without fully loading them.
///
/// Lightweight check (directory traversal only, no file parsing) used to
/// detect when sessions have been added or removed externally.
///
/// Returns `None` if the directory cannot be read.
pub fn count_session_files() -> Option<usize> {
    let config = kild_config::Config::new();
    count_session_files_in_dir(&config.sessions_dir())
}

/// Count `.json` session files in a directory.
///
/// Extracted for testability — allows unit tests to provide a temp directory
/// instead of relying on the actual sessions directory.
pub fn count_session_files_in_dir(sessions_dir: &Path) -> Option<usize> {
    if !sessions_dir.exists() {
        return Some(0);
    }

    match std::fs::read_dir(sessions_dir) {
        Ok(entries) => {
            let count = entries
                .filter_map(|e| e.ok())
                .filter(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("json"))
                .count();
            Some(count)
        }
        Err(e) => {
            tracing::warn!(
                event = "core.session.count_files_failed",
                path = %sessions_dir.display(),
                error = %e
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_count_session_files_in_dir_empty() {
        let dir = TempDir::new().unwrap();
        assert_eq!(count_session_files_in_dir(dir.path()), Some(0));
    }

    #[test]
    fn test_count_session_files_in_dir_nonexistent() {
        let path = Path::new("/nonexistent/path/that/does/not/exist");
        assert_eq!(count_session_files_in_dir(path), Some(0));
    }

    #[test]
    fn test_count_session_files_in_dir_counts_only_json() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("session1.json"), "{}").unwrap();
        std::fs::write(dir.path().join("session2.json"), "{}").unwrap();
        std::fs::write(dir.path().join("readme.txt"), "text").unwrap();
        std::fs::write(dir.path().join("data.toml"), "toml").unwrap();

        assert_eq!(count_session_files_in_dir(dir.path()), Some(2));
    }

    #[test]
    fn test_count_session_files_in_dir_with_mixed_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.json"), "{}").unwrap();
        std::fs::write(dir.path().join("b.json"), "{}").unwrap();
        std::fs::write(dir.path().join("c.json"), "{}").unwrap();
        std::fs::write(dir.path().join("not-json.txt"), "text").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();

        assert_eq!(count_session_files_in_dir(dir.path()), Some(3));
    }
}
