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

    /// Session branch from the KILD_SESSION_BRANCH env var (injected by daemon).
    #[serde(default)]
    pub session_id: Option<String>,

    /// Transcript summary for brain forwarding.
    #[serde(default)]
    pub transcript_summary: Option<String>,

    /// For Stop events: whether the stop_hook_active flag is set.
    #[serde(default)]
    pub stop_hook_active: Option<bool>,
}

/// Response returned to Claude Code.
///
/// For blocking hooks (Stop, SubagentStop), `decision: "block"` prevents the event.
/// For non-blocking hooks, any 2xx response is fine.
#[derive(Debug, Serialize)]
pub struct HookResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
}

/// Result of processing a hook event.
pub struct HookResult {
    /// HTTP response to send back to Claude Code.
    pub response: HookResponse,
    /// Agent status to write (if any).
    pub agent_status: Option<String>,
    /// Branch name for the session.
    pub branch: Option<String>,
    /// Whether to forward this event to the brain.
    pub forward_to_brain: bool,
    /// Event tag for brain forwarding (e.g., "agent.stop").
    pub event_tag: Option<String>,
    /// Whether this idle event should be gated (first-only forwarding).
    pub should_gate: bool,
}

/// Process a hook event payload and determine actions.
///
/// Pure logic — no I/O. Returns what actions the caller should take.
pub fn process_hook(payload: &HookPayload, verbose: bool) -> HookResult {
    let branch = payload.session_id.clone();
    let is_brain = branch.as_deref() == Some("honryu");

    match payload.hook_event_name.as_str() {
        "Stop" => HookResult {
            response: HookResponse { decision: None },
            agent_status: Some("idle".to_string()),
            branch: branch.clone(),
            forward_to_brain: !is_brain && branch.is_some(),
            event_tag: Some("agent.stop".to_string()),
            should_gate: true,
        },
        "SubagentStop" => HookResult {
            response: HookResponse { decision: None },
            agent_status: Some("idle".to_string()),
            branch: branch.clone(),
            forward_to_brain: verbose && !is_brain && branch.is_some(),
            event_tag: Some("subagent.stop".to_string()),
            should_gate: false,
        },
        other => {
            warn!(
                event = "daemon.hooks.unknown_event",
                hook_event_name = other,
            );
            HookResult {
                response: HookResponse { decision: None },
                agent_status: None,
                branch: None,
                forward_to_brain: false,
                event_tag: None,
                should_gate: false,
            }
        }
    }
}

/// Shared state for the HTTP hook handler.
pub struct HookState {
    pub idle_gate: Mutex<IdleGate>,
    pub verbose: bool,
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

    let body_bytes = match req.collect().await {
        Ok(collected) => collected.to_bytes(),
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
    if let (Some(status_str), Some(branch)) = (&result.agent_status, &result.branch) {
        update_agent_status(branch, status_str);
    }

    // Brain forwarding with idle gate
    if result.forward_to_brain
        && let Some(branch) = &result.branch
    {
        let should_forward = if result.should_gate {
            let mut gate = state.idle_gate.lock().await;
            gate.try_arm(branch)
        } else {
            true
        };

        if should_forward
            && let Some(tag) = &result.event_tag
        {
            let summary = payload.transcript_summary.as_deref().unwrap_or("");
            let msg = if summary.is_empty() {
                format!("[EVENT] {} {}", branch, tag)
            } else {
                format!("[EVENT] {} {}: {}", branch, tag, summary)
            };
            forward_to_brain(branch, &msg);
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
fn update_agent_status(branch: &str, status_str: &str) {
    let status = match status_str {
        "idle" => kild_core::AgentStatus::Idle,
        "waiting" => kild_core::AgentStatus::Waiting,
        "working" => kild_core::AgentStatus::Working,
        other => {
            warn!(event = "daemon.hooks.unknown_status", status = other);
            return;
        }
    };

    match kild_core::session_ops::update_agent_status(branch, status, true) {
        Ok(_) => {
            info!(
                event = "daemon.hooks.agent_status_updated",
                branch = branch,
                status = status_str,
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
///
/// Exposed for other daemon modules to call when tasks are written.
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
