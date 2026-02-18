use std::sync::Arc;

use base64::Engine;
use tokio::io::BufReader;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::errors::DaemonError;
use crate::protocol::codec::{read_message, write_message_flush};
use crate::protocol::pane_backend::{
    CaptureParams, ContextMap, InitializeParams, KillParams, PaneBackendEvent, PaneBackendRequest,
    PaneBackendResponse, SpawnAgentParams, WriteParams,
};
use crate::session::manager::SessionManager;

/// Handle a pane backend connection using the `CustomPaneBackend` JSON-RPC protocol.
///
/// The first line has already been read by `route_connection` and is passed in as
/// `first_line`. It must be an `initialize` request; anything else is rejected.
///
/// This handler runs for the lifetime of the connection, processing requests and
/// pushing `context_output` / `context_exited` events for all child sessions.
pub async fn handle_pane_backend_connection(
    first_line: String,
    mut reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    write_half: tokio::net::unix::OwnedWriteHalf,
    session_manager: Arc<RwLock<SessionManager>>,
    shutdown: CancellationToken,
) {
    // Parse the first line — must be `initialize`.
    let init_req: PaneBackendRequest = match serde_json::from_str(first_line.trim()) {
        Ok(r) => r,
        Err(e) => {
            warn!(
                event = "daemon.pane_backend.parse_failed",
                error = %e,
                "Failed to parse pane backend initialize request",
            );
            return;
        }
    };

    if init_req.method != "initialize" {
        warn!(
            event = "daemon.pane_backend.wrong_first_method",
            method = %init_req.method,
            "Expected initialize as first pane backend message",
        );
        return;
    }

    let params: InitializeParams = match init_req.parse_params() {
        Ok(p) => p,
        Err(e) => {
            warn!(
                event = "daemon.pane_backend.invalid_params",
                method = "initialize",
                error = %e,
            );
            return;
        }
    };

    if params.protocol_version != "1" {
        warn!(
            event = "daemon.pane_backend.unsupported_version",
            version = %params.protocol_version,
            "Unsupported pane backend protocol version",
        );
        return;
    }

    let leader_id = params.session_hint.clone().unwrap_or_default();

    let mut ctx_map = ContextMap::new();
    if let Some(hint) = &params.session_hint {
        ctx_map.register_leader(hint);
    }

    let init_id = init_req.id.clone().unwrap_or(serde_json::Value::Null);
    let init_response = PaneBackendResponse::ok(
        init_id,
        serde_json::json!({
            "protocol_version": "1",
            "capabilities": ["events", "capture"],
            "self_context_id": "ctx_0",
        }),
    );

    let writer = Arc::new(Mutex::new(write_half));

    {
        let mut w = writer.lock().await;
        if let Err(e) = write_message_flush(&mut *w, &init_response).await {
            warn!(
                event = "daemon.pane_backend.init_write_failed",
                error = %e,
            );
            return;
        }
    }

    info!(
        event = "daemon.pane_backend.connection_accepted",
        leader_id = %leader_id,
    );

    // Main request/response loop.
    loop {
        tokio::select! {
            result = read_message::<_, PaneBackendRequest>(&mut reader) => {
                match result {
                    Ok(Some(req)) => {
                        let response = dispatch_request(
                            req,
                            &mut ctx_map,
                            &leader_id,
                            &session_manager,
                            &writer,
                        ).await;

                        let mut w = writer.lock().await;
                        if let Err(e) = write_message_flush(&mut *w, &response).await {
                            debug!(
                                event = "daemon.pane_backend.write_failed",
                                error = %e,
                            );
                            break;
                        }
                    }
                    Ok(None) => {
                        debug!(event = "daemon.pane_backend.connection_closed");
                        break;
                    }
                    Err(e) => {
                        warn!(
                            event = "daemon.pane_backend.read_error",
                            error = %e,
                        );
                        break;
                    }
                }
            }
            _ = shutdown.cancelled() => {
                debug!(event = "daemon.pane_backend.shutdown");
                break;
            }
        }
    }
}

/// Dispatch a single pane backend request and return the response.
async fn dispatch_request(
    req: PaneBackendRequest,
    ctx_map: &mut ContextMap,
    leader_id: &str,
    session_manager: &Arc<RwLock<SessionManager>>,
    writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
) -> PaneBackendResponse {
    let id = req.id.clone().unwrap_or(serde_json::Value::Null);

    match req.method.as_str() {
        "spawn_agent" => {
            let params: SpawnAgentParams = match req.parse_params() {
                Ok(p) => p,
                Err(e) => {
                    return PaneBackendResponse::err(id, -32602, format!("invalid params: {}", e));
                }
            };

            if params.command.is_empty() {
                return PaneBackendResponse::err(
                    id,
                    -32602,
                    "spawn_agent: command must be non-empty".to_string(),
                );
            }

            // Use ctx_map.next_id to derive the child session ID. This index
            // matches the ctx_id that `allocate` will assign.
            let ctx_index = ctx_map.next_id;
            let child_sid = format!("{}_ctx_{}", leader_id, ctx_index);

            let cwd = params.cwd.as_deref().unwrap_or("/tmp");
            let cmd = &params.command[0];
            let args: Vec<String> = params.command[1..].to_vec();
            let env_pairs: Vec<(String, String)> = params.env.into_iter().collect();

            {
                let mut mgr = session_manager.write().await;
                match mgr.create_session(
                    &child_sid,
                    cwd,
                    cmd,
                    &args,
                    &env_pairs,
                    24,
                    220,
                    false,
                    Some(leader_id),
                ) {
                    Ok(_) => {}
                    Err(e) => {
                        return PaneBackendResponse::err(id, -32603, e.to_string());
                    }
                }
            }

            // Subscribe passively (does not increment client count).
            let rx_opt = session_manager
                .read()
                .await
                .subscribe_output_passive(&child_sid);

            let ctx_id = ctx_map.allocate(&child_sid);

            if let Some(mut rx) = rx_opt {
                let writer_clone = Arc::clone(writer);
                let ctx_id_clone = ctx_id.clone();
                let mgr_clone = Arc::clone(session_manager);
                let child_sid_clone = child_sid.clone();

                tokio::spawn(async move {
                    let engine = base64::engine::general_purpose::STANDARD;
                    loop {
                        match rx.recv().await {
                            Ok(data) => {
                                let encoded = engine.encode(&data);
                                let event =
                                    PaneBackendEvent::context_output(&ctx_id_clone, &encoded);
                                let mut w = writer_clone.lock().await;
                                if write_message_flush(&mut *w, &event).await.is_err() {
                                    break;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                let code = get_exit_code(&mgr_clone, &child_sid_clone).await;
                                let event = PaneBackendEvent::context_exited(&ctx_id_clone, code);
                                let mut w = writer_clone.lock().await;
                                let _ = write_message_flush(&mut *w, &event).await;
                                break;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                // Slow consumer — drop lagged frames and continue
                            }
                        }
                    }
                });
            } else {
                // Session exited before we could subscribe — push context_exited immediately.
                let code = get_exit_code(session_manager, &child_sid).await;
                let event = PaneBackendEvent::context_exited(&ctx_id, code);
                let mut w = writer.lock().await;
                let _ = write_message_flush(&mut *w, &event).await;
            }

            info!(
                event = "daemon.pane_backend.spawn_agent_completed",
                ctx_id = %ctx_id,
                child_session = %child_sid,
            );

            PaneBackendResponse::ok(id, serde_json::json!({ "context_id": ctx_id }))
        }

        "write" => {
            let params: WriteParams = match req.parse_params() {
                Ok(p) => p,
                Err(e) => {
                    return PaneBackendResponse::err(id, -32602, format!("invalid params: {}", e));
                }
            };

            let session_id = match ctx_map.session_for(&params.context_id) {
                Some(s) => s.to_string(),
                None => {
                    return PaneBackendResponse::err(
                        id,
                        -32602,
                        format!("unknown context_id: {}", params.context_id),
                    );
                }
            };

            let decoded = match base64::engine::general_purpose::STANDARD.decode(&params.data) {
                Ok(d) => d,
                Err(e) => {
                    return PaneBackendResponse::err(
                        id,
                        -32602,
                        format!("base64 decode error: {}", e),
                    );
                }
            };

            let mgr = session_manager.read().await;
            match mgr.write_stdin(&session_id, &decoded) {
                Ok(()) => PaneBackendResponse::ok(id, serde_json::json!({})),
                Err(e) => PaneBackendResponse::err(id, -32603, e.to_string()),
            }
        }

        "capture" => {
            let params: CaptureParams = match req.parse_params() {
                Ok(p) => p,
                Err(e) => {
                    return PaneBackendResponse::err(id, -32602, format!("invalid params: {}", e));
                }
            };

            let session_id = match ctx_map.session_for(&params.context_id) {
                Some(s) => s.to_string(),
                None => {
                    return PaneBackendResponse::err(
                        id,
                        -32602,
                        format!("unknown context_id: {}", params.context_id),
                    );
                }
            };

            let scrollback = {
                let mgr = session_manager.read().await;
                mgr.scrollback_contents(&session_id).unwrap_or_default()
            };

            let data = if let Some(line_count) = params.lines {
                let text = String::from_utf8_lossy(&scrollback);
                let lines: Vec<&str> = text.lines().collect();
                let tail_start = lines.len().saturating_sub(line_count);
                lines[tail_start..].join("\n").into_bytes()
            } else {
                scrollback
            };

            let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
            PaneBackendResponse::ok(id, serde_json::json!({ "data": encoded }))
        }

        "kill" => {
            let params: KillParams = match req.parse_params() {
                Ok(p) => p,
                Err(e) => {
                    return PaneBackendResponse::err(id, -32602, format!("invalid params: {}", e));
                }
            };

            let session_id = match ctx_map.remove_ctx(&params.context_id) {
                Some(s) => s,
                None => {
                    return PaneBackendResponse::err(
                        id,
                        -32602,
                        format!("unknown context_id: {}", params.context_id),
                    );
                }
            };

            let mut mgr = session_manager.write().await;
            match mgr.stop_session(&session_id) {
                Ok(()) => PaneBackendResponse::ok(id, serde_json::json!({})),
                Err(DaemonError::SessionNotFound(_)) => {
                    // Already stopped — treat as success
                    PaneBackendResponse::ok(id, serde_json::json!({}))
                }
                Err(e) => PaneBackendResponse::err(id, -32603, e.to_string()),
            }
        }

        "list" => {
            let ctx_ids = ctx_map.all_ctx_ids();
            PaneBackendResponse::ok(id, serde_json::json!({ "contexts": ctx_ids }))
        }

        other => {
            warn!(event = "daemon.pane_backend.unknown_method", method = other,);
            PaneBackendResponse::err(id, -32601, format!("method not found: {}", other))
        }
    }
}

/// Get the exit code for a session, returning -1 if unavailable.
async fn get_exit_code(session_manager: &Arc<RwLock<SessionManager>>, session_id: &str) -> i32 {
    session_manager
        .read()
        .await
        .get_session(session_id)
        .and_then(|s| s.exit_code)
        .unwrap_or(-1)
}
