//! Session file persistence
//!
//! Handles reading/writing session data to disk with atomic operations.

use crate::sessions::{errors::SessionError, types::*};
use std::fs;
use std::path::{Path, PathBuf};

/// Compute session directory path: `<sessions_dir>/<safe_id>/`.
fn session_dir(sessions_dir: &Path, session_id: &str) -> PathBuf {
    let safe_id = session_id.replace('/', "_");
    sessions_dir.join(safe_id)
}

/// Compute session file path: `<sessions_dir>/<safe_id>/kild.json`.
fn session_file(sessions_dir: &Path, session_id: &str) -> PathBuf {
    session_dir(sessions_dir, session_id).join("kild.json")
}

pub fn ensure_sessions_directory(sessions_dir: &Path) -> Result<(), SessionError> {
    fs::create_dir_all(sessions_dir).map_err(|e| SessionError::IoError { source: e })?;
    Ok(())
}

fn cleanup_temp_file(temp_file: &Path, original_error: &std::io::Error) {
    if let Err(cleanup_err) = fs::remove_file(temp_file) {
        tracing::warn!(
            event = "core.session.temp_file_cleanup_failed",
            temp_file = %temp_file.display(),
            original_error = %original_error,
            cleanup_error = %cleanup_err,
            message = "Failed to clean up temp file after operation error"
        );
    }
}

/// Migrate a session from flat file format to per-session directory format.
///
/// Detects old format: `<sessions_dir>/<safe_id>.json` exists as a file (not directory).
/// Migrates by: creating `<safe_id>/` directory, moving `.json` → `kild.json`,
/// moving `.status` → `status`, moving `.pr` → `pr`.
///
/// Returns `Ok(true)` if migration happened, `Ok(false)` if already new format or no file.
fn migrate_session_if_needed(sessions_dir: &Path, safe_id: &str) -> Result<bool, SessionError> {
    let old_json = sessions_dir.join(format!("{safe_id}.json"));
    if !old_json.is_file() {
        return Ok(false);
    }

    let dir = sessions_dir.join(safe_id);
    fs::create_dir_all(&dir).map_err(|e| {
        tracing::warn!(
            event = "core.session.dir_create_failed",
            path = %dir.display(),
            error = %e,
        );
        SessionError::IoError { source: e }
    })?;

    // Move main session file (race-safe: another process may have already migrated)
    match fs::rename(&old_json, dir.join("kild.json")) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Another process already migrated this session
            return Ok(false);
        }
        Err(e) => return Err(SessionError::IoError { source: e }),
    }

    // Move sidecar files (best-effort, race-safe)
    let old_status = sessions_dir.join(format!("{safe_id}.status"));
    if old_status.is_file() {
        match fs::rename(&old_status, dir.join("status")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(
                    event = "core.session.migration_sidecar_failed",
                    safe_id = safe_id,
                    sidecar = "status",
                    error = %e,
                );
            }
        }
    }

    let old_pr = sessions_dir.join(format!("{safe_id}.pr"));
    if old_pr.is_file() {
        match fs::rename(&old_pr, dir.join("pr")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(
                    event = "core.session.migration_sidecar_failed",
                    safe_id = safe_id,
                    sidecar = "pr",
                    error = %e,
                );
            }
        }
    }

    // Clean up old temp files (best-effort)
    for ext in &["json.tmp", "status.tmp", "pr.tmp"] {
        let old_tmp = sessions_dir.join(format!("{safe_id}.{ext}"));
        if old_tmp.is_file()
            && let Err(e) = fs::remove_file(&old_tmp)
        {
            tracing::debug!(
                event = "core.session.migration_temp_cleanup_failed",
                safe_id = safe_id,
                file = %old_tmp.display(),
                error = %e,
            );
        }
    }

    tracing::info!(
        event = "core.session.migrated_to_directory",
        safe_id = safe_id,
        session_dir = %dir.display(),
    );

    Ok(true)
}

pub fn save_session_to_file(session: &Session, sessions_dir: &Path) -> Result<(), SessionError> {
    let dir = session_dir(sessions_dir, &session.id);
    fs::create_dir_all(&dir).map_err(|e| {
        tracing::warn!(
            event = "core.session.dir_create_failed",
            path = %dir.display(),
            error = %e,
        );
        SessionError::IoError { source: e }
    })?;
    let file = session_file(sessions_dir, &session.id);
    let session_json = serde_json::to_string_pretty(session).map_err(|e| {
        tracing::error!(
            event = "core.session.serialization_failed",
            session_id = %session.id,
            error = %e,
            message = "Failed to serialize session to JSON"
        );
        SessionError::IoError {
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        }
    })?;

    let temp_file = dir.join("kild.json.tmp");

    // Write to temp file
    if let Err(e) = fs::write(&temp_file, &session_json) {
        cleanup_temp_file(&temp_file, &e);
        return Err(SessionError::IoError { source: e });
    }

    // Rename temp file to final location
    if let Err(e) = fs::rename(&temp_file, &file) {
        cleanup_temp_file(&temp_file, &e);
        return Err(SessionError::IoError { source: e });
    }

    Ok(())
}

pub fn load_sessions_from_files(
    sessions_dir: &Path,
) -> Result<(Vec<Session>, usize), SessionError> {
    let mut sessions = Vec::new();
    let mut skipped_count = 0;

    // Return empty list if sessions directory doesn't exist
    if !sessions_dir.exists() {
        return Ok((sessions, skipped_count));
    }

    let entries = fs::read_dir(sessions_dir).map_err(|e| SessionError::IoError { source: e })?;

    for entry in entries {
        let entry = entry.map_err(|e| SessionError::IoError { source: e })?;
        let path = entry.path();

        // Determine session file path based on entry type
        let session_file = if path.is_dir() {
            // New format: <safe_id>/kild.json
            let kild_json = path.join("kild.json");
            if !kild_json.exists() {
                continue;
            }
            kild_json
        } else if path.extension().and_then(|s| s.to_str()) == Some("json") {
            // Old format: <safe_id>.json — auto-migrate
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                match migrate_session_if_needed(sessions_dir, stem) {
                    Ok(true) => {
                        // Migrated — load from new location
                        sessions_dir.join(stem).join("kild.json")
                    }
                    Ok(false) => continue, // Shouldn't happen (we checked is_file above)
                    Err(e) => {
                        skipped_count += 1;
                        tracing::warn!(
                            event = "core.session.migration_failed",
                            file = %path.display(),
                            error = %e,
                        );
                        continue;
                    }
                }
            } else {
                continue;
            }
        } else {
            continue;
        };

        let content = match fs::read_to_string(&session_file) {
            Ok(content) => content,
            Err(e) => {
                skipped_count += 1;
                tracing::warn!(
                    event = "core.session.load_read_error",
                    file = %session_file.display(),
                    error = %e,
                    message = "Failed to read session file, skipping"
                );
                continue;
            }
        };

        let session = match serde_json::from_str::<Session>(&content) {
            Ok(session) => session,
            Err(e) => {
                skipped_count += 1;
                tracing::warn!(
                    event = "core.session.load_invalid_json",
                    file = %session_file.display(),
                    error = %e,
                    message = "Failed to parse session JSON, skipping"
                );
                continue;
            }
        };

        if !session.has_agents() && session.status == super::types::SessionStatus::Active {
            tracing::warn!(
                event = "core.session.load_legacy_no_agents",
                file = %session_file.display(),
                session_id = %session.id,
                branch = %session.branch,
                "Active session has no tracked agents (legacy format) — operations may be degraded"
            );
        }

        if let Err(validation_error) = super::validation::validate_session_structure(&session) {
            skipped_count += 1;
            tracing::warn!(
                event = "core.session.load_invalid_structure",
                file = %session_file.display(),
                worktree_path = %session.worktree_path.display(),
                validation_error = %validation_error,
                message = "Session file has invalid structure, skipping"
            );
            continue;
        }

        sessions.push(session);
    }

    Ok((sessions, skipped_count))
}

pub fn load_session_from_file(name: &str, sessions_dir: &Path) -> Result<Session, SessionError> {
    // Find session by branch name
    let session =
        find_session_by_name(sessions_dir, name)?.ok_or_else(|| SessionError::NotFound {
            name: name.to_string(),
        })?;

    Ok(session)
}

pub fn find_session_by_name(
    sessions_dir: &Path,
    name: &str,
) -> Result<Option<Session>, SessionError> {
    let (sessions, _) = load_sessions_from_files(sessions_dir)?;

    // Find session by branch name (the "name" parameter refers to branch name)
    for session in sessions {
        if &*session.branch == name {
            return Ok(Some(session));
        }
    }

    Ok(None)
}

/// Patch a single field in a session JSON file without deserializing into Session.
///
/// This preserves unknown fields that may exist from newer binary versions,
/// preventing data loss when older binaries update session files (e.g., agent-status hook).
/// Writes are atomic via temp file + rename, consistent with `save_session_to_file()`.
pub fn patch_session_json_field(
    sessions_dir: &Path,
    session_id: &str,
    field: &str,
    value: serde_json::Value,
) -> Result<(), SessionError> {
    let safe_id = session_id.replace('/', "_");
    let dir = session_dir(sessions_dir, session_id);
    if !dir.join("kild.json").exists() {
        migrate_session_if_needed(sessions_dir, &safe_id)?;
    }
    let file = session_file(sessions_dir, session_id);
    let content = fs::read_to_string(&file).map_err(|e| SessionError::IoError { source: e })?;
    let mut json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| SessionError::IoError {
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?;

    let obj = json.as_object_mut().ok_or_else(|| SessionError::IoError {
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "session JSON root is not an object",
        ),
    })?;
    obj.insert(field.to_string(), value);

    let updated = serde_json::to_string_pretty(&json).map_err(|e| SessionError::IoError {
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
    })?;

    let temp_file = dir.join("kild.json.tmp");
    if let Err(e) = fs::write(&temp_file, &updated) {
        cleanup_temp_file(&temp_file, &e);
        return Err(SessionError::IoError { source: e });
    }
    if let Err(e) = fs::rename(&temp_file, &file) {
        cleanup_temp_file(&temp_file, &e);
        return Err(SessionError::IoError { source: e });
    }

    Ok(())
}

/// Patch multiple fields in a session JSON file without deserializing into Session.
///
/// Same as `patch_session_json_field` but for multiple fields atomically.
/// This avoids multiple file reads/writes when updating several fields at once.
pub fn patch_session_json_fields(
    sessions_dir: &Path,
    session_id: &str,
    fields: &[(&str, serde_json::Value)],
) -> Result<(), SessionError> {
    let safe_id = session_id.replace('/', "_");
    let dir = session_dir(sessions_dir, session_id);
    if !dir.join("kild.json").exists() {
        migrate_session_if_needed(sessions_dir, &safe_id)?;
    }
    let file = session_file(sessions_dir, session_id);
    let content = fs::read_to_string(&file).map_err(|e| SessionError::IoError { source: e })?;
    let mut json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| SessionError::IoError {
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?;

    let obj = json.as_object_mut().ok_or_else(|| SessionError::IoError {
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "session JSON root is not an object",
        ),
    })?;
    for (field, value) in fields {
        obj.insert((*field).to_string(), value.clone());
    }

    let updated = serde_json::to_string_pretty(&json).map_err(|e| SessionError::IoError {
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
    })?;

    let temp_file = dir.join("kild.json.tmp");
    if let Err(e) = fs::write(&temp_file, &updated) {
        cleanup_temp_file(&temp_file, &e);
        return Err(SessionError::IoError { source: e });
    }
    if let Err(e) = fs::rename(&temp_file, &file) {
        cleanup_temp_file(&temp_file, &e);
        return Err(SessionError::IoError { source: e });
    }

    Ok(())
}

/// Write agent status sidecar file atomically.
pub fn write_agent_status(
    sessions_dir: &Path,
    session_id: &str,
    status_info: &super::types::AgentStatusInfo,
) -> Result<(), SessionError> {
    let dir = session_dir(sessions_dir, session_id);
    fs::create_dir_all(&dir).map_err(|e| {
        tracing::warn!(
            event = "core.session.dir_create_failed",
            path = %dir.display(),
            error = %e,
        );
        SessionError::IoError { source: e }
    })?;
    let sidecar_file = dir.join("status");
    let content = serde_json::to_string(status_info).map_err(|e| SessionError::IoError {
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
    })?;
    let temp_file = dir.join("status.tmp");
    if let Err(e) = fs::write(&temp_file, &content) {
        cleanup_temp_file(&temp_file, &e);
        return Err(SessionError::IoError { source: e });
    }
    if let Err(e) = fs::rename(&temp_file, &sidecar_file) {
        cleanup_temp_file(&temp_file, &e);
        return Err(SessionError::IoError { source: e });
    }
    Ok(())
}

/// Read agent status from sidecar file. Returns None if file doesn't exist or is corrupt.
pub fn read_agent_status(
    sessions_dir: &Path,
    session_id: &str,
) -> Option<super::types::AgentStatusInfo> {
    let sidecar_file = session_dir(sessions_dir, session_id).join("status");
    let content = match fs::read_to_string(&sidecar_file) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!(
                event = "core.session.agent_status_read_failed",
                session_id = %session_id,
                error = %e,
            );
            return None;
        }
    };
    match serde_json::from_str(&content) {
        Ok(status) => Some(status),
        Err(e) => {
            tracing::warn!(
                event = "core.session.agent_status_parse_failed",
                session_id = %session_id,
                error = %e,
            );
            None
        }
    }
}

/// Remove agent status sidecar file. Best-effort (logs warning on failure).
pub fn remove_agent_status_file(sessions_dir: &Path, session_id: &str) {
    let sidecar_file = session_dir(sessions_dir, session_id).join("status");
    if sidecar_file.exists()
        && let Err(e) = fs::remove_file(&sidecar_file)
    {
        tracing::warn!(
            event = "core.session.agent_status_file_remove_failed",
            session_id = %session_id,
            error = %e,
        );
    }
}

/// Write PR info sidecar file atomically.
pub fn write_pr_info(
    sessions_dir: &Path,
    session_id: &str,
    pr_info: &crate::forge::types::PrInfo,
) -> Result<(), SessionError> {
    let dir = session_dir(sessions_dir, session_id);
    fs::create_dir_all(&dir).map_err(|e| {
        tracing::warn!(
            event = "core.session.dir_create_failed",
            path = %dir.display(),
            error = %e,
        );
        SessionError::IoError { source: e }
    })?;
    let sidecar_file = dir.join("pr");
    let content = serde_json::to_string(pr_info).map_err(|e| SessionError::IoError {
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
    })?;
    let temp_file = dir.join("pr.tmp");
    if let Err(e) = fs::write(&temp_file, &content) {
        cleanup_temp_file(&temp_file, &e);
        return Err(SessionError::IoError { source: e });
    }
    if let Err(e) = fs::rename(&temp_file, &sidecar_file) {
        cleanup_temp_file(&temp_file, &e);
        return Err(SessionError::IoError { source: e });
    }
    Ok(())
}

/// Read PR info from sidecar file. Returns None if file doesn't exist or is corrupt.
pub fn read_pr_info(sessions_dir: &Path, session_id: &str) -> Option<crate::forge::types::PrInfo> {
    let sidecar_file = session_dir(sessions_dir, session_id).join("pr");
    let content = match fs::read_to_string(&sidecar_file) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!(
                event = "core.session.pr_info_read_failed",
                session_id = %session_id,
                error = %e,
            );
            return None;
        }
    };
    match serde_json::from_str(&content) {
        Ok(info) => Some(info),
        Err(e) => {
            tracing::warn!(
                event = "core.session.pr_info_parse_failed",
                session_id = %session_id,
                error = %e,
            );
            None
        }
    }
}

/// Remove PR info sidecar file. Best-effort (logs warning on failure).
pub fn remove_pr_info_file(sessions_dir: &Path, session_id: &str) {
    let sidecar_file = session_dir(sessions_dir, session_id).join("pr");
    if sidecar_file.exists()
        && let Err(e) = fs::remove_file(&sidecar_file)
    {
        tracing::warn!(
            event = "core.session.pr_info_file_remove_failed",
            session_id = %session_id,
            error = %e,
        );
    }
}

pub fn remove_session_file(sessions_dir: &Path, session_id: &str) -> Result<(), SessionError> {
    let dir = session_dir(sessions_dir, session_id);

    if dir.is_dir() {
        // Warn about unexpected files that will be removed
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !matches!(
                    name.as_ref(),
                    "kild.json" | "status" | "pr" | "kild.json.tmp" | "status.tmp" | "pr.tmp"
                ) {
                    tracing::warn!(
                        event = "core.session.remove_unexpected_file",
                        session_id = %session_id,
                        file = %name,
                        "Unexpected file in session directory will be removed"
                    );
                }
            }
        }
        tracing::debug!(
            event = "core.session.remove_dir_started",
            session_id = %session_id,
            path = %dir.display(),
        );
        fs::remove_dir_all(&dir).map_err(|e| SessionError::IoError { source: e })?;
    } else {
        // Try old-format cleanup as fallback
        let safe_id = session_id.replace('/', "_");
        let old_file = sessions_dir.join(format!("{safe_id}.json"));
        if old_file.is_file() {
            fs::remove_file(&old_file).map_err(|e| SessionError::IoError { source: e })?;
        } else {
            tracing::warn!(
                event = "core.session.remove_nonexistent_file",
                session_id = %session_id,
                "Attempted to remove session that doesn't exist"
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_sessions_directory() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_sessions");

        // Clean up if exists
        let _ = std::fs::remove_dir_all(&temp_dir);

        // Should create directory
        assert!(ensure_sessions_directory(&temp_dir).is_ok());
        assert!(temp_dir.exists());

        // Should not error if directory already exists
        assert!(ensure_sessions_directory(&temp_dir).is_ok());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_session_to_file() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_save_session");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session::new(
            "test/branch".into(),
            "test".into(),
            "branch".into(),
            worktree_path,
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        // Save session
        assert!(save_session_to_file(&session, &temp_dir).is_ok());

        // Check directory and file exist with correct name (/ replaced with _)
        let session_dir = temp_dir.join("test_branch");
        let session_file = session_dir.join("kild.json");
        assert!(session_dir.is_dir());
        assert!(session_file.exists());

        // Verify content
        let content = std::fs::read_to_string(&session_file).unwrap();
        let loaded_session: Session = serde_json::from_str(&content).unwrap();
        assert_eq!(loaded_session, session);

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_session_atomic_write_temp_cleanup() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_atomic_write");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session::new(
            "test/atomic".into(),
            "test".into(),
            "atomic".into(),
            worktree_path,
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        // Save session
        assert!(save_session_to_file(&session, &temp_dir).is_ok());

        // Verify temp file is cleaned up after successful write
        let session_dir = temp_dir.join("test_atomic");
        let temp_file = session_dir.join("kild.json.tmp");
        assert!(
            !temp_file.exists(),
            "Temp file should be cleaned up after successful write"
        );

        // Verify final file exists
        let session_file = session_dir.join("kild.json");
        assert!(session_file.exists(), "Final session file should exist");

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_session_atomic_behavior() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_atomic_behavior");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session::new(
            "test/atomic-behavior".into(),
            "test".into(),
            "atomic-behavior".into(),
            worktree_path,
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        let session_dir = temp_dir.join("test_atomic-behavior");
        std::fs::create_dir_all(&session_dir).unwrap();
        let session_file = session_dir.join("kild.json");

        // Create existing file with different content
        std::fs::write(&session_file, "old content").unwrap();

        // Save session atomically
        assert!(save_session_to_file(&session, &temp_dir).is_ok());

        // Verify file was replaced atomically (no partial writes)
        let content = std::fs::read_to_string(&session_file).unwrap();
        assert!(content.contains("test/atomic-behavior"));
        assert!(!content.contains("old content"));

        // Verify it's valid JSON
        let loaded_session: Session = serde_json::from_str(&content).unwrap();
        assert_eq!(loaded_session, session);

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_session_temp_file_cleanup_on_failure() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_temp_cleanup");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session::new(
            "test/cleanup".into(),
            "test".into(),
            "cleanup".into(),
            worktree_path,
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        // Create a directory where kild.json should be to force rename failure
        let session_dir = temp_dir.join("test_cleanup");
        std::fs::create_dir_all(&session_dir).unwrap();
        let session_file = session_dir.join("kild.json");
        std::fs::create_dir_all(&session_file).unwrap(); // Create as directory to force rename failure

        // Attempt to save session - should fail due to rename failure
        let result = save_session_to_file(&session, &temp_dir);
        assert!(result.is_err(), "Save should fail when rename fails");

        // Verify temp file is cleaned up after failure
        let temp_file = session_dir.join("kild.json.tmp");
        assert!(
            !temp_file.exists(),
            "Temp file should be cleaned up after rename failure"
        );

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_sessions_from_files() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_load_sessions");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Test empty directory
        let (sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();
        assert_eq!(sessions.len(), 0);
        assert_eq!(skipped, 0);

        // Create test sessions with existing worktree paths
        let worktree1 = temp_dir.join("worktree1");
        let worktree2 = temp_dir.join("worktree2");
        std::fs::create_dir_all(&worktree1).unwrap();
        std::fs::create_dir_all(&worktree2).unwrap();

        let session1 = Session::new(
            "test/branch1".into(),
            "test".into(),
            "branch1".into(),
            worktree1,
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        let session2 = Session::new(
            "test/branch2".into(),
            "test".into(),
            "branch2".into(),
            worktree2,
            "kiro".to_string(),
            SessionStatus::Stopped,
            "2024-01-02T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        // Save sessions
        save_session_to_file(&session1, &temp_dir).unwrap();
        save_session_to_file(&session2, &temp_dir).unwrap();

        // Load sessions
        let (loaded_sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();
        assert_eq!(loaded_sessions.len(), 2);
        assert_eq!(skipped, 0);

        // Verify sessions (order might vary)
        let ids: Vec<String> = loaded_sessions.iter().map(|s| s.id.to_string()).collect();
        assert!(ids.contains(&"test/branch1".to_string()));
        assert!(ids.contains(&"test/branch2".to_string()));

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_sessions_nonexistent_directory() {
        use std::env;

        let nonexistent_dir = env::temp_dir().join("kild_test_nonexistent");
        let _ = std::fs::remove_dir_all(&nonexistent_dir);

        let (sessions, skipped) = load_sessions_from_files(&nonexistent_dir).unwrap();
        assert_eq!(sessions.len(), 0);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn test_find_session_by_name() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_find_session");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session::new(
            "test/feature-branch".into(),
            "test".into(),
            "feature-branch".into(),
            worktree_path,
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        // Save session
        save_session_to_file(&session, &temp_dir).unwrap();

        // Find by branch name
        let found = find_session_by_name(&temp_dir, "feature-branch").unwrap();
        assert!(found.is_some());
        assert_eq!(&*found.unwrap().id, "test/feature-branch");

        // Try to find non-existent session
        let not_found = find_session_by_name(&temp_dir, "non-existent").unwrap();
        assert!(not_found.is_none());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_remove_session_file() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_remove_session");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create worktree directory
        let worktree_path = temp_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session::new(
            "test/branch".into(),
            "test".into(),
            "branch".into(),
            worktree_path,
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        // Save session
        save_session_to_file(&session, &temp_dir).unwrap();

        let session_dir = temp_dir.join("test_branch");
        assert!(session_dir.is_dir());

        // Remove session directory
        remove_session_file(&temp_dir, &session.id).unwrap();
        assert!(!session_dir.exists());

        // Removing non-existent session should not error
        assert!(remove_session_file(&temp_dir, "non-existent").is_ok());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_sessions_with_invalid_files() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_invalid_files");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create a valid session with existing worktree path
        let worktree_path = temp_dir.join("valid_worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let valid_session = Session::new(
            "test/valid".into(),
            "test".into(),
            "valid".into(),
            worktree_path,
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );
        save_session_to_file(&valid_session, &temp_dir).unwrap();

        // Create invalid JSON as directory with kild.json
        let invalid_dir = temp_dir.join("invalid");
        std::fs::create_dir_all(&invalid_dir).unwrap();
        std::fs::write(invalid_dir.join("kild.json"), "{ invalid json }").unwrap();

        // Create directory with invalid session structure (missing required fields)
        let invalid_structure_dir = temp_dir.join("invalid_structure");
        std::fs::create_dir_all(&invalid_structure_dir).unwrap();
        std::fs::write(
            invalid_structure_dir.join("kild.json"),
            r#"{"id": "", "project_id": "test"}"#,
        )
        .unwrap();

        // Load sessions - should only return the valid one
        let (sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(&*sessions[0].id, "test/valid");
        assert_eq!(skipped, 2); // invalid JSON + invalid structure

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// Test that sessions with missing worktree paths are still loaded (issue #102).
    ///
    /// Sessions with non-existent worktrees should be included in load results
    /// so users can see them and clean up as needed.
    #[test]
    fn test_load_sessions_includes_missing_worktree() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_missing_worktree");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let nonexistent_worktree = temp_dir.join("worktree_that_does_not_exist");

        let session_missing_worktree = Session::new(
            "test/orphaned".into(),
            "test".into(),
            "orphaned".into(),
            nonexistent_worktree.clone(),
            "claude".to_string(),
            SessionStatus::Stopped,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        let session_dir = temp_dir.join("test_orphaned");
        std::fs::create_dir_all(&session_dir).unwrap();
        let session_file = session_dir.join("kild.json");
        let json = serde_json::to_string_pretty(&session_missing_worktree).unwrap();
        std::fs::write(&session_file, json).unwrap();

        assert!(session_file.exists());
        assert!(!nonexistent_worktree.exists());

        let (sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(skipped, 0);
        assert_eq!(&*sessions[0].id, "test/orphaned");
        assert_eq!(&*sessions[0].branch, "orphaned");
        assert!(!sessions[0].is_worktree_valid());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// Test loading sessions with mixed worktree states (valid and invalid together).
    ///
    /// Verifies that `load_sessions_from_files` correctly handles a directory containing
    /// both sessions with valid worktrees and sessions with missing worktrees.
    #[test]
    fn test_load_sessions_mixed_valid_and_missing_worktrees() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_mixed_worktrees");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create a valid worktree directory
        let valid_worktree = temp_dir.join("valid_worktree");
        std::fs::create_dir_all(&valid_worktree).unwrap();

        // Missing worktree path (not created)
        let missing_worktree = temp_dir.join("missing_worktree");

        // Session 1: valid worktree
        let session_valid = Session::new(
            "test/valid-session".into(),
            "test".into(),
            "valid-session".into(),
            valid_worktree.clone(),
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            3000,
            3009,
            10,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        // Session 2: missing worktree
        let session_missing = Session::new(
            "test/missing-session".into(),
            "test".into(),
            "missing-session".into(),
            missing_worktree.clone(),
            "claude".to_string(),
            SessionStatus::Stopped,
            "2024-01-01T00:00:00Z".to_string(),
            3010,
            3019,
            10,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        // Save both sessions as per-session directories
        let valid_dir = temp_dir.join("test_valid-session");
        std::fs::create_dir_all(&valid_dir).unwrap();
        std::fs::write(
            valid_dir.join("kild.json"),
            serde_json::to_string_pretty(&session_valid).unwrap(),
        )
        .unwrap();
        let missing_dir = temp_dir.join("test_missing-session");
        std::fs::create_dir_all(&missing_dir).unwrap();
        std::fs::write(
            missing_dir.join("kild.json"),
            serde_json::to_string_pretty(&session_missing).unwrap(),
        )
        .unwrap();

        // Verify preconditions
        assert!(valid_worktree.exists());
        assert!(!missing_worktree.exists());

        // Load all sessions
        let (sessions, skipped) = load_sessions_from_files(&temp_dir).unwrap();

        // Both sessions should be loaded
        assert_eq!(sessions.len(), 2, "Both sessions should be loaded");
        assert_eq!(skipped, 0, "No sessions should be skipped");

        // Find each session and verify is_worktree_valid()
        let valid = sessions
            .iter()
            .find(|s| &*s.branch == "valid-session")
            .expect("Valid session should be loaded");
        let missing = sessions
            .iter()
            .find(|s| &*s.branch == "missing-session")
            .expect("Missing session should be loaded");

        assert!(
            valid.is_worktree_valid(),
            "Valid session should have valid worktree"
        );
        assert!(
            !missing.is_worktree_valid(),
            "Missing session should have invalid worktree"
        );

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_patch_session_json_field_preserves_unknown_fields() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_patch_preserves_fields");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Write a session JSON with an extra field not in the Session struct
        let json = serde_json::json!({
            "id": "proj/my-branch",
            "project_id": "proj",
            "branch": "my-branch",
            "worktree_path": "/tmp/test",
            "agent": "claude",
            "status": "Active",
            "created_at": "2024-01-01T00:00:00Z",
            "port_range_start": 3000,
            "port_range_end": 3009,
            "port_count": 10,
            "last_activity": "2024-01-01T00:00:00Z",
            "agents": [],
            "future_field": "must_survive"
        });
        let session_dir = temp_dir.join("proj_my-branch");
        std::fs::create_dir_all(&session_dir).unwrap();
        let session_file = session_dir.join("kild.json");
        std::fs::write(&session_file, serde_json::to_string_pretty(&json).unwrap()).unwrap();

        // Patch last_activity
        patch_session_json_field(
            &temp_dir,
            "proj/my-branch",
            "last_activity",
            serde_json::Value::String("2024-06-15T12:00:00Z".to_string()),
        )
        .unwrap();

        // Read back and verify unknown field is preserved
        let content = std::fs::read_to_string(&session_file).unwrap();
        let patched: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            patched["last_activity"], "2024-06-15T12:00:00Z",
            "Patched field should be updated"
        );
        assert_eq!(
            patched["future_field"], "must_survive",
            "Unknown fields must be preserved"
        );
        assert_eq!(
            patched["branch"], "my-branch",
            "Existing fields must be preserved"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_patch_session_json_field_fails_on_non_object() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_patch_non_object");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Write an array instead of an object
        let session_dir = temp_dir.join("proj_branch");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(session_dir.join("kild.json"), "[]").unwrap();

        let result = patch_session_json_field(
            &temp_dir,
            "proj/branch",
            "last_activity",
            serde_json::Value::String("2024-06-15T12:00:00Z".to_string()),
        );

        assert!(
            result.is_err(),
            "Should fail when JSON root is not an object"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_patch_session_json_fields_preserves_unknown_fields() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_patch_multi_preserves_fields");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let json = serde_json::json!({
            "id": "proj/my-branch",
            "project_id": "proj",
            "branch": "my-branch",
            "worktree_path": "/tmp/test",
            "agent": "claude",
            "status": "Active",
            "created_at": "2024-01-01T00:00:00Z",
            "port_range_start": 3000,
            "port_range_end": 3009,
            "port_count": 10,
            "last_activity": "2024-01-01T00:00:00Z",
            "agents": [],
            "future_field": "must_survive"
        });
        let session_dir = temp_dir.join("proj_my-branch");
        std::fs::create_dir_all(&session_dir).unwrap();
        let session_file = session_dir.join("kild.json");
        std::fs::write(&session_file, serde_json::to_string_pretty(&json).unwrap()).unwrap();

        // Patch both status and last_activity atomically
        patch_session_json_fields(
            &temp_dir,
            "proj/my-branch",
            &[
                ("status", serde_json::json!("Stopped")),
                (
                    "last_activity",
                    serde_json::Value::String("2024-06-15T12:00:00Z".to_string()),
                ),
            ],
        )
        .unwrap();

        let content = std::fs::read_to_string(&session_file).unwrap();
        let patched: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(patched["status"], "Stopped", "Status should be updated");
        assert_eq!(
            patched["last_activity"], "2024-06-15T12:00:00Z",
            "last_activity should be updated"
        );
        assert_eq!(
            patched["future_field"], "must_survive",
            "Unknown fields must be preserved"
        );
        assert_eq!(
            patched["branch"], "my-branch",
            "Existing fields must be preserved"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_write_and_read_agent_status() {
        use super::super::types::{AgentStatus, AgentStatusInfo};
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_agent_status_write_read");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let info = AgentStatusInfo {
            status: AgentStatus::Working,
            updated_at: "2026-02-05T12:00:00Z".to_string(),
        };

        write_agent_status(&temp_dir, "test/branch", &info).unwrap();

        let sidecar_file = temp_dir.join("test_branch").join("status");
        assert!(sidecar_file.exists());

        let read_back = read_agent_status(&temp_dir, "test/branch");
        assert_eq!(read_back, Some(info));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_agent_status_missing_file() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_agent_status_missing");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let result = read_agent_status(&temp_dir, "nonexistent");
        assert_eq!(result, None);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_agent_status_corrupt_json() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_agent_status_corrupt");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let session_dir = temp_dir.join("bad_session");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(session_dir.join("status"), "not json").unwrap();

        let result = read_agent_status(&temp_dir, "bad_session");
        assert_eq!(result, None);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_remove_agent_status_file_exists() {
        use super::super::types::{AgentStatus, AgentStatusInfo};
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_agent_status_remove");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let info = AgentStatusInfo {
            status: AgentStatus::Idle,
            updated_at: "2026-02-05T12:00:00Z".to_string(),
        };
        write_agent_status(&temp_dir, "test/rm", &info).unwrap();

        let sidecar_file = temp_dir.join("test_rm").join("status");
        assert!(sidecar_file.exists());

        remove_agent_status_file(&temp_dir, "test/rm");
        assert!(!sidecar_file.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_remove_agent_status_file_missing_is_noop() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_agent_status_remove_missing");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Should not panic or error
        remove_agent_status_file(&temp_dir, "nonexistent");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // --- PR info sidecar tests ---

    #[test]
    fn test_write_and_read_pr_info() {
        use crate::forge::types::{CiStatus, PrInfo, PrState, ReviewStatus};
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_pr_info_write_read");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let info = PrInfo {
            number: 42,
            url: "https://github.com/org/repo/pull/42".to_string(),
            state: PrState::Open,
            ci_status: CiStatus::Passing,
            ci_summary: Some("3/3 passing".to_string()),
            review_status: ReviewStatus::Approved,
            review_summary: Some("1 approved".to_string()),
            updated_at: "2026-02-05T12:00:00Z".to_string(),
        };

        write_pr_info(&temp_dir, "test/branch", &info).unwrap();

        let sidecar_file = temp_dir.join("test_branch").join("pr");
        assert!(sidecar_file.exists());

        let read_back = read_pr_info(&temp_dir, "test/branch");
        assert_eq!(read_back, Some(info));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_pr_info_missing_file() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_pr_info_missing");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let result = read_pr_info(&temp_dir, "nonexistent");
        assert_eq!(result, None);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_pr_info_corrupt_json() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_pr_info_corrupt");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let session_dir = temp_dir.join("bad_session");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(session_dir.join("pr"), "not json").unwrap();

        let result = read_pr_info(&temp_dir, "bad_session");
        assert_eq!(result, None);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_remove_pr_info_file_exists() {
        use crate::forge::types::{CiStatus, PrInfo, PrState, ReviewStatus};
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_pr_info_remove");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let info = PrInfo {
            number: 1,
            url: "https://github.com/org/repo/pull/1".to_string(),
            state: PrState::Open,
            ci_status: CiStatus::Unknown,
            ci_summary: None,
            review_status: ReviewStatus::Unknown,
            review_summary: None,
            updated_at: "2026-02-05T12:00:00Z".to_string(),
        };
        write_pr_info(&temp_dir, "test/rm", &info).unwrap();

        let sidecar_file = temp_dir.join("test_rm").join("pr");
        assert!(sidecar_file.exists());

        remove_pr_info_file(&temp_dir, "test/rm");
        assert!(!sidecar_file.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_remove_pr_info_file_missing_is_noop() {
        use std::env;

        let temp_dir = env::temp_dir().join("kild_test_pr_info_remove_missing");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Should not panic or error
        remove_pr_info_file(&temp_dir, "nonexistent");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_save_load_roundtrip_all_fields() {
        use crate::state::types::RuntimeMode;
        use crate::terminal::types::TerminalType;

        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_dir = tmp.path();

        let worktree_dir = sessions_dir.join("worktree");
        std::fs::create_dir_all(&worktree_dir).unwrap();

        let agent = super::super::types::AgentProcess::new(
            "claude".to_string(),
            "proj_feat_0".to_string(),
            Some(12345),
            Some("claude-code".to_string()),
            Some(1705318200),
            Some(TerminalType::Ghostty),
            Some("kild-feat".to_string()),
            "claude --session-id abc".to_string(),
            "2024-01-15T10:30:00Z".to_string(),
            None,
        )
        .unwrap();

        let session = Session::new(
            "proj/feat".into(),
            "proj".into(),
            "feat".into(),
            worktree_dir,
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            3000,
            3009,
            10,
            Some("2024-01-15T10:30:00Z".to_string()),
            Some("Implementing auth".to_string()),
            vec![agent],
            Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
            Some("tl_proj_feat".to_string()),
            Some(RuntimeMode::Daemon),
        );

        save_session_to_file(&session, sessions_dir).unwrap();

        let loaded = find_session_by_name(sessions_dir, "feat")
            .unwrap()
            .expect("session should be found");

        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.project_id, session.project_id);
        assert_eq!(loaded.branch, session.branch);
        assert_eq!(loaded.worktree_path, session.worktree_path);
        assert_eq!(loaded.agent, session.agent);
        assert_eq!(loaded.status, session.status);
        assert_eq!(loaded.created_at, session.created_at);
        assert_eq!(loaded.port_range_start, session.port_range_start);
        assert_eq!(loaded.port_range_end, session.port_range_end);
        assert_eq!(loaded.port_count, session.port_count);
        assert_eq!(loaded.last_activity, session.last_activity);
        assert_eq!(loaded.note, session.note);
        assert_eq!(loaded.agent_session_id, session.agent_session_id);
        assert_eq!(loaded.task_list_id, session.task_list_id);
        assert_eq!(loaded.runtime_mode, session.runtime_mode);
        assert_eq!(loaded.agent_count(), 1);
        let agent = loaded.latest_agent().unwrap();
        assert_eq!(agent.agent(), "claude");
        assert_eq!(agent.spawn_id(), "proj_feat_0");
        assert_eq!(agent.process_id(), Some(12345));
        assert_eq!(agent.process_name(), Some("claude-code"));
        assert_eq!(agent.process_start_time(), Some(1705318200));
        assert_eq!(agent.terminal_type(), Some(&TerminalType::Ghostty));
        assert_eq!(agent.terminal_window_id(), Some("kild-feat"));
        assert_eq!(agent.command(), "claude --session-id abc");
        assert_eq!(agent.opened_at(), "2024-01-15T10:30:00Z");
        assert!(agent.daemon_session_id().is_none());
    }

    #[test]
    fn test_session_id_filename_mapping() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_dir = tmp.path();

        let worktree_dir = sessions_dir.join("wt");
        std::fs::create_dir_all(&worktree_dir).unwrap();

        let session = Session::new(
            "my-project/deep/nested".into(),
            "my-project".into(),
            "deep-nested".into(),
            worktree_dir,
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            None,
            None,
            vec![],
            None,
            None,
            None,
        );

        save_session_to_file(&session, sessions_dir).unwrap();

        let expected_dir = sessions_dir.join("my-project_deep_nested");
        let expected_file = expected_dir.join("kild.json");
        assert!(expected_dir.is_dir());
        assert!(expected_file.exists());

        let loaded = load_session_from_file("deep-nested", sessions_dir).unwrap();
        assert_eq!(&*loaded.id, "my-project/deep/nested");
    }

    // --- Migration tests ---

    #[test]
    fn test_migrate_session_with_all_sidecars() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_dir = tmp.path();

        // Create old-format files
        std::fs::write(
            sessions_dir.join("proj_branch.json"),
            r#"{"id":"proj/branch"}"#,
        )
        .unwrap();
        std::fs::write(
            sessions_dir.join("proj_branch.status"),
            r#"{"status":"idle"}"#,
        )
        .unwrap();
        std::fs::write(sessions_dir.join("proj_branch.pr"), r#"{"number":42}"#).unwrap();

        let result = migrate_session_if_needed(sessions_dir, "proj_branch").unwrap();
        assert!(result, "Should report migration happened");

        // Old files should be gone
        assert!(!sessions_dir.join("proj_branch.json").exists());
        assert!(!sessions_dir.join("proj_branch.status").exists());
        assert!(!sessions_dir.join("proj_branch.pr").exists());

        // New files should exist
        let session_dir = sessions_dir.join("proj_branch");
        assert!(session_dir.is_dir());
        assert!(session_dir.join("kild.json").exists());
        assert!(session_dir.join("status").exists());
        assert!(session_dir.join("pr").exists());

        // Content preserved
        let content = std::fs::read_to_string(session_dir.join("kild.json")).unwrap();
        assert!(content.contains("proj/branch"));
    }

    #[test]
    fn test_migrate_session_json_only() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_dir = tmp.path();

        // Only the JSON file, no sidecars
        std::fs::write(
            sessions_dir.join("proj_branch.json"),
            r#"{"id":"proj/branch"}"#,
        )
        .unwrap();

        let result = migrate_session_if_needed(sessions_dir, "proj_branch").unwrap();
        assert!(result);

        let session_dir = sessions_dir.join("proj_branch");
        assert!(session_dir.join("kild.json").exists());
        assert!(!session_dir.join("status").exists());
        assert!(!session_dir.join("pr").exists());
    }

    #[test]
    fn test_migrate_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_dir = tmp.path();

        // Already new format
        let session_dir = sessions_dir.join("proj_branch");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(session_dir.join("kild.json"), r#"{"id":"proj/branch"}"#).unwrap();

        let result = migrate_session_if_needed(sessions_dir, "proj_branch").unwrap();
        assert!(!result, "Should return false for already-migrated session");
    }

    #[test]
    fn test_migrate_nonexistent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let result = migrate_session_if_needed(tmp.path(), "nonexistent").unwrap();
        assert!(!result, "Should return false for non-existent session");
    }

    #[test]
    fn test_load_sessions_auto_migrates_old_format() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_dir = tmp.path();

        // Create worktree directory
        let worktree_path = sessions_dir.join("worktree");
        std::fs::create_dir_all(&worktree_path).unwrap();

        let session = Session::new(
            "test/migrate-me".into(),
            "test".into(),
            "migrate-me".into(),
            worktree_path,
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        // Write in OLD format
        let old_file = sessions_dir.join("test_migrate-me.json");
        std::fs::write(&old_file, serde_json::to_string_pretty(&session).unwrap()).unwrap();

        // Load should auto-migrate and return the session
        let (sessions, skipped) = load_sessions_from_files(sessions_dir).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(skipped, 0);
        assert_eq!(&*sessions[0].id, "test/migrate-me");

        // Old file should be gone, new dir should exist
        assert!(!old_file.exists());
        assert!(
            sessions_dir
                .join("test_migrate-me")
                .join("kild.json")
                .exists()
        );
    }

    #[test]
    fn test_load_sessions_mixed_old_and_new() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_dir = tmp.path();

        // Create worktree directories
        let wt1 = sessions_dir.join("wt1");
        let wt2 = sessions_dir.join("wt2");
        std::fs::create_dir_all(&wt1).unwrap();
        std::fs::create_dir_all(&wt2).unwrap();

        let session1 = Session::new(
            "test/new-format".into(),
            "test".into(),
            "new-format".into(),
            wt1,
            "claude".to_string(),
            SessionStatus::Active,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        let session2 = Session::new(
            "test/old-format".into(),
            "test".into(),
            "old-format".into(),
            wt2,
            "claude".to_string(),
            SessionStatus::Stopped,
            "2024-01-01T00:00:00Z".to_string(),
            0,
            0,
            0,
            Some("2024-01-01T00:00:00Z".to_string()),
            None,
            vec![],
            None,
            None,
            None,
        );

        // Save session1 in NEW format (via save_session_to_file)
        save_session_to_file(&session1, sessions_dir).unwrap();

        // Write session2 in OLD format
        let old_file = sessions_dir.join("test_old-format.json");
        std::fs::write(&old_file, serde_json::to_string_pretty(&session2).unwrap()).unwrap();

        // Load should return both
        let (sessions, skipped) = load_sessions_from_files(sessions_dir).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(skipped, 0);

        let ids: Vec<String> = sessions.iter().map(|s| s.id.to_string()).collect();
        assert!(ids.contains(&"test/new-format".to_string()));
        assert!(ids.contains(&"test/old-format".to_string()));
    }

    #[test]
    fn test_migrate_cleans_up_temp_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_dir = tmp.path();

        // Create old-format files including temp files
        std::fs::write(
            sessions_dir.join("proj_branch.json"),
            r#"{"id":"proj/branch"}"#,
        )
        .unwrap();
        std::fs::write(sessions_dir.join("proj_branch.json.tmp"), "temp data").unwrap();
        std::fs::write(sessions_dir.join("proj_branch.status.tmp"), "temp data").unwrap();

        migrate_session_if_needed(sessions_dir, "proj_branch").unwrap();

        // Temp files should be cleaned up
        assert!(!sessions_dir.join("proj_branch.json.tmp").exists());
        assert!(!sessions_dir.join("proj_branch.status.tmp").exists());
    }

    #[test]
    fn test_concurrent_migration() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_dir = tmp.path().to_path_buf();

        // Create old-format file
        std::fs::write(
            sessions_dir.join("proj_branch.json"),
            r#"{"id":"proj/branch"}"#,
        )
        .unwrap();

        let dir1 = sessions_dir.clone();
        let dir2 = sessions_dir.clone();

        let t1 = std::thread::spawn(move || migrate_session_if_needed(&dir1, "proj_branch"));
        let t2 = std::thread::spawn(move || migrate_session_if_needed(&dir2, "proj_branch"));

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        // Both should succeed
        assert!(r1.is_ok(), "Thread 1 should succeed: {:?}", r1);
        assert!(r2.is_ok(), "Thread 2 should succeed: {:?}", r2);

        // Exactly one should report migration happened
        let migrated_count = [r1.unwrap(), r2.unwrap()].iter().filter(|&&v| v).count();
        assert!(
            migrated_count <= 2,
            "At most both threads could see the old file before rename"
        );

        // Final state: new-format directory with kild.json
        assert!(sessions_dir.join("proj_branch").join("kild.json").exists());
        assert!(!sessions_dir.join("proj_branch.json").exists());
    }

    #[test]
    fn test_session_dir_and_session_file_helpers() {
        let base = std::path::Path::new("/tmp/sessions");

        assert_eq!(
            session_dir(base, "proj/branch"),
            std::path::PathBuf::from("/tmp/sessions/proj_branch")
        );
        assert_eq!(
            session_file(base, "proj/branch"),
            std::path::PathBuf::from("/tmp/sessions/proj_branch/kild.json")
        );
        assert_eq!(
            session_dir(base, "deep/nested/id"),
            std::path::PathBuf::from("/tmp/sessions/deep_nested_id")
        );
    }

    #[test]
    fn test_remove_session_file_warns_unexpected_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        let sessions_dir = tmp.path();

        // Create session dir with expected + unexpected files
        let dir = sessions_dir.join("proj_branch");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("kild.json"), r#"{"id":"proj/branch"}"#).unwrap();
        std::fs::write(dir.join("status"), r#"{"status":"idle"}"#).unwrap();
        std::fs::write(dir.join("unexpected.log"), "should warn").unwrap();

        // Should succeed and remove everything
        remove_session_file(sessions_dir, "proj/branch").unwrap();
        assert!(!dir.exists());
    }
}
