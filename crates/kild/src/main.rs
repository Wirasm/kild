//! kild spike — drive a `pi --mode rpc` subprocess and render its structured event
//! stream.
//!
//! This is slice 1: it exercises the whole spine (spawn pi, JSONL transport, typed
//! events) before any worktree/supervisor/UI work. The point is to *prove the
//! structured-event model* — we render streamed text, tool cards, and a final
//! context/cost summary, with no terminal emulation anywhere.
//!
//! Usage: `kild [prompt...]`

use std::io::Write;

use anyhow::Result;
use kild_core::rpc::{DeltaKind, PiOutput, PiRpcSession, RpcCommand, SpawnOptions};

#[tokio::main]
async fn main() -> Result<()> {
    let prompt = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let prompt = if prompt.trim().is_empty() {
        "In one sentence, what files are in the current directory? Use your tools to check."
            .to_string()
    } else {
        prompt
    };

    eprintln!("\x1b[2m⟶ spawning `pi --mode rpc`…\x1b[0m");
    let opts = SpawnOptions {
        provider: std::env::var("KILD_PROVIDER").ok(),
        model: std::env::var("KILD_MODEL").ok(),
        ..Default::default()
    };
    let mut session = PiRpcSession::spawn(opts)?;

    eprintln!("\x1b[2m⟶ prompt: {prompt}\x1b[0m");
    session.send(&RpcCommand::GetState).await?;
    session
        .send(&RpcCommand::Prompt { message: prompt })
        .await?;

    let mut got_text = false;
    while let Some(event) = session.next_event().await {
        match event {
            PiOutput::MessageUpdate {
                assistant_message_event,
            } => {
                if assistant_message_event.kind == DeltaKind::TextDelta {
                    if let Some(delta) = assistant_message_event.delta {
                        print!("{delta}");
                        let _ = std::io::stdout().flush();
                        got_text = true;
                    }
                }
            }
            PiOutput::ToolExecutionStart {
                tool_name, args, ..
            } => {
                println!(
                    "\n\x1b[36m🔧 {tool_name}\x1b[0m \x1b[2m{}\x1b[0m",
                    compact_args(&args)
                );
            }
            PiOutput::ToolExecutionEnd {
                tool_name,
                is_error,
                ..
            } => {
                let mark = if is_error {
                    "\x1b[31m✗\x1b[0m"
                } else {
                    "\x1b[32m✓\x1b[0m"
                };
                println!("\x1b[2m   {tool_name}\x1b[0m {mark}");
            }
            PiOutput::AutoRetryStart {
                attempt,
                max_attempts,
            } => {
                eprintln!("\x1b[33m⟳ pi auto-retry {attempt}/{max_attempts}\x1b[0m");
            }
            PiOutput::CompactionStart => eprintln!("\x1b[2m⟲ compacting context…\x1b[0m"),
            PiOutput::AgentEnd => {
                println!();
                session.send(&RpcCommand::GetSessionStats).await?;
            }
            PiOutput::Response { command, data, .. } if command == "get_state" => {
                let model = &data["model"];
                eprintln!(
                    "\x1b[2m⟶ model: {} / {}\x1b[0m\n",
                    model["provider"].as_str().unwrap_or("?"),
                    model["id"].as_str().unwrap_or("?")
                );
            }
            PiOutput::Response {
                command,
                data,
                error,
                ..
            } if command == "get_session_stats" => {
                print_stats(&data, error.as_deref());
                break;
            }
            _ => {}
        }
    }

    if !got_text {
        eprintln!(
            "\n\x1b[33m(no assistant text — check [pi] stderr above for auth/model errors)\x1b[0m"
        );
    }

    session.shutdown().await?;
    Ok(())
}

/// One-line preview of a tool's JSON arguments.
fn compact_args(args: &serde_json::Value) -> String {
    let full = args.to_string();
    let truncated: String = full.chars().take(80).collect();
    if truncated.len() < full.len() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

/// Render the `get_session_stats` payload: tokens, cost, context-window usage.
fn print_stats(data: &serde_json::Value, error: Option<&str>) {
    if let Some(err) = error {
        eprintln!("\x1b[31mstats error: {err}\x1b[0m");
        return;
    }
    let tokens = data["tokens"]["total"].as_u64().unwrap_or(0);
    let cost = data["cost"].as_f64().unwrap_or(0.0);
    let context = match data["contextUsage"]["percent"].as_f64() {
        Some(percent) => format!("{percent:.0}%"),
        None => "n/a".to_string(),
    };
    println!(
        "\x1b[2m─────\x1b[0m \x1b[1mpi session\x1b[0m  tokens={tokens}  cost=${cost:.4}  context={context}"
    );
}
