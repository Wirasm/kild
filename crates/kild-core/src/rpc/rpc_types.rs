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
    /// A tool began executing.
    ToolExecutionStart {
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(default)]
        args: Value,
    },
    ToolExecutionUpdate,
    /// A tool finished executing.
    ToolExecutionEnd {
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
    /// Delta kind: `text_delta`, `thinking_delta`, `toolcall_delta`, `text_start`, …
    #[serde(rename = "type")]
    pub kind: String,
    /// The incremental chunk, present on `*_delta` kinds.
    #[serde(default)]
    pub delta: Option<String>,
}
