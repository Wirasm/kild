use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A command sent to `pi --mode rpc` over stdin (one JSON object per line).
///
/// Minimal surface for now — pi exposes many more (`steer`, `follow_up`,
/// `set_model`, `compact`, `fork`, …). Add variants as slices need them; don't add
/// them speculatively.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RpcCommand {
    /// Send a user prompt. pi accepts it, then streams events asynchronously.
    Prompt { message: String },
    /// Abort the current agent run.
    Abort,
    /// Query current session state (model, streaming flag, …). Arrives as a [`PiOutput::Response`].
    GetState,
    /// Request token / cost / context-window stats. Arrives as a [`PiOutput::Response`].
    GetSessionStats,
}

/// Anything `pi` writes to stdout: streaming agent events *and* command responses,
/// distinguished by the `type` tag.
///
/// Unmodeled event types fall through to [`PiOutput::Unknown`] rather than
/// erroring, so a pi upgrade that adds events never breaks the client.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PiOutput {
    AgentStart,
    TurnStart,
    TurnEnd,
    MessageStart,
    /// Streaming update of the assistant's reply.
    MessageUpdate {
        #[serde(rename = "assistantMessageEvent")]
        assistant_message_event: AssistantDelta,
    },
    MessageEnd,
    /// A tool began executing. `tool_call_id` correlates start/end (pi can run
    /// tools in parallel, so name alone is ambiguous).
    ToolExecutionStart {
        #[serde(rename = "toolCallId", default)]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(default)]
        args: Value,
    },
    ToolExecutionUpdate,
    /// A tool finished executing.
    ToolExecutionEnd {
        #[serde(rename = "toolCallId", default)]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(rename = "isError", default)]
        is_error: bool,
    },
    QueueUpdate,
    CompactionStart,
    CompactionEnd,
    /// pi is retrying after a transient provider error (overloaded, rate limit, 5xx).
    AutoRetryStart {
        #[serde(default)]
        attempt: u32,
        #[serde(rename = "maxAttempts", default)]
        max_attempts: u32,
    },
    AutoRetryEnd,
    /// The agent finished this run.
    AgentEnd,
    /// A response to a command (e.g. `get_session_stats`).
    Response {
        command: String,
        #[serde(default)]
        success: bool,
        #[serde(default)]
        data: Value,
        #[serde(default)]
        error: Option<String>,
    },
    ExtensionError,
    /// Any event type we don't model — ignored, but never an error.
    #[serde(other)]
    Unknown,
}

/// The nested `assistantMessageEvent` inside a `message_update` — a streaming delta
/// of the assistant's reply (text, thinking, or tool-call arguments).
#[derive(Debug, Clone, Deserialize)]
pub struct AssistantDelta {
    #[serde(rename = "type")]
    pub kind: DeltaKind,
    /// The incremental chunk, present on `*_delta` kinds.
    #[serde(default)]
    pub delta: Option<String>,
}

/// Kind of streaming delta inside an `assistantMessageEvent`. Unmodeled kinds
/// (`text_start`, `done`, …) fall through to [`DeltaKind::Other`] so a pi change
/// can't break parsing — and modeled kinds are compared at the type level instead
/// of via magic strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaKind {
    TextDelta,
    ThinkingDelta,
    ToolcallDelta,
    #[serde(other)]
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> PiOutput {
        serde_json::from_str(line).expect("valid PiOutput")
    }

    #[test]
    fn parses_text_delta_message_update() {
        let ev = parse(
            r#"{"type":"message_update","message":{},"assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"Hello"}}"#,
        );
        match ev {
            PiOutput::MessageUpdate {
                assistant_message_event,
            } => {
                assert_eq!(assistant_message_event.kind, DeltaKind::TextDelta);
                assert_eq!(assistant_message_event.delta.as_deref(), Some("Hello"));
            }
            other => panic!("expected MessageUpdate, got {other:?}"),
        }
    }

    #[test]
    fn unknown_delta_kind_falls_through_to_other() {
        let ev = parse(r#"{"type":"message_update","assistantMessageEvent":{"type":"text_start"}}"#);
        match ev {
            PiOutput::MessageUpdate {
                assistant_message_event,
            } => assert_eq!(assistant_message_event.kind, DeltaKind::Other),
            other => panic!("expected MessageUpdate, got {other:?}"),
        }
    }

    #[test]
    fn parses_tool_execution_camelcase_renames() {
        match parse(
            r#"{"type":"tool_execution_start","toolCallId":"call_1","toolName":"bash","args":{"command":"ls"}}"#,
        ) {
            PiOutput::ToolExecutionStart {
                tool_call_id,
                tool_name,
                ..
            } => {
                assert_eq!(tool_call_id, "call_1");
                assert_eq!(tool_name, "bash");
            }
            other => panic!("expected ToolExecutionStart, got {other:?}"),
        }
        match parse(
            r#"{"type":"tool_execution_end","toolCallId":"call_1","toolName":"bash","isError":true,"result":{}}"#,
        ) {
            PiOutput::ToolExecutionEnd {
                tool_call_id,
                is_error,
                ..
            } => {
                assert_eq!(tool_call_id, "call_1");
                assert!(is_error);
            }
            other => panic!("expected ToolExecutionEnd, got {other:?}"),
        }
    }

    #[test]
    fn parses_auto_retry_max_attempts_rename() {
        match parse(
            r#"{"type":"auto_retry_start","attempt":1,"maxAttempts":3,"delayMs":2000,"errorMessage":"overloaded"}"#,
        ) {
            PiOutput::AutoRetryStart {
                attempt,
                max_attempts,
            } => {
                assert_eq!(attempt, 1);
                assert_eq!(max_attempts, 3);
            }
            other => panic!("expected AutoRetryStart, got {other:?}"),
        }
    }

    #[test]
    fn response_parses_and_unknown_event_never_errors() {
        assert!(matches!(
            parse(r#"{"type":"response","command":"get_session_stats","success":true,"data":{"cost":0.01}}"#),
            PiOutput::Response { .. }
        ));
        // A future / unrecognized event type must route to Unknown, not fail.
        assert!(matches!(
            parse(r#"{"type":"brand_new_event_from_a_pi_upgrade","foo":1}"#),
            PiOutput::Unknown
        ));
    }

    #[test]
    fn commands_serialize_to_pi_wire_format() {
        assert_eq!(
            serde_json::to_string(&RpcCommand::Prompt {
                message: "hi".into()
            })
            .unwrap(),
            r#"{"type":"prompt","message":"hi"}"#
        );
        assert_eq!(
            serde_json::to_string(&RpcCommand::GetState).unwrap(),
            r#"{"type":"get_state"}"#
        );
        assert_eq!(
            serde_json::to_string(&RpcCommand::GetSessionStats).unwrap(),
            r#"{"type":"get_session_stats"}"#
        );
        assert_eq!(
            serde_json::to_string(&RpcCommand::Abort).unwrap(),
            r#"{"type":"abort"}"#
        );
    }
}
