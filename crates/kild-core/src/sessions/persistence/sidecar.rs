//! Sidecar file operations for agent status and PR info
//!
//! Sidecar files are stored inside per-session directories alongside `kild.json`.

use crate::sessions::errors::SessionError;
use std::fs;
use std::path::Path;

use super::session_files::{cleanup_temp_file, session_dir};

/// Write agent status sidecar file atomically.
pub fn write_agent_status(
    sessions_dir: &Path,
    session_id: &str,
    status_info: &crate::sessions::types::AgentStatusInfo,
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
) -> Option<crate::sessions::types::AgentStatusInfo> {
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
    pr_info: &crate::forge::types::PullRequest,
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
pub fn read_pr_info(
    sessions_dir: &Path,
    session_id: &str,
) -> Option<crate::forge::types::PullRequest> {
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
