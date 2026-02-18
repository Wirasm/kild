use std::collections::HashMap;

/// Maps `ctx_id ↔ daemon_session_id` for a single pane backend connection.
///
/// `ctx_0` is reserved for the leader session (pre-registered via `register_leader`
/// on `initialize`). Subsequent calls to `allocate` assign `ctx_1`, `ctx_2`, etc.
pub struct ContextMap {
    pub(crate) next_id: u32,
    ctx_to_session: HashMap<String, String>,
    session_to_ctx: HashMap<String, String>,
}

impl Default for ContextMap {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextMap {
    pub fn new() -> Self {
        Self {
            next_id: 0,
            ctx_to_session: HashMap::new(),
            session_to_ctx: HashMap::new(),
        }
    }

    /// Pre-register the leader session as `ctx_0`.
    ///
    /// Should be called on `initialize` when a `session_hint` is present.
    /// Sets `next_id` to 1 so subsequent `allocate` calls start at `ctx_1`.
    pub fn register_leader(&mut self, session_id: &str) {
        let ctx_id = "ctx_0".to_string();
        self.next_id = 1;
        self.ctx_to_session
            .insert(ctx_id.clone(), session_id.to_string());
        self.session_to_ctx.insert(session_id.to_string(), ctx_id);
    }

    /// Allocate a new context ID for a child session. Returns the new `ctx_id`.
    pub fn allocate(&mut self, session_id: &str) -> String {
        let ctx_id = format!("ctx_{}", self.next_id);
        self.next_id += 1;
        self.ctx_to_session
            .insert(ctx_id.clone(), session_id.to_string());
        self.session_to_ctx
            .insert(session_id.to_string(), ctx_id.clone());
        ctx_id
    }

    /// Look up the daemon session ID for a context ID.
    pub fn session_for(&self, ctx_id: &str) -> Option<&str> {
        self.ctx_to_session.get(ctx_id).map(|s| s.as_str())
    }

    /// Look up the context ID for a daemon session ID.
    pub fn ctx_for_session(&self, session_id: &str) -> Option<&str> {
        self.session_to_ctx.get(session_id).map(|s| s.as_str())
    }

    /// Remove a context by ctx_id. Returns the daemon session ID if found.
    pub fn remove_ctx(&mut self, ctx_id: &str) -> Option<String> {
        if let Some(session_id) = self.ctx_to_session.remove(ctx_id) {
            self.session_to_ctx.remove(&session_id);
            Some(session_id)
        } else {
            None
        }
    }

    /// List all context IDs currently registered.
    pub fn all_ctx_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.ctx_to_session.keys().cloned().collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_map_allocates_sequentially() {
        let mut ctx = ContextMap::new();
        let id0 = ctx.allocate("session_0");
        let id1 = ctx.allocate("session_1");
        let id2 = ctx.allocate("session_2");
        assert_eq!(id0, "ctx_0");
        assert_eq!(id1, "ctx_1");
        assert_eq!(id2, "ctx_2");
    }

    #[test]
    fn test_context_map_register_leader_starts_at_ctx_0() {
        let mut ctx = ContextMap::new();
        ctx.register_leader("leader_session");

        // Leader is ctx_0
        assert_eq!(ctx.session_for("ctx_0"), Some("leader_session"));
        assert_eq!(ctx.ctx_for_session("leader_session"), Some("ctx_0"));

        // Subsequent allocates start at ctx_1
        let id = ctx.allocate("child_session");
        assert_eq!(id, "ctx_1");
    }

    #[test]
    fn test_context_map_reverse_lookup_roundtrips() {
        let mut ctx = ContextMap::new();
        ctx.register_leader("leader_sid");
        let child_id = ctx.allocate("child_sid");

        assert_eq!(ctx.ctx_for_session("leader_sid"), Some("ctx_0"));
        assert_eq!(ctx.session_for("ctx_0"), Some("leader_sid"));

        assert_eq!(ctx.ctx_for_session("child_sid"), Some(child_id.as_str()));
        assert_eq!(ctx.session_for(&child_id), Some("child_sid"));
    }

    #[test]
    fn test_context_map_remove_ctx() {
        let mut ctx = ContextMap::new();
        ctx.register_leader("leader_sid");
        let child_id = ctx.allocate("child_sid");

        let removed = ctx.remove_ctx(&child_id);
        assert_eq!(removed.as_deref(), Some("child_sid"));
        assert!(ctx.session_for(&child_id).is_none());
        assert!(ctx.ctx_for_session("child_sid").is_none());

        // Leader still present
        assert!(ctx.session_for("ctx_0").is_some());
    }

    #[test]
    fn test_context_map_all_ctx_ids_sorted() {
        let mut ctx = ContextMap::new();
        ctx.register_leader("leader");
        ctx.allocate("child_a");
        ctx.allocate("child_b");

        let ids = ctx.all_ctx_ids();
        assert_eq!(ids, vec!["ctx_0", "ctx_1", "ctx_2"]);
    }
}
