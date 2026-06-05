use serde::Serialize;

use super::rpc_client::{PiRpcSession, SpawnOptions};
use super::rpc_errors::RpcError;
use super::rpc_types::{DeltaKind, PiOutput, RpcCommand};

/// The result of driving one agent run to completion (`agent_end` + final stats).
///
/// Pure kild domain data — no `pi` wire shapes leak out, so this is safe to print,
/// serialize, or hand to another slice. This is the boundary translation the
/// architecture asks of every `pi` consumer.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RunOutcome {
    /// Resolved model as `provider / id` (`None` if `pi` never reported it).
    pub model: Option<String>,
    /// The assistant's full reply, concatenated from streamed text deltas.
    pub text: String,
    /// Tools the agent invoked, in execution order.
    pub tools: Vec<ToolRun>,
    /// Total tokens for the run (from `get_session_stats`).
    pub tokens: Option<u64>,
    /// Run cost in USD.
    pub cost: Option<f64>,
    /// Context-window usage, as a percentage.
    pub context_pct: Option<f64>,
}

/// One tool invocation within a run.
#[derive(Debug, Clone, Serialize)]
pub struct ToolRun {
    pub name: String,
    pub ok: bool,
}

/// A live progress signal during a run, for callers that want feedback while the
/// agent works. Translated to kild types — callers never see [`PiOutput`].
#[derive(Debug, Clone)]
pub enum RunProgress {
    ToolStarted { name: String },
    ToolEnded { name: String, ok: bool },
    Retry { attempt: u32, max: u32 },
}

/// Spawn a `pi` session, send one prompt, and drive it to completion, returning
/// the aggregated [`RunOutcome`]. `on_progress` fires for tool/retry events as they
/// happen (use it for live feedback; pass `|_| {}` to ignore).
///
/// This is the one-shot counterpart to [`PiRpcSession::split`] (which the UI uses
/// for interactive, multi-turn streaming). The whole event loop lives here so
/// callers — the CLI, a future daemon — stay thin.
pub async fn run_to_completion(
    opts: SpawnOptions,
    prompt: String,
    mut on_progress: impl FnMut(RunProgress),
) -> Result<RunOutcome, RpcError> {
    let mut session = PiRpcSession::spawn(opts)?;
    session.send(&RpcCommand::GetState).await?;
    session
        .send(&RpcCommand::Prompt { message: prompt })
        .await?;

    let mut outcome = RunOutcome::default();
    while let Some(event) = session.next_event().await {
        match event {
            PiOutput::MessageUpdate {
                assistant_message_event,
            } if assistant_message_event.kind == DeltaKind::TextDelta => {
                if let Some(delta) = assistant_message_event.delta {
                    outcome.text.push_str(&delta);
                }
            }
            PiOutput::ToolExecutionStart { tool_name, .. } => {
                on_progress(RunProgress::ToolStarted { name: tool_name });
            }
            PiOutput::ToolExecutionEnd {
                tool_name,
                is_error,
                ..
            } => {
                outcome.tools.push(ToolRun {
                    name: tool_name.clone(),
                    ok: !is_error,
                });
                on_progress(RunProgress::ToolEnded {
                    name: tool_name,
                    ok: !is_error,
                });
            }
            PiOutput::AutoRetryStart {
                attempt,
                max_attempts,
            } => on_progress(RunProgress::Retry {
                attempt,
                max: max_attempts,
            }),
            PiOutput::Response { command, data, .. } if command == "get_state" => {
                let model = &data["model"];
                if let (Some(provider), Some(id)) =
                    (model["provider"].as_str(), model["id"].as_str())
                {
                    outcome.model = Some(format!("{provider} / {id}"));
                }
            }
            // The run is done; ask for the final token/cost/context numbers, which
            // arrive as the `get_session_stats` response below.
            PiOutput::AgentEnd => {
                session.send(&RpcCommand::GetSessionStats).await?;
            }
            PiOutput::Response { command, data, .. } if command == "get_session_stats" => {
                outcome.tokens = data["tokens"]["total"].as_u64();
                outcome.cost = data["cost"].as_f64();
                outcome.context_pct = data["contextUsage"]["percent"].as_f64();
                break;
            }
            _ => {}
        }
    }

    session.shutdown().await?;
    Ok(outcome)
}
