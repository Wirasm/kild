//! Session file CRUD operations
//!
//! Handles reading/writing session data to disk with atomic operations.

use crate::sessions::{errors::SessionError, types::*};
use std::fs;
use std::path::{Path, PathBuf};

/// Compute session directory path: `<sessions_dir>/<safe_id>/`.
pub(super) fn session_dir(sessions_dir: &Path, session_id: &str) -> PathBuf {
    let safe_id = session_id.replace('/', "_");
    sessions_dir.join(safe_id)
}

/// Compute session file path: `<sessions_dir>/<safe_id>/kild.json`.
pub(super) fn session_file(sessions_dir: &Path, session_id: &str) -> PathBuf {
    session_dir(sessions_dir, session_id).join("kild.json")
}

pub fn ensure_sessions_directory(sessions_dir: &Path) -> Result<(), SessionError> {
    fs::create_dir_all(sessions_dir).map_err(|e| SessionError::IoError { source: e })?;
    Ok(())
}

pub(super) fn cleanup_temp_file(temp_file: &Path, original_error: &std::io::Error) {
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
pub(super) fn migrate_session_if_needed(
    sessions_dir: &Path,
    safe_id: &str,
) -> Result<bool, SessionError> {
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
    let session_json = serde_json::to_string(session).map_err(|e| {
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

    // Maintain branch index for O(1) find_session_by_name lookups
    super::index::update_branch_index(sessions_dir, &session.branch, &session.id);

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
            // Skip index file — it is not a session file
            if path.file_name().and_then(|s| s.to_str()) == Some(super::index::INDEX_FILE) {
                continue;
            }
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

        if !session.has_agents() && session.status == super::super::types::SessionStatus::Active {
            tracing::warn!(
                event = "core.session.load_legacy_no_agents",
                file = %session_file.display(),
                session_id = %session.id,
                branch = %session.branch,
                "Active session has no tracked agents (legacy format) — operations may be degraded"
            );
        }

        if let Err(validation_error) =
            super::super::validation::validate_session_structure(&session)
        {
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
    // Fast path: try branch index first
    if let Some(session_id) = super::index::lookup_branch(sessions_dir, name) {
        let file = session_file(sessions_dir, &session_id);
        if file.exists() {
            let content =
                fs::read_to_string(&file).map_err(|e| SessionError::IoError { source: e })?;
            if let Ok(session) = serde_json::from_str::<Session>(&content)
                && &*session.branch == name
            {
                return Ok(Some(session));
            }
            // Index stale (branch renamed or session replaced) — fall through to scan
        }
        // Index entry exists but file is gone or stale — fall through to scan
    }

    // Slow path: full scan (index miss or stale)
    let (sessions, _) = load_sessions_from_files(sessions_dir)?;
    for session in sessions {
        if &*session.branch == name {
            // Opportunistically repair the index
            super::index::update_branch_index(sessions_dir, name, &session.id);
            return Ok(Some(session));
        }
    }

    Ok(None)
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

    super::index::purge_session_id_from_branch_index(sessions_dir, session_id);

    Ok(())
}
