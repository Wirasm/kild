use std::sync::Arc;

use kild_core::rpc::{PiOutput, PiRpcSession, PiRpcWriter, RpcCommand, SpawnOptions};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

/// Active session's write handle. The event pump owns the read half.
#[derive(Default)]
struct AppState {
    writer: Arc<Mutex<Option<PiRpcWriter>>>,
}

/// Frontend-facing event — a translated, serializable view of `PiOutput`.
/// pi's wire shapes never reach the UI (the boundary rule lives here).
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum UiEvent {
    Model { provider: String, id: String },
    Text { delta: String },
    ToolStart { name: String, args: String },
    ToolEnd { name: String, ok: bool },
    Retry { attempt: u32, max: u32 },
    AgentEnd,
    Stats {
        tokens: u64,
        cost: f64,
        context_pct: Option<f64>,
    },
    SessionEnd,
}

/// Spawn a fresh `pi --mode rpc` session and start pumping its events to the UI.
#[tauri::command]
async fn spawn_session(
    app: AppHandle,
    state: State<'_, AppState>,
    model: Option<String>,
) -> Result<(), String> {
    let writer_arc = state.writer.clone();

    let session = PiRpcSession::spawn(SpawnOptions {
        model,
        ..Default::default()
    })
    .map_err(|e| e.to_string())?;
    let (writer, mut events) = session.split();

    {
        // Replace any previous session (dropping its writer closes that pi).
        let mut guard = writer_arc.lock().await;
        *guard = Some(writer);
        if let Some(w) = guard.as_mut() {
            let _ = w.send(&RpcCommand::GetState).await; // report resolved model
        }
    }

    let pump_writer = writer_arc.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(ev) = events.recv().await {
            if let Some(ui) = translate(&ev) {
                let _ = app.emit("pi-event", ui);
            }
            if matches!(ev, PiOutput::AgentEnd) {
                if let Some(w) = pump_writer.lock().await.as_mut() {
                    let _ = w.send(&RpcCommand::GetSessionStats).await;
                }
            }
        }
        let _ = app.emit("pi-event", UiEvent::SessionEnd);
    });

    Ok(())
}

/// Send a user prompt to the active session.
#[tauri::command]
async fn send_prompt(state: State<'_, AppState>, text: String) -> Result<(), String> {
    let writer_arc = state.writer.clone();
    let mut guard = writer_arc.lock().await;
    let writer = guard.as_mut().ok_or("no active session")?;
    writer
        .send(&RpcCommand::Prompt { message: text })
        .await
        .map_err(|e| e.to_string())
}

fn translate(ev: &PiOutput) -> Option<UiEvent> {
    match ev {
        PiOutput::MessageUpdate {
            assistant_message_event,
        } if assistant_message_event.kind == "text_delta" => assistant_message_event
            .delta
            .clone()
            .map(|delta| UiEvent::Text { delta }),
        PiOutput::ToolExecutionStart { tool_name, args } => Some(UiEvent::ToolStart {
            name: tool_name.clone(),
            args: args.to_string(),
        }),
        PiOutput::ToolExecutionEnd {
            tool_name,
            is_error,
        } => Some(UiEvent::ToolEnd {
            name: tool_name.clone(),
            ok: !is_error,
        }),
        PiOutput::AutoRetryStart {
            attempt,
            max_attempts,
        } => Some(UiEvent::Retry {
            attempt: *attempt,
            max: *max_attempts,
        }),
        PiOutput::AgentEnd => Some(UiEvent::AgentEnd),
        PiOutput::Response { command, data, .. } if command == "get_state" => {
            let model = &data["model"];
            Some(UiEvent::Model {
                provider: model["provider"].as_str().unwrap_or("?").to_string(),
                id: model["id"].as_str().unwrap_or("?").to_string(),
            })
        }
        PiOutput::Response { command, data, .. } if command == "get_session_stats" => {
            Some(UiEvent::Stats {
                tokens: data["tokens"]["total"].as_u64().unwrap_or(0),
                cost: data["cost"].as_f64().unwrap_or(0.0),
                context_pct: data["contextUsage"]["percent"].as_f64(),
            })
        }
        _ => None,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![spawn_session, send_prompt])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
