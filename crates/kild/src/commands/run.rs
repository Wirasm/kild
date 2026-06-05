//! `kild run …` — drive one agent task to completion and print the result.
//!
//! The aggregated result goes to stdout (plain text, or `--json`); live tool
//! progress and the model/stats line go to stderr, so a script or skill can parse
//! stdout without filtering chatter.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use kild_core::project;
use kild_core::rpc::{run_to_completion, RunOutcome, RunProgress, SpawnOptions};

#[derive(Args)]
pub struct RunArgs {
    /// The prompt to send the agent (all positional words are joined).
    #[arg(required = true, num_args = 1..)]
    prompt: Vec<String>,
    /// Registered project to run in. Defaults to the current directory.
    #[arg(long)]
    project: Option<String>,
    /// Agent (named system prompt) to spawn. Defaults to pi's own prompt.
    #[arg(long)]
    agent: Option<String>,
    /// Model pattern (e.g. `claude-opus-4-8`). Defaults to pi's configured model.
    #[arg(long)]
    model: Option<String>,
}

pub async fn handle(args: RunArgs, json: bool) -> Result<()> {
    // Working directory: a registered project's path, or the current dir (`None`
    // lets pi inherit our cwd).
    let cwd: Option<PathBuf> = match &args.project {
        Some(name) => Some(
            project::find_project(name)?
                .with_context(|| format!("no such project: {name}"))?
                .path,
        ),
        None => None,
    };

    // Agent prompt file for `--append-system-prompt` (`None` = pi's default).
    let append_system_prompt = match &args.agent {
        Some(name) => kild_core::agent::resolve_prompt(name, cwd.as_deref())?,
        None => None,
    };

    let opts = SpawnOptions {
        cwd,
        model: args.model,
        append_system_prompt,
        ..Default::default()
    };

    let outcome = run_to_completion(opts, args.prompt.join(" "), |p| log_progress(&p)).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&outcome)?);
    } else {
        print_human(&outcome);
    }
    Ok(())
}

/// Dimmed live feedback on stderr while the agent works.
fn log_progress(progress: &RunProgress) {
    match progress {
        RunProgress::ToolStarted { name } => eprintln!("\x1b[2m🔧 {name}…\x1b[0m"),
        RunProgress::ToolEnded { name, ok } => {
            let mark = if *ok {
                "\x1b[32m✓\x1b[0m"
            } else {
                "\x1b[31m✗\x1b[0m"
            };
            eprintln!("\x1b[2m   {name}\x1b[0m {mark}");
        }
        RunProgress::Retry { attempt, max } => {
            eprintln!("\x1b[33m⟳ retry {attempt}/{max}\x1b[0m");
        }
    }
}

/// Human result: the answer on stdout, model + stats on stderr.
fn print_human(outcome: &RunOutcome) {
    if let Some(model) = &outcome.model {
        eprintln!("\x1b[2m⟶ {model}\x1b[0m");
    }
    println!("{}", outcome.text);
    let tokens = outcome
        .tokens
        .map(|t| t.to_string())
        .unwrap_or_else(|| "?".into());
    let cost = outcome.cost.unwrap_or(0.0);
    let context = outcome
        .context_pct
        .map(|c| format!("{c:.0}%"))
        .unwrap_or_else(|| "n/a".into());
    eprintln!("\x1b[2m───── tokens={tokens} cost=${cost:.4} context={context}\x1b[0m");
}
