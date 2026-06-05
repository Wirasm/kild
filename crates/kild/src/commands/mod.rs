//! CLI surface — clap definitions and the dispatch router.
//!
//! This module (and its submodules) is the presentation layer: it owns argument
//! parsing and output formatting only. Every command delegates to a `kild-core`
//! slice — there is no business logic here.

mod agent;
mod project;
mod run;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// kild — run `pi` coding agents in your projects, scriptably.
#[derive(Parser)]
#[command(
    name = "kild",
    version,
    about = "Run pi coding agents — the CLI is the primary, skill-friendly interface"
)]
pub struct Cli {
    /// Emit machine-readable JSON instead of human text (for scripts and skills).
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage projects (a directory an agent works in).
    Project {
        #[command(subcommand)]
        action: project::ProjectAction,
    },
    /// List or inspect agents (named, reusable system prompts).
    Agent {
        #[command(subcommand)]
        action: agent::AgentAction,
    },
    /// Run a one-shot agent task to completion and print the result.
    Run(run::RunArgs),
}

/// Route a parsed CLI invocation to its handler.
pub async fn dispatch(cli: Cli) -> Result<()> {
    let json = cli.json;
    match cli.command {
        Command::Project { action } => project::handle(action, json),
        Command::Agent { action } => agent::handle(action, json),
        Command::Run(args) => run::handle(args, json).await,
    }
}
