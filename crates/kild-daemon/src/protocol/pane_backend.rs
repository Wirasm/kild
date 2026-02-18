use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Envelope for all inbound pane backend requests.
///
/// All inbound messages share this structure. The `method` field distinguishes
/// the request type; `params` is method-specific and parsed separately.
#[derive(Debug, Deserialize)]
pub struct PaneBackendRequest {
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

impl PaneBackendRequest {
    /// Parse the `params` field into a concrete type.
    pub fn parse_params<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.params.clone())
    }
}

/// Parameters for the `initialize` method.
#[derive(Debug, Deserialize)]
pub struct InitializeParams {
    pub protocol_version: String,
    /// Optional hint identifying which daemon session is the leader.
    ///
    /// KILD injects `CLAUDE_PANE_BACKEND_SESSION_ID` into the daemon PTY env.
    /// Claude Code echoes this back so the daemon can correlate the connection
    /// with the correct leader session.
    pub session_hint: Option<String>,
}

/// Parameters for the `spawn_agent` method.
#[derive(Debug, Deserialize)]
pub struct SpawnAgentParams {
    pub command: Vec<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Parameters for the `write` method.
#[derive(Debug, Deserialize)]
pub struct WriteParams {
    pub context_id: String,
    /// Base64-encoded data to write to the context's stdin.
    pub data: String,
}

/// Parameters for the `capture` method.
#[derive(Debug, Deserialize)]
pub struct CaptureParams {
    pub context_id: String,
    /// Number of trailing lines to return. If `None`, returns all scrollback.
    pub lines: Option<usize>,
}

/// Parameters for the `kill` method.
#[derive(Debug, Deserialize)]
pub struct KillParams {
    pub context_id: String,
}

/// Outbound JSON-RPC response.
#[derive(Debug, Serialize)]
pub struct PaneBackendResponse {
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl PaneBackendResponse {
    pub fn ok(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: serde_json::Value, code: i32, message: String) -> Self {
        Self {
            id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        }
    }
}

/// JSON-RPC error object.
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

/// Outbound push event (no `id` field — server-initiated).
#[derive(Debug, Serialize)]
pub struct PaneBackendEvent {
    pub method: &'static str,
    pub params: serde_json::Value,
}

impl PaneBackendEvent {
    pub fn context_exited(context_id: &str, exit_code: i32) -> Self {
        Self {
            method: "context_exited",
            params: serde_json::json!({
                "context_id": context_id,
                "exit_code": exit_code,
            }),
        }
    }

    pub fn context_output(context_id: &str, data_b64: &str) -> Self {
        Self {
            method: "context_output",
            params: serde_json::json!({
                "context_id": context_id,
                "data": data_b64,
            }),
        }
    }
}

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
    fn test_deserialize_initialize_with_session_hint() {
        let json = r#"{"id":"1","method":"initialize","params":{"protocol_version":"1","session_hint":"myapp_feature"}}"#;
        let req: PaneBackendRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "initialize");
        let params: InitializeParams = req.parse_params().unwrap();
        assert_eq!(params.protocol_version, "1");
        assert_eq!(params.session_hint.as_deref(), Some("myapp_feature"));
    }

    #[test]
    fn test_deserialize_initialize_without_session_hint() {
        let json = r#"{"id":"1","method":"initialize","params":{"protocol_version":"1"}}"#;
        let req: PaneBackendRequest = serde_json::from_str(json).unwrap();
        let params: InitializeParams = req.parse_params().unwrap();
        assert_eq!(params.session_hint, None);
    }

    #[test]
    fn test_deserialize_spawn_agent_full() {
        let json = r#"{"id":"2","method":"spawn_agent","params":{"command":["claude","--dangerously-skip-permissions"],"cwd":"/tmp/wt","env":{"FOO":"bar"},"metadata":{"name":"worker-1"}}}"#;
        let req: PaneBackendRequest = serde_json::from_str(json).unwrap();
        let params: SpawnAgentParams = req.parse_params().unwrap();
        assert_eq!(params.command[0], "claude");
        assert_eq!(params.cwd.as_deref(), Some("/tmp/wt"));
        assert_eq!(params.env.get("FOO").map(|s| s.as_str()), Some("bar"));
    }

    #[test]
    fn test_deserialize_spawn_agent_minimal() {
        let json = r#"{"id":"2","method":"spawn_agent","params":{"command":["claude"]}}"#;
        let req: PaneBackendRequest = serde_json::from_str(json).unwrap();
        let params: SpawnAgentParams = req.parse_params().unwrap();
        assert!(params.env.is_empty());
        assert!(params.metadata.is_null());
    }

    #[test]
    fn test_serialize_response_ok_has_no_error_field() {
        let resp = PaneBackendResponse::ok(
            serde_json::json!("1"),
            serde_json::json!({"context_id": "ctx_1"}),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("\"error\""));
        assert!(json.contains("\"result\""));
    }

    #[test]
    fn test_serialize_response_err_has_no_result_field() {
        let resp =
            PaneBackendResponse::err(serde_json::json!("1"), -32602, "invalid params".to_string());
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("\"result\""));
        assert!(json.contains("\"error\""));
    }

    #[test]
    fn test_serialize_event_context_exited_method_value() {
        let event = PaneBackendEvent::context_exited("ctx_1", 0);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"method\":\"context_exited\""));
        assert!(json.contains("\"exit_code\":0"));
    }

    #[test]
    fn test_serialize_event_context_output() {
        let event = PaneBackendEvent::context_output("ctx_2", "aGVsbG8=");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"method\":\"context_output\""));
        assert!(json.contains("aGVsbG8="));
    }

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
