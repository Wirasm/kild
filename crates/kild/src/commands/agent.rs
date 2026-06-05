//! `kild agent …` — list and inspect the named system prompts agents run with.

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Subcommand;
use kild_core::agent;

#[derive(Subcommand)]
pub enum AgentAction {
    /// List agents available to a project (built-in `default` + convention dirs).
    Ls {
        /// Project directory to also scan for project-local agents.
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Print an agent's resolved system prompt.
    Show {
        /// Agent name.
        name: String,
        /// Project directory to also scan for project-local agents.
        #[arg(long)]
        project: Option<PathBuf>,
    },
}

pub fn handle(action: AgentAction, json: bool) -> Result<()> {
    match action {
        AgentAction::Ls { project } => {
            let agents = agent::list_agents(project.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&agents)?);
            } else {
                for a in &agents {
                    println!("{}", a.name);
                }
            }
        }
        AgentAction::Show { name, project } => {
            let agents = agent::list_agents(project.as_deref())?;
            let Some(found) = agents.into_iter().find(|a| a.name == name) else {
                bail!("no such agent: {name}");
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&found)?);
            } else if found.system_prompt.is_empty() {
                eprintln!("(agent `{}` uses pi's default prompt)", found.name);
            } else {
                println!("{}", found.system_prompt);
            }
        }
    }
    Ok(())
}
