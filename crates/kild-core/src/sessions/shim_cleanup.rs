use kild_paths::KildPaths;
use tracing::{error, info};

/// Destroy all child shim panes for a session and remove the shim directory.
///
/// Reads `~/.kild/shim/<session_id>/panes.json`, destroys every child pane's
/// daemon session (skipping `%0` leader), then removes the shim directory.
/// Best-effort: logs warnings on failure but never blocks the caller.
///
/// # Schema contract
///
/// The JSON parsed here must stay in sync with `PaneRegistry` in
/// `kild-tmux-shim/src/state.rs`. If the registry schema changes, update both.
pub(super) fn cleanup_shim_panes(paths: &KildPaths, session_id: &str) {
    let shim_dir = paths.shim_session_dir(session_id);
    if !shim_dir.exists() {
        return;
    }

    // Destroy any child shim panes that may still be running
    let panes_path = paths.shim_panes_file(session_id);
    match std::fs::read_to_string(&panes_path) {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(registry) => {
                if let Some(panes) = registry.get("panes").and_then(|p| p.as_object()) {
                    for (pane_id, entry) in panes {
                        if pane_id == "%0" {
                            continue; // Skip the parent pane (already destroyed by caller)
                        }
                        if let Some(child_sid) =
                            entry.get("daemon_session_id").and_then(|s| s.as_str())
                        {
                            info!(
                                event = "core.session.destroy_shim_child",
                                pane_id = pane_id,
                                daemon_session_id = child_sid
                            );
                            if let Err(e) =
                                crate::daemon::client::destroy_daemon_session(child_sid, true)
                            {
                                error!(
                                    event = "core.session.destroy_shim_child_failed",
                                    pane_id = pane_id,
                                    daemon_session_id = child_sid,
                                    error = %e,
                                );
                                eprintln!(
                                    "Warning: Failed to destroy agent team PTY {}: {}",
                                    pane_id, e
                                );
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!(
                    event = "core.session.shim_registry_parse_failed",
                    session_id = session_id,
                    path = %panes_path.display(),
                    error = %e,
                );
                eprintln!(
                    "Warning: Could not parse agent team state at {} — child PTYs may be orphaned: {}",
                    panes_path.display(),
                    e
                );
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // No panes.json means no child panes to clean up
        }
        Err(e) => {
            error!(
                event = "core.session.shim_registry_read_failed",
                session_id = session_id,
                path = %panes_path.display(),
                error = %e,
            );
            eprintln!(
                "Warning: Could not read agent team state at {} — child PTYs may be orphaned: {}",
                panes_path.display(),
                e
            );
        }
    }

    // Remove the entire shim state directory
    if let Err(e) = std::fs::remove_dir_all(&shim_dir) {
        error!(
            event = "core.session.shim_cleanup_failed",
            session_id = session_id,
            path = %shim_dir.display(),
            error = %e,
        );
        eprintln!(
            "Warning: Failed to remove agent team state at {}: {}",
            shim_dir.display(),
            e
        );
    } else {
        info!(
            event = "core.session.shim_cleanup_completed",
            session_id = session_id
        );
    }
}
