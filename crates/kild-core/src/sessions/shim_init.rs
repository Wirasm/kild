use kild_paths::KildPaths;

/// Initialize the shim pane registry for a new daemon session.
///
/// Creates `~/.kild/shim/<session_id>/panes.json` with the leader pane
/// registered as `%0`. Idempotent: overwrites existing state.
///
/// # Schema contract
///
/// The JSON written here must stay in sync with `PaneRegistry` in
/// `kild-tmux-shim/src/state.rs`. A parallel `state::init_registry()` exists
/// there but cannot be called from kild-core due to the crate dependency
/// direction. If the registry schema changes, update both.
pub(super) fn init_pane_registry(session_id: &str, daemon_session_id: &str) -> Result<(), String> {
    let paths = KildPaths::resolve().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(paths.shim_session_dir(session_id))
        .map_err(|e| format!("failed to create shim state directory: {}", e))?;

    let initial_state = serde_json::json!({
        "next_pane_id": 1,
        "session_name": "kild_0",
        "panes": {
            "%0": {
                "daemon_session_id": daemon_session_id,
                "title": "",
                "border_style": "",
                "window_id": "0",
                "hidden": false
            }
        },
        "windows": {
            "0": { "name": "main", "pane_ids": ["%0"] }
        },
        "sessions": {
            "kild_0": { "name": "kild_0", "windows": ["0"] }
        }
    });

    std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(paths.shim_lock_file(session_id))
        .map_err(|e| format!("failed to create shim lock file: {}", e))?;

    let json = serde_json::to_string_pretty(&initial_state)
        .map_err(|e| format!("failed to serialize shim state: {}", e))?;
    std::fs::write(paths.shim_panes_file(session_id), json)
        .map_err(|e| format!("failed to write shim state: {}", e))?;

    Ok(())
}
