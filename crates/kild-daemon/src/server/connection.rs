use std::sync::Arc;

use base64::Engine;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use kild_core::errors::KildError;

use crate::protocol::codec::{read_message, write_message};
use crate::protocol::messages::{ClientMessage, DaemonMessage, ErrorCode};
use crate::session::manager::SessionManager;
use crate::session::state::ClientId;

/// Handle a single client connection.
///
/// Reads JSONL messages from the client, dispatches them to the session manager,
/// and sends responses back. For `attach` requests, enters streaming mode.
pub async fn handle_connection(
    stream: UnixStream,
    session_manager: Arc<Mutex<SessionManager>>,
    shutdown: tokio_util::sync::CancellationToken,
) {
    let client_id = {
        let mut mgr = session_manager.lock().await;
        mgr.next_client_id()
    };

    debug!(event = "daemon.connection.accepted", client_id = client_id,);

    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let writer = Arc::new(Mutex::new(writer));

    loop {
        tokio::select! {
            result = read_message::<_, ClientMessage>(&mut reader) => {
                match result {
                    Ok(Some(msg)) => {
                        let response = dispatch_message(
                            msg,
                            client_id,
                            &session_manager,
                            writer.clone(),
                            &shutdown,
                        ).await;

                        if let Some(response) = response {
                            let mut w = writer.lock().await;
                            if let Err(e) = write_message(&mut *w, &response).await {
                                error!(
                                    event = "daemon.connection.write_failed",
                                    client_id = client_id,
                                    error = %e,
                                );
                                break;
                            }
                        }
                    }
                    Ok(None) => {
                        debug!(
                            event = "daemon.connection.closed",
                            client_id = client_id,
                        );
                        break;
                    }
                    Err(e) => {
                        warn!(
                            event = "daemon.connection.read_error",
                            client_id = client_id,
                            error = %e,
                        );
                        break;
                    }
                }
            }
            _ = shutdown.cancelled() => {
                debug!(
                    event = "daemon.connection.shutdown",
                    client_id = client_id,
                );
                break;
            }
        }
    }

    // Clean up: detach client from all sessions
    let mut mgr = session_manager.lock().await;
    mgr.detach_client_from_all(client_id);
}

/// Dispatch a client message to the session manager and return a response.
///
/// Returns `None` for messages that don't generate a direct response (handled inline).
async fn dispatch_message(
    msg: ClientMessage,
    client_id: ClientId,
    session_manager: &Arc<Mutex<SessionManager>>,
    writer: Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    shutdown: &tokio_util::sync::CancellationToken,
) -> Option<DaemonMessage> {
    let msg_id = msg.id().to_string();
    match msg {
        ClientMessage::CreateSession {
            id,
            session_id,
            working_directory,
            command,
            args,
            env_vars,
            rows,
            cols,
            use_login_shell,
        } => {
            let mut mgr = session_manager.lock().await;
            let env_pairs: Vec<(String, String)> = env_vars.into_iter().collect();

            match mgr.create_session(
                &session_id,
                &working_directory,
                &command,
                &args,
                &env_pairs,
                rows,
                cols,
                use_login_shell,
            ) {
                Ok(session_info) => Some(DaemonMessage::SessionCreated {
                    id,
                    session: session_info,
                }),
                Err(e) => Some(DaemonMessage::Error {
                    id,
                    code: ErrorCode::from_code(e.error_code()),
                    message: e.to_string(),
                }),
            }
        }

        ClientMessage::Attach {
            id,
            session_id,
            rows,
            cols,
        } => {
            let (rx, scrollback, resize_failed) = {
                let mut mgr = session_manager.lock().await;

                // Resize to client dimensions
                let resize_failed = if let Err(e) = mgr.resize_pty(&session_id, rows, cols) {
                    warn!(
                        event = "daemon.connection.resize_failed",
                        session_id = %session_id,
                        rows = rows,
                        cols = cols,
                        error = %e,
                    );
                    true
                } else {
                    false
                };

                // Subscribe to broadcast BEFORE capturing scrollback to avoid
                // losing output produced between capture and stream start.
                let rx = match mgr.attach_client(&session_id, client_id) {
                    Ok(rx) => rx,
                    Err(e) => {
                        return Some(DaemonMessage::Error {
                            id,
                            code: ErrorCode::from_code(e.error_code()),
                            message: e.to_string(),
                        });
                    }
                };

                let scrollback = match mgr.scrollback_contents(&session_id) {
                    Some(data) => data,
                    None => {
                        warn!(
                            event = "daemon.connection.scrollback_not_found",
                            session_id = %session_id,
                            "Session not found during scrollback fetch",
                        );
                        Vec::new()
                    }
                };

                (rx, scrollback, resize_failed)
            };

            // Hold the writer lock for ack + scrollback + buffered drain so
            // the streaming task cannot interleave before replay is complete.
            {
                let mut w = writer.lock().await;

                // Send ack
                if let Err(e) = write_message(&mut *w, &DaemonMessage::Ack { id }).await {
                    warn!(
                        event = "daemon.connection.ack_write_failed",
                        session_id = %session_id,
                        client_id = client_id,
                        error = %e,
                    );
                    return None;
                }

                // Notify client if resize failed (non-fatal)
                if resize_failed {
                    let resize_warning = DaemonMessage::SessionEvent {
                        event: "resize_failed".to_string(),
                        session_id: session_id.clone(),
                        details: Some(serde_json::json!({
                            "message": "Terminal resize failed. Display may be garbled. Try detaching and reattaching."
                        })),
                    };
                    if let Err(e) = write_message(&mut *w, &resize_warning).await {
                        warn!(
                            event = "daemon.connection.resize_warning_write_failed",
                            session_id = %session_id,
                            client_id = client_id,
                            error = %e,
                        );
                    }
                }

                // Send scrollback replay so attaching client has context
                if !scrollback.is_empty() {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(&scrollback);
                    let scrollback_msg = DaemonMessage::PtyOutput {
                        session_id: session_id.clone(),
                        data: encoded,
                    };
                    if let Err(e) = write_message(&mut *w, &scrollback_msg).await {
                        warn!(
                            event = "daemon.connection.scrollback_write_failed",
                            session_id = %session_id,
                            client_id = client_id,
                            error = %e,
                        );
                    }
                }
            }
            // Writer lock released — streaming task can now write freely.

            // Spawn streaming task for PTY output.
            // Any output that arrived between subscribe and now is buffered in
            // the broadcast receiver and will be drained by the streaming loop.
            let writer_clone = writer.clone();
            let session_id_clone = session_id.clone();
            let shutdown_clone = shutdown.clone();

            tokio::spawn(async move {
                stream_pty_output(rx, &session_id_clone, writer_clone, shutdown_clone).await;
            });

            None // Response already sent
        }

        ClientMessage::Detach { id, session_id } => {
            let mut mgr = session_manager.lock().await;
            match mgr.detach_client(&session_id, client_id) {
                Ok(()) => Some(DaemonMessage::Ack { id }),
                Err(e) => Some(DaemonMessage::Error {
                    id,
                    code: ErrorCode::from_code(e.error_code()),
                    message: e.to_string(),
                }),
            }
        }

        ClientMessage::ResizePty {
            id,
            session_id,
            rows,
            cols,
        } => {
            let mut mgr = session_manager.lock().await;
            match mgr.resize_pty(&session_id, rows, cols) {
                Ok(()) => Some(DaemonMessage::Ack { id }),
                Err(e) => Some(DaemonMessage::Error {
                    id,
                    code: ErrorCode::from_code(e.error_code()),
                    message: e.to_string(),
                }),
            }
        }

        ClientMessage::WriteStdin {
            id,
            session_id,
            data,
        } => {
            let decoded = match base64::engine::general_purpose::STANDARD.decode(&data) {
                Ok(d) => d,
                Err(e) => {
                    return Some(DaemonMessage::Error {
                        id,
                        code: ErrorCode::Base64DecodeError,
                        message: e.to_string(),
                    });
                }
            };

            let mgr = session_manager.lock().await;
            match mgr.write_stdin(&session_id, &decoded) {
                Ok(()) => Some(DaemonMessage::Ack { id }),
                Err(e) => Some(DaemonMessage::Error {
                    id,
                    code: ErrorCode::from_code(e.error_code()),
                    message: e.to_string(),
                }),
            }
        }

        ClientMessage::StopSession { id, session_id } => {
            let mut mgr = session_manager.lock().await;
            match mgr.stop_session(&session_id) {
                Ok(()) => Some(DaemonMessage::Ack { id }),
                Err(e) => Some(DaemonMessage::Error {
                    id,
                    code: ErrorCode::from_code(e.error_code()),
                    message: e.to_string(),
                }),
            }
        }

        ClientMessage::DestroySession {
            id,
            session_id,
            force,
        } => {
            let mut mgr = session_manager.lock().await;
            match mgr.destroy_session(&session_id, force) {
                Ok(()) => Some(DaemonMessage::Ack { id }),
                Err(e) => Some(DaemonMessage::Error {
                    id,
                    code: ErrorCode::from_code(e.error_code()),
                    message: e.to_string(),
                }),
            }
        }

        ClientMessage::ListSessions { id, project_id: _ } => {
            let mgr = session_manager.lock().await;
            let sessions = mgr.list_sessions();
            Some(DaemonMessage::SessionList { id, sessions })
        }

        ClientMessage::GetSession { id, session_id } => {
            let mgr = session_manager.lock().await;
            match mgr.get_session(&session_id) {
                Some(session) => Some(DaemonMessage::SessionInfo { id, session }),
                None => Some(DaemonMessage::Error {
                    id,
                    code: ErrorCode::SessionNotFound,
                    message: format!("No session found with id '{}'", session_id),
                }),
            }
        }

        ClientMessage::ReadScrollback { id, session_id } => {
            info!(
                event = "daemon.connection.read_scrollback",
                session_id = %session_id
            );
            let mgr = session_manager.lock().await;
            match mgr.scrollback_contents(&session_id) {
                Some(data) => {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
                    Some(DaemonMessage::ScrollbackContents { id, data: encoded })
                }
                None => Some(DaemonMessage::Error {
                    id,
                    code: ErrorCode::SessionNotFound,
                    message: format!("No session found with id '{}'", session_id),
                }),
            }
        }

        ClientMessage::DaemonStop { id } => {
            info!(
                event = "daemon.server.stop_requested",
                client_id = client_id
            );
            shutdown.cancel();
            Some(DaemonMessage::Ack { id })
        }

        ClientMessage::Ping { id } => Some(DaemonMessage::Ack { id }),

        other => {
            warn!(
                event = "daemon.connection.unhandled_message",
                client_id = client_id,
                message = ?other,
                "Received unknown client message variant"
            );
            Some(DaemonMessage::Error {
                id: msg_id,
                code: ErrorCode::ProtocolError,
                message: "Unknown message type. The daemon may need to be updated.".to_string(),
            })
        }
    }
}

/// Stream PTY output to a client until detach, shutdown, or channel close.
async fn stream_pty_output(
    mut rx: tokio::sync::broadcast::Receiver<Vec<u8>>,
    session_id: &str,
    writer: Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    shutdown: tokio_util::sync::CancellationToken,
) {
    let engine = base64::engine::general_purpose::STANDARD;

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(data) => {
                        let encoded = engine.encode(&data);
                        let msg = DaemonMessage::PtyOutput {
                            session_id: session_id.into(),
                            data: encoded,
                        };
                        let mut w = writer.lock().await;
                        if let Err(e) = write_message(&mut *w, &msg).await {
                            debug!(
                                event = "daemon.connection.stream_write_failed",
                                session_id = session_id,
                                error = %e,
                            );
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        let msg = DaemonMessage::PtyOutputDropped {
                            session_id: session_id.into(),
                            bytes_dropped: n as usize,
                        };
                        let mut w = writer.lock().await;
                        if let Err(e) = write_message(&mut *w, &msg).await {
                            error!(
                                event = "daemon.connection.lag_notification_failed",
                                session_id = session_id,
                                bytes_dropped = n,
                                error = %e,
                            );
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        debug!(
                            event = "daemon.connection.stream_closed",
                            session_id = session_id,
                        );
                        break;
                    }
                }
            }
            _ = shutdown.cancelled() => {
                break;
            }
        }
    }
}
