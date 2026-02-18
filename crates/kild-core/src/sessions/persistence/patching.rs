//! JSON field patching for session files
//!
//! Patches individual fields without full deserialization to preserve unknown fields.

use crate::sessions::errors::SessionError;
use std::fs;
use std::path::Path;

use super::session_files::{
    cleanup_temp_file, migrate_session_if_needed, session_dir, session_file,
};

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

    let updated = serde_json::to_string(&json).map_err(|e| SessionError::IoError {
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

    let updated = serde_json::to_string(&json).map_err(|e| SessionError::IoError {
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
