//! HTTP hook endpoint for Claude Code `type: "http"` hooks.
//!
//! Receives hook event payloads from Claude Code over HTTP, processes them in Rust
//! (agent status updates, brain forwarding, idle deduplication), and returns JSON
//! responses. Replaces the shell script pipeline for events that support HTTP hooks.
//!
//! Events handled via HTTP: Stop, SubagentStop.
//! Events still using command hooks: TeammateIdle, TaskCompleted (exit-code blocking),
//! Notification (HTTP not supported by Claude Code for this event).

pub mod idle_gate;
#[cfg(test)]
mod tests;

use std::net::SocketAddr;
use std::sync::Arc;

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use self::idle_gate::IdleGate;

/// Hook event payload from Claude Code.
///
/// Claude Code POSTs JSON with these fields (and more — we only deserialize what we need).
#[derive(Debug, Deserialize)]
pub struct HookPayload {
    /// The hook event name: "Stop", "SubagentStop", etc.
    pub hook_event_name: String,

    /// KILD session branch name. Claude Code populates this from `KILD_SESSION_BRANCH`
    /// which the daemon injects into the PTY environment. The JSON key is `session_id`
    /// because Claude Code passes environment variables through that field.
    #[serde(default)]
    pub session_id: Option<String>,

    /// Transcript summary for brain forwarding.
    #[serde(default)]
    pub transcript_summary: Option<String>,

    /// For Stop events: whether the stop_hook_active flag is set.
    #[serde(default)]
    pub stop_hook_active: Option<bool>,
}

/// Hook decision for Claude Code's HTTP hook protocol.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HookDecision {
    Allow,
    Block,
}

/// Response returned to Claude Code.
///
/// For blocking hooks (Stop, SubagentStop), `decision: Block` prevents the event.
/// For non-blocking hooks, any 2xx response is fine.
#[derive(Debug, Serialize)]
pub struct HookResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<HookDecision>,
}

/// Brain forwarding action — only present when the event should be forwarded.
pub struct BrainForward {
    /// Branch name for the session (always present when forwarding).
    pub branch: String,
    /// Event tag (e.g., "agent.stop", "subagent.stop").
    pub event_tag: String,
    /// Whether to deduplicate (first occurrence only per task cycle).
    pub gated: bool,
}

/// Result of processing a hook event.
pub struct HookResult {
    /// HTTP response to send back to Claude Code.
    pub response: HookResponse,
    /// Agent status to write (if any).
    pub agent_status: Option<kild_core::AgentStatus>,
    /// Branch name for agent status updates.
    pub branch: Option<String>,
    /// Brain forwarding action. None = don't forward.
    pub forward: Option<BrainForward>,
}

/// Process a hook event payload and determine actions.
///
/// Pure logic — no I/O. Returns what actions the caller should take.
pub fn process_hook(payload: &HookPayload, verbose: bool) -> HookResult {
    let branch = payload.session_id.clone();
    let is_brain = branch.as_deref() == Some("honryu");

    match payload.hook_event_name.as_str() {
        "Stop" => {
            let forward = if !is_brain {
                branch.as_ref().map(|b| BrainForward {
                    branch: b.clone(),
                    event_tag: "agent.stop".to_string(),
                    gated: true,
                })
            } else {
                None
            };
            HookResult {
                response: HookResponse { decision: None },
                agent_status: Some(kild_core::AgentStatus::Idle),
                branch: branch.clone(),
                forward,
            }
        }
        "SubagentStop" => {
            let forward = if verbose && !is_brain {
                branch.as_ref().map(|b| BrainForward {
                    branch: b.clone(),
                    event_tag: "subagent.stop".to_string(),
                    gated: false,
                })
            } else {
                None
            };
            HookResult {
                response: HookResponse { decision: None },
                agent_status: Some(kild_core::AgentStatus::Idle),
                branch: branch.clone(),
                forward,
            }
        }
        other => {
            warn!(
                event = "daemon.hooks.unknown_event",
                hook_event_name = other,
            );
            HookResult {
                response: HookResponse { decision: None },
                agent_status: None,
                branch: None,
                forward: None,
            }
        }
    }
}

/// Shared state for the HTTP hook handler.
pub struct HookState {
    pub(crate) idle_gate: Mutex<IdleGate>,
    pub(crate) verbose: bool,
}

impl Default for HookState {
    fn default() -> Self {
        Self::new()
    }
}

impl HookState {
    pub fn new() -> Self {
        Self {
            idle_gate: Mutex::new(IdleGate::new()),
            verbose: std::env::var("KILD_HOOK_VERBOSE")
                .map(|v| v == "1")
                .unwrap_or(false),
        }
    }
}

/// Handle an HTTP request to the hooks endpoint.
async fn handle_request(
    req: Request<Incoming>,
    state: Arc<HookState>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    // Only accept POST /hooks
    if req.method() != Method::POST || req.uri().path() != "/hooks" {
        return Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from("Not Found")))
            .unwrap());
    }

    // Reject oversized payloads (1 MiB limit — Claude Code hook payloads are small).
    const MAX_BODY_SIZE: u64 = 1024 * 1024;
    if let Some(content_length) = req.headers().get("content-length")
        && let Ok(len) = content_length.to_str().unwrap_or("0").parse::<u64>()
        && len > MAX_BODY_SIZE
    {
        return Ok(Response::builder()
            .status(StatusCode::PAYLOAD_TOO_LARGE)
            .body(Full::new(Bytes::from("Payload Too Large")))
            .unwrap());
    }

    let body_bytes = match req.collect().await {
        Ok(collected) => {
            let bytes = collected.to_bytes();
            if bytes.len() as u64 > MAX_BODY_SIZE {
                return Ok(Response::builder()
                    .status(StatusCode::PAYLOAD_TOO_LARGE)
                    .body(Full::new(Bytes::from("Payload Too Large")))
                    .unwrap());
            }
            bytes
        }
        Err(e) => {
            error!(event = "daemon.hooks.body_read_failed", error = %e);
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::from("Bad Request")))
                .unwrap());
        }
    };

    let payload: HookPayload = match serde_json::from_slice(&body_bytes) {
        Ok(p) => p,
        Err(e) => {
            warn!(event = "daemon.hooks.payload_parse_failed", error = %e);
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::from(format!(
                    "{{\"error\":\"invalid JSON: {}\"}}",
                    e
                ))))
                .unwrap());
        }
    };

    info!(
        event = "daemon.hooks.event_received",
        hook_event_name = %payload.hook_event_name,
        branch = ?payload.session_id,
    );

    let result = process_hook(&payload, state.verbose);

    // Apply agent status update
    if let (Some(status), Some(branch)) = (&result.agent_status, &result.branch) {
        update_agent_status(branch, *status);
    }

    // Brain forwarding with idle gate
    if let Some(fwd) = &result.forward {
        let should_forward = if fwd.gated {
            let mut gate = state.idle_gate.lock().await;
            gate.try_arm(&fwd.branch)
        } else {
            true
        };

        if should_forward {
            let summary = payload.transcript_summary.as_deref().unwrap_or("");
            let msg = if summary.is_empty() {
                format!("[EVENT] {} {}", fwd.branch, fwd.event_tag)
            } else {
                format!("[EVENT] {} {}: {}", fwd.branch, fwd.event_tag, summary)
            };
            forward_to_brain(&fwd.branch, &msg);
        }
    }

    let response_json =
        serde_json::to_string(&result.response).unwrap_or_else(|_| "{}".to_string());

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(response_json)))
        .unwrap())
}

/// Update agent status via kild-core. Best-effort — logs on failure.
fn update_agent_status(branch: &str, status: kild_core::AgentStatus) {
    match kild_core::session_ops::update_agent_status(branch, status, true) {
        Ok(_) => {
            info!(
                event = "daemon.hooks.agent_status_updated",
                branch = branch,
                status = %status,
            );
        }
        Err(e) => {
            warn!(
                event = "daemon.hooks.agent_status_update_failed",
                branch = branch,
                error = %e,
            );
        }
    }
}

/// Forward an event message to the Honryū brain session. Best-effort.
fn forward_to_brain(branch: &str, message: &str) {
    // Check if honryu is running by listing sessions and finding an active honryu session.
    // This runs on the tokio blocking thread pool since kild-core is sync.
    let msg = message.to_string();
    let branch = branch.to_string();
    tokio::task::spawn_blocking(move || {
        match kild_core::session_ops::list_sessions() {
            Ok(sessions) => {
                let honryu_active = sessions.iter().any(|s| {
                    s.branch.as_ref() == "honryu" && s.status == kild_core::SessionStatus::Active
                });

                if !honryu_active {
                    return;
                }

                // Write to Claude Code inbox for the brain
                let safe_name = kild_core::sessions::fleet::fleet_safe_name(&branch);
                if let Err(e) = kild_core::sessions::fleet::write_to_inbox(
                    kild_core::sessions::fleet::BRAIN_BRANCH,
                    &safe_name,
                    &msg,
                ) {
                    warn!(
                        event = "daemon.hooks.brain_forward_failed",
                        branch = %branch,
                        error = %e,
                    );
                } else {
                    info!(
                        event = "daemon.hooks.brain_forward_completed",
                        branch = %branch,
                    );
                }
            }
            Err(e) => {
                warn!(
                    event = "daemon.hooks.session_list_failed",
                    error = %e,
                );
            }
        }
    });
}

/// Clear the idle gate for a branch (called when a new task is injected).
pub async fn clear_idle_gate(state: &HookState, branch: &str) {
    let mut gate = state.idle_gate.lock().await;
    gate.clear(branch);
}

/// Start the HTTP hook listener on the given port.
///
/// Returns `Ok(())` when the shutdown token is cancelled.
pub async fn serve_hooks(
    port: u16,
    state: Arc<HookState>,
    shutdown: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;

    info!(
        event = "daemon.hooks.http_listening",
        addr = %addr,
    );

    loop {
        tokio::select! {
            accept = listener.accept() => {
                match accept {
                    Ok((stream, _addr)) => {
                        let state = state.clone();
                        tokio::spawn(async move {
                            let io = TokioIo::new(stream);
                            let service = service_fn(move |req| {
                                let state = state.clone();
                                handle_request(req, state)
                            });
                            if let Err(e) = http1::Builder::new()
                                .serve_connection(io, service)
                                .await
                            {
                                warn!(
                                    event = "daemon.hooks.connection_error",
                                    error = %e,
                                );
                            }
                        });
                    }
                    Err(e) => {
                        error!(
                            event = "daemon.hooks.accept_failed",
                            error = %e,
                        );
                    }
                }
            }
            _ = shutdown.cancelled() => {
                info!(event = "daemon.hooks.http_shutdown");
                break;
            }
        }
    }

    Ok(())
}
