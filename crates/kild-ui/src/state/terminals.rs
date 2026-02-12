// Allow dead_code — terminal store methods are consumed as multi-terminal views are wired up.
#![allow(dead_code)]

use std::collections::HashMap;

use crate::terminal::TerminalView;

/// Manages terminal view entities keyed by kild session ID.
///
/// Encapsulates the mapping between kild sessions and their terminal views,
/// supporting multi-session attach where each kild gets its own daemon
/// terminal connection. Terminals are lazily attached on first focus
/// and kept alive for instant switching.
///
/// # Invariants
/// - `focused_kild` must reference a key in `terminals` or be None
/// - `daemon_to_kild` values must be a subset of `terminals` keys
pub struct TerminalStore {
    /// Map from kild session ID → terminal view entity.
    terminals: HashMap<String, gpui::Entity<TerminalView>>,
    /// Map from daemon_session_id → kild session ID (for teammate panes).
    daemon_to_kild: HashMap<String, String>,
    /// Currently focused terminal's kild session ID.
    focused_kild: Option<String>,
}

impl Default for TerminalStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalStore {
    pub fn new() -> Self {
        Self {
            terminals: HashMap::new(),
            daemon_to_kild: HashMap::new(),
            focused_kild: None,
        }
    }

    /// Register a terminal view for a kild session.
    ///
    /// Associates the kild session ID with a terminal entity and records
    /// the daemon session mapping for teammate resolution.
    pub fn attach_terminal(
        &mut self,
        kild_id: String,
        daemon_session_id: String,
        entity: gpui::Entity<TerminalView>,
    ) {
        tracing::info!(
            event = "ui.terminals.attach",
            kild_id = %kild_id,
            daemon_session_id = %daemon_session_id,
        );
        self.daemon_to_kild
            .insert(daemon_session_id, kild_id.clone());
        self.terminals.insert(kild_id, entity);
    }

    /// Remove and drop the terminal entity for a kild session.
    pub fn detach_terminal(&mut self, kild_id: &str) {
        tracing::info!(event = "ui.terminals.detach", kild_id = %kild_id);
        self.terminals.remove(kild_id);
        self.daemon_to_kild.retain(|_, v| v != kild_id);
        if self.focused_kild.as_deref() == Some(kild_id) {
            self.focused_kild = None;
        }
    }

    /// Look up the terminal entity for a kild session.
    pub fn get_terminal(&self, kild_id: &str) -> Option<&gpui::Entity<TerminalView>> {
        self.terminals.get(kild_id)
    }

    /// Check if a terminal is already attached for a kild.
    pub fn has_terminal(&self, kild_id: &str) -> bool {
        self.terminals.contains_key(kild_id)
    }

    /// Get the currently focused terminal entity.
    pub fn focused_terminal(&self) -> Option<&gpui::Entity<TerminalView>> {
        self.focused_kild
            .as_deref()
            .and_then(|id| self.terminals.get(id))
    }

    /// Get the focused kild session ID.
    pub fn focused_kild_id(&self) -> Option<&str> {
        self.focused_kild.as_deref()
    }

    /// Set which kild's terminal is focused.
    ///
    /// No-op if the terminal does not exist in the store.
    pub fn set_focus(&mut self, kild_id: &str) {
        if !self.terminals.contains_key(kild_id) {
            tracing::warn!(
                event = "ui.terminals.focus_invalid",
                kild_id = %kild_id,
            );
            return;
        }
        tracing::debug!(event = "ui.terminals.focus_changed", kild_id = %kild_id);
        self.focused_kild = Some(kild_id.to_string());
    }

    /// Clear focus (no terminal focused).
    pub fn clear_focus(&mut self) {
        self.focused_kild = None;
    }

    /// Resolve a daemon session ID to a kild session ID.
    pub fn kild_for_daemon(&self, daemon_session_id: &str) -> Option<&str> {
        self.daemon_to_kild
            .get(daemon_session_id)
            .map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_store_is_empty() {
        let store = TerminalStore::new();
        assert!(store.focused_terminal().is_none());
        assert!(store.focused_kild_id().is_none());
        assert!(!store.has_terminal("test"));
    }

    #[test]
    fn test_has_terminal() {
        let mut store = TerminalStore::new();
        assert!(!store.has_terminal("kild-1"));

        // We can't create real gpui::Entity in unit tests, so test the HashMap logic
        // by testing detach on non-existent key (no panic)
        store.detach_terminal("kild-1");
        assert!(!store.has_terminal("kild-1"));
    }

    #[test]
    fn test_set_focus_rejects_nonexistent_terminal() {
        let mut store = TerminalStore::new();

        // set_focus on non-existent terminal is a no-op
        store.set_focus("kild-1");
        assert!(
            store.focused_kild_id().is_none(),
            "focus should not be set for non-existent terminal"
        );
    }

    #[test]
    fn test_clear_focus() {
        let store = TerminalStore::new();
        assert!(store.focused_kild_id().is_none());
    }

    #[test]
    fn test_detach_on_nonexistent_is_safe() {
        let mut store = TerminalStore::new();
        // Detach on non-existent is safe, no panic
        store.detach_terminal("kild-1");
        assert!(store.focused_kild_id().is_none());
    }
}
