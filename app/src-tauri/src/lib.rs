use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use kild_core::agent::Agent;
use kild_core::project::Project;
use kild_core::rpc::{DeltaKind, PiOutput, PiRpcSession, PiRpcWriter, RpcCommand, SpawnOptions};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

/// Active session's write handle. The event pump owns the read half.
#[derive(Default)]
struct AppState {
    writer: Arc<Mutex<Option<PiRpcWriter>>>,
    /// Bumped on every spawn; a superseded session's pump sees the mismatch and
    /// stops emitting / stops writing into the new session.
    generation: Arc<AtomicU64>,
}

/// Frontend-facing event — a translated, serializable view of `PiOutput`.
/// pi's wire shapes never reach the UI (the boundary rule lives here).
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum UiEvent {
    Model { provider: String, id: String },
    Text { delta: String },
    ToolStart { id: String, name: String, args: String },
    ToolEnd { id: String, name: String, ok: bool },
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
    cwd: Option<String>,
    agent: Option<String>,
) -> Result<(), String> {
    let writer_arc = state.writer.clone();
    let generation = state.generation.clone();
    // Claim a new generation; any prior session's pump is now stale.
    let gen = generation.fetch_add(1, Ordering::SeqCst) + 1;

    let session = PiRpcSession::spawn(SpawnOptions {
        model,
        cwd: cwd.map(std::path::PathBuf::from),
        // A non-default agent's prompt is layered on pi's default; `default`
        // (or unknown) resolves to None -> pi's own prompt.
        append_system_prompt: agent.as_deref().and_then(kild_core::agent::prompt_file),
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
            // A newer session superseded us — stop emitting and stop writing into
            // what is now someone else's session.
            if generation.load(Ordering::SeqCst) != gen {
                return;
            }
            if let Some(ui) = translate(&ev) {
                let _ = app.emit("pi-event", ui);
            }
            if matches!(ev, PiOutput::AgentEnd) {
                let mut guard = pump_writer.lock().await;
                if generation.load(Ordering::SeqCst) == gen {
                    if let Some(w) = guard.as_mut() {
                        let _ = w.send(&RpcCommand::GetSessionStats).await;
                    }
                }
            }
        }
        if generation.load(Ordering::SeqCst) == gen {
            let _ = app.emit("pi-event", UiEvent::SessionEnd);
        }
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

/// List registered projects.
#[tauri::command]
fn list_projects() -> Result<Vec<Project>, String> {
    kild_core::project::load_projects().map_err(|e| e.to_string())
}

/// Register a project (an existing directory). Names are unique.
#[tauri::command]
fn add_project(name: String, path: String) -> Result<Project, String> {
    kild_core::project::add_project(name, path).map_err(|e| e.to_string())
}

/// List agents (built-in `default` + authored prompt files).
#[tauri::command]
fn list_agents() -> Result<Vec<Agent>, String> {
    kild_core::agent::list_agents().map_err(|e| e.to_string())
}

/// Author a new agent (name + system prompt).
#[tauri::command]
fn add_agent(name: String, system_prompt: String) -> Result<Agent, String> {
    kild_core::agent::add_agent(name, system_prompt).map_err(|e| e.to_string())
}

fn translate(ev: &PiOutput) -> Option<UiEvent> {
    match ev {
        PiOutput::MessageUpdate {
            assistant_message_event,
        } if assistant_message_event.kind == DeltaKind::TextDelta => assistant_message_event
            .delta
            .clone()
            .map(|delta| UiEvent::Text { delta }),
        PiOutput::ToolExecutionStart {
            tool_call_id,
            tool_name,
            args,
        } => Some(UiEvent::ToolStart {
            id: tool_call_id.clone(),
            name: tool_name.clone(),
            args: args.to_string(),
        }),
        PiOutput::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            is_error,
        } => Some(UiEvent::ToolEnd {
            id: tool_call_id.clone(),
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
        .invoke_handler(tauri::generate_handler![
            spawn_session,
            send_prompt,
            list_projects,
            add_project,
            list_agents,
            add_agent
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
