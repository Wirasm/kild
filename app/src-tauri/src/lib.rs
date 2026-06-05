use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use kild_core::agent::Agent;
use kild_core::project::Project;
use kild_core::rpc::{DeltaKind, PiOutput, PiRpcSession, PiRpcWriter, RpcCommand, SpawnOptions};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

/// Live sessions, keyed by id. Each entry is a write handle; its read half is
/// owned by a per-session event pump. This is the runtime registry — in-memory,
/// many concurrent pi sessions.
#[derive(Default)]
struct AppState {
    sessions: Arc<Mutex<HashMap<u64, Arc<Mutex<PiRpcWriter>>>>>,
    next_id: Arc<AtomicU64>,
}

/// Frontend-facing event — a translated, serializable view of `PiOutput`, tagged
/// with the session it belongs to so the UI routes it to the right transcript.
/// pi's wire shapes never reach the UI (the boundary rule lives here).
#[derive(Clone, Serialize)]
struct SessionEvent {
    session: u64,
    event: UiEvent,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum UiEvent {
    Model {
        provider: String,
        id: String,
    },
    Text {
        delta: String,
    },
    ToolStart {
        id: String,
        name: String,
        args: String,
    },
    ToolEnd {
        id: String,
        name: String,
        ok: bool,
    },
    Retry {
        attempt: u32,
        max: u32,
    },
    AgentEnd,
    Stats {
        tokens: u64,
        cost: f64,
        context_pct: Option<f64>,
    },
    SessionEnd,
}

/// Spawn a fresh `pi --mode rpc` session, register it, and pump its events to the
/// UI tagged with the new session id (which is returned to the caller).
#[tauri::command]
async fn spawn_session(
    app: AppHandle,
    state: State<'_, AppState>,
    model: Option<String>,
    cwd: Option<String>,
    agent: Option<String>,
) -> Result<u64, String> {
    let sessions = state.sessions.clone();
    let id = state.next_id.fetch_add(1, Ordering::SeqCst) + 1;

    // A non-default agent's prompt is resolved within the project and layered on
    // pi's default. A read/write failure is surfaced rather than silently
    // downgraded; `Ok(None)` (the default or an unknown agent) intentionally falls
    // back to pi's own prompt.
    let append_system_prompt = match agent.as_deref() {
        Some(name) => {
            kild_core::agent::resolve_prompt(name, cwd.as_deref().map(std::path::Path::new))
                .map_err(|e| e.to_string())?
        }
        None => None,
    };
    let session = PiRpcSession::spawn(SpawnOptions {
        model,
        cwd: cwd.map(std::path::PathBuf::from),
        append_system_prompt,
        ..Default::default()
    })
    .map_err(|e| e.to_string())?;
    let (writer, mut events) = session.split();
    let writer = Arc::new(Mutex::new(writer));

    let _ = writer.lock().await.send(&RpcCommand::GetState).await; // report resolved model
    sessions.lock().await.insert(id, writer);

    let pump_sessions = sessions.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(ev) = events.recv().await {
            if let Some(ui) = translate(&ev) {
                let _ = app.emit(
                    "pi-event",
                    SessionEvent {
                        session: id,
                        event: ui,
                    },
                );
            }
            if matches!(ev, PiOutput::AgentEnd) {
                // Clone the handle out of the map, then drop the registry lock
                // before writing to pi — never hold it across a stdin write.
                let writer = pump_sessions.lock().await.get(&id).cloned();
                if let Some(writer) = writer {
                    let _ = writer.lock().await.send(&RpcCommand::GetSessionStats).await;
                }
            }
        }
        // pi exited (clean, crash, or stopped): drop the entry and notify the UI.
        pump_sessions.lock().await.remove(&id);
        let _ = app.emit(
            "pi-event",
            SessionEvent {
                session: id,
                event: UiEvent::SessionEnd,
            },
        );
    });

    Ok(id)
}

/// Send a user prompt to a specific session.
#[tauri::command]
async fn send_prompt(state: State<'_, AppState>, session: u64, text: String) -> Result<(), String> {
    let writer = state.sessions.lock().await.get(&session).cloned();
    let writer = writer.ok_or("no such session")?;
    // Bind the result so the MutexGuard drops before `writer` (tail-expression
    // temporaries would otherwise outlive the local).
    let result = writer
        .lock()
        .await
        .send(&RpcCommand::Prompt { message: text })
        .await;
    result.map_err(|e| e.to_string())
}

/// Stop a session: dropping its writer closes stdin, so pi exits and the pump
/// emits `session_end`.
#[tauri::command]
async fn stop_session(state: State<'_, AppState>, session: u64) -> Result<(), String> {
    state.sessions.lock().await.remove(&session);
    Ok(())
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

/// List agents available to a project: built-in `default` + agents read from the
/// project's `.kild/agents` / `.claude/agents` / `.pi/agents` and the global dirs.
#[tauri::command]
fn list_agents(project: Option<String>) -> Result<Vec<Agent>, String> {
    let root = project.map(std::path::PathBuf::from);
    kild_core::agent::list_agents(root.as_deref()).map_err(|e| e.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use kild_core::rpc::{AssistantDelta, DeltaKind};

    #[test]
    fn tool_end_error_flag_maps_to_ok() {
        assert!(matches!(
            translate(&PiOutput::ToolExecutionEnd {
                tool_call_id: "c1".into(),
                tool_name: "bash".into(),
                is_error: true,
            }),
            Some(UiEvent::ToolEnd { ok: false, .. })
        ));
        assert!(matches!(
            translate(&PiOutput::ToolExecutionEnd {
                tool_call_id: "c1".into(),
                tool_name: "bash".into(),
                is_error: false,
            }),
            Some(UiEvent::ToolEnd { ok: true, .. })
        ));
    }

    #[test]
    fn only_text_delta_updates_become_text() {
        let ui = translate(&PiOutput::MessageUpdate {
            assistant_message_event: AssistantDelta {
                kind: DeltaKind::ThinkingDelta,
                delta: Some("hmm".into()),
            },
        });
        assert!(ui.is_none());
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
            stop_session,
            list_projects,
            add_project,
            list_agents
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
