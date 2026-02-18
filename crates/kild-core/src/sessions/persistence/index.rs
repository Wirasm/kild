//! Branch-to-session-id index for O(1) lookups by branch name.
//!
//! Stored as `branch_index.json` in the sessions directory.
//! Format: JSON object mapping branch names to session IDs.
//! Best-effort: on read errors, returns empty map (triggers full scan fallback).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::session_files::cleanup_temp_file;

pub(super) const INDEX_FILE: &str = "branch_index.json";
const INDEX_TMP_FILE: &str = "branch_index.json.tmp";

pub(super) fn load_branch_index(sessions_dir: &Path) -> HashMap<String, String> {
    let index_file = sessions_dir.join(INDEX_FILE);
    let content = match fs::read_to_string(&index_file) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return HashMap::new(),
        Err(e) => {
            tracing::warn!(
                event = "core.session.branch_index_read_failed",
                error = %e,
            );
            return HashMap::new();
        }
    };
    match serde_json::from_str(&content) {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!(
                event = "core.session.branch_index_parse_failed",
                error = %e,
            );
            HashMap::new()
        }
    }
}

/// Upsert branch → session_id in the index. Best-effort: warns on failure.
pub(super) fn update_branch_index(sessions_dir: &Path, branch: &str, session_id: &str) {
    let mut index = load_branch_index(sessions_dir);
    index.insert(branch.to_string(), session_id.to_string());
    write_branch_index(sessions_dir, &index);
}

/// Remove any index entry whose value (session_id) matches. Best-effort.
///
/// Used when only the session_id is known (e.g. cleanup by session_id only).
pub(super) fn purge_session_id_from_branch_index(sessions_dir: &Path, session_id: &str) {
    let mut index = load_branch_index(sessions_dir);
    let original_len = index.len();
    index.retain(|_, v| v.as_str() != session_id);
    if index.len() == original_len {
        return;
    }
    write_branch_index(sessions_dir, &index);
}

/// Lookup session_id for a branch name. Returns None if not in index.
pub(super) fn lookup_branch(sessions_dir: &Path, branch: &str) -> Option<String> {
    load_branch_index(sessions_dir).remove(branch)
}

fn write_branch_index(sessions_dir: &Path, index: &HashMap<String, String>) {
    let index_file = sessions_dir.join(INDEX_FILE);
    let tmp_file = sessions_dir.join(INDEX_TMP_FILE);
    let content = match serde_json::to_string(index) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                event = "core.session.branch_index_serialize_failed",
                error = %e,
            );
            return;
        }
    };
    if let Err(e) = fs::write(&tmp_file, &content) {
        cleanup_temp_file(&tmp_file, &e);
        tracing::warn!(
            event = "core.session.branch_index_write_failed",
            error = %e,
        );
        return;
    }
    if let Err(e) = fs::rename(&tmp_file, &index_file) {
        cleanup_temp_file(&tmp_file, &e);
        tracing::warn!(
            event = "core.session.branch_index_rename_failed",
            error = %e,
        );
    }
}
