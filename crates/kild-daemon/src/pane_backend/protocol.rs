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
    /// Capabilities advertised by Claude Code (e.g. `["events"]`).
    ///
    /// Deserialized but not yet acted upon. Future handlers can gate behaviour
    /// on whether a capability is present before sending unsolicited events.
    #[serde(default)]
    pub capabilities: Vec<String>,
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
        assert!(params.capabilities.is_empty());
    }

    #[test]
    fn test_deserialize_initialize_with_capabilities() {
        let json = r#"{"id":"1","method":"initialize","params":{"protocol_version":"1","capabilities":["events"]}}"#;
        let req: PaneBackendRequest = serde_json::from_str(json).unwrap();
        let params: InitializeParams = req.parse_params().unwrap();
        assert_eq!(params.capabilities, vec!["events"]);
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
}
