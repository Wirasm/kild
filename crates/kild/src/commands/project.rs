//! `kild project …` — register and list the directories agents work in.

use anyhow::Result;
use clap::Subcommand;
use kild_core::project;

#[derive(Subcommand)]
pub enum ProjectAction {
    /// List registered projects.
    Ls,
    /// Register a project directory (names are unique; a leading `~` is expanded).
    Add { name: String, path: String },
    /// Remove a project by name.
    Rm { name: String },
}

pub fn handle(action: ProjectAction, json: bool) -> Result<()> {
    match action {
        ProjectAction::Ls => {
            let projects = project::load_projects()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&projects)?);
            } else if projects.is_empty() {
                eprintln!("no projects registered — add one with `kild project add <name> <path>`");
            } else {
                for p in &projects {
                    println!("{}\t{}", p.name, p.path.display());
                }
            }
        }
        ProjectAction::Add { name, path } => {
            let p = project::add_project(name, path)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&p)?);
            } else {
                println!("added {} → {}", p.name, p.path.display());
            }
        }
        ProjectAction::Rm { name } => {
            project::remove_project(&name)?;
            if !json {
                println!("removed {name}");
            }
        }
    }
    Ok(())
}
