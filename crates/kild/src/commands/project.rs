use std::io;
use std::path::{Path, PathBuf};

use clap::ArgMatches;
use tracing::{error, info, warn};

use kild_core::projects::{
    Project, ProjectError, ProjectsData, generate_project_id, load_projects, save_projects,
};

use crate::color;

pub(crate) fn handle_project_command(
    matches: &ArgMatches, config: &kild_config::KildConfig
) -> Result<(), Box<dyn std::error::Error>> {
    match matches.subcommand() {
        Some(("add", sub)) => handle_project_add(sub),
        Some(("list", sub)) => handle_project_list(sub),
        Some(("remove", sub)) => handle_project_remove(sub),
        Some(("info", sub)) => handle_project_info(sub),
        Some(("default", sub)) => handle_project_default(sub),
        _ => Err("Unknown project subcommand".into()),
    }
}

/// Surface a load error to the user and return early.
fn check_load_error(data: &ProjectsData) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(ref load_error) = data.load_error {
        eprintln!("{}", color::error(load_error));
        return Err(load_error.clone().into());
    }
    Ok(())
}

fn handle_project_add(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let path_str = matches
        .get_one::<String>("path")
        .ok_or("missing required argument: path")?;
    let name = matches.get_one::<String>("name").cloned();

    info!(event = "cli.projects.add_started", path = path_str.as_str());

    let project = match Project::new(PathBuf::from(path_str), name) {
        Ok(p) => p,
        Err(e) => {
            error!(event = "cli.projects.add_failed", error = %e);
            eprintln!("{}", color::error(&e.to_string()));
            return Err(e.into());
        }
    };

    let mut data = load_projects();
    check_load_error(&data)?;

    if data.projects.iter().any(|p| p.path() == project.path()) {
        let e = ProjectError::AlreadyExists;
        error!(event = "cli.projects.add_failed", error = %e);
        eprintln!("{}", color::error(&e.to_string()));
        return Err(e.into());
    }

    let canonical_path = project.path().to_path_buf();
    let project_name = project.name().to_string();

    // Auto-select first project
    if data.projects.is_empty() {
        data.active = Some(canonical_path.clone());
    }
    data.projects.push(project);

    if let Err(e) = save_projects(&data) {
        error!(event = "cli.projects.add_failed", error = %e);
        eprintln!("{}", color::error(&e.to_string()));
        return Err(e.into());
    }

    println!(
        "{} {}",
        color::bold("Project registered:"),
        color::aurora(&project_name)
    );
    println!(
        "  {} {}",
        color::muted("path:"),
        color::ice(&canonical_path.display().to_string())
    );

    info!(
        event = "cli.projects.add_completed",
        name = project_name.as_str()
    );

    Ok(())
}

fn handle_project_list(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let json_output = matches.get_flag("json");

    info!(
        event = "cli.projects.list_started",
        json_output = json_output
    );

    let data = load_projects();
    check_load_error(&data)?;

    if json_output {
        let projects_json: Vec<serde_json::Value> = data
            .projects
            .iter()
            .map(|p| {
                serde_json::json!({
                    "name": p.name(),
                    "path": p.path().display().to_string(),
                    "id": generate_project_id(p.path()).to_string(),
                })
            })
            .collect();
        let active = data.active.as_ref().map(|a| a.display().to_string());
        let output = serde_json::json!({
            "projects": projects_json,
            "active": active,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if data.projects.is_empty() {
        println!("No projects registered.");
    } else {
        println!("{}", color::bold("Registered projects:"));
        for p in &data.projects {
            let id = generate_project_id(p.path());
            let active_marker = if data.active.as_deref() == Some(p.path()) {
                color::aurora("*")
            } else {
                " ".to_string()
            };
            println!(
                "  {} {}  {}  {}",
                active_marker,
                color::ice(id.as_ref()),
                color::aurora(p.name()),
                color::muted(&p.path().display().to_string()),
            );
        }
    }

    info!(
        event = "cli.projects.list_completed",
        count = data.projects.len()
    );

    Ok(())
}

fn handle_project_remove(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let identifier = matches
        .get_one::<String>("identifier")
        .ok_or("missing required argument: identifier")?;

    info!(
        event = "cli.projects.remove_started",
        identifier = identifier.as_str()
    );

    let mut data = load_projects();
    check_load_error(&data)?;

    let resolved_path = match resolve_identifier(&data, identifier) {
        Some(path) => path,
        None => {
            let msg = format!("Project not found: {}", identifier);
            eprintln!("{}", color::error(&msg));
            return Err(msg.into());
        }
    };

    let original_len = data.projects.len();
    data.projects
        .retain(|p| p.path() != resolved_path.as_path());

    if data.projects.len() == original_len {
        error!(
            event = "cli.projects.remove_failed",
            identifier = identifier.as_str(),
            resolved_path = %resolved_path.display(),
            "Internal invariant violation: resolved path not found during retain"
        );
        let msg = format!(
            "Internal error: project at '{}' disappeared from list",
            resolved_path.display()
        );
        eprintln!("{}", color::error(&msg));
        return Err(ProjectError::NotFound.into());
    }

    // Clear active if removed, select first remaining
    if data.active.as_deref() == Some(resolved_path.as_path()) {
        data.active = data.projects.first().map(|p| p.path().to_path_buf());
    }

    if let Err(e) = save_projects(&data) {
        error!(event = "cli.projects.remove_failed", error = %e);
        eprintln!("{}", color::error(&e.to_string()));
        return Err(e.into());
    }

    println!("{}", color::aurora("Project removed."));
    info!(event = "cli.projects.remove_completed", path = %resolved_path.display());

    Ok(())
}

fn handle_project_info(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let identifier = matches
        .get_one::<String>("identifier")
        .ok_or("missing required argument: identifier")?;

    info!(
        event = "cli.projects.info_started",
        identifier = identifier.as_str()
    );

    let data = load_projects();
    check_load_error(&data)?;

    let resolved_path = match resolve_identifier(&data, identifier) {
        Some(path) => path,
        None => {
            let msg = format!("Project not found: {}", identifier);
            eprintln!("{}", color::error(&msg));
            return Err(msg.into());
        }
    };

    let project = match data
        .projects
        .iter()
        .find(|p| p.path() == resolved_path.as_path())
    {
        Some(p) => p,
        None => {
            error!(
                event = "cli.projects.info_failed",
                identifier = identifier.as_str(),
                resolved_path = %resolved_path.display(),
                "Internal invariant violation: resolved path not in project list"
            );
            return Err(format!(
                "Internal error: project at '{}' disappeared from list",
                resolved_path.display()
            )
            .into());
        }
    };

    let id = generate_project_id(project.path());
    let is_active = data.active.as_deref() == Some(project.path());

    println!("{}", color::aurora(project.name()));
    println!(
        "  {} {}",
        color::muted("path:  "),
        color::ice(&project.path().display().to_string())
    );
    println!("  {} {}", color::muted("id:    "), color::ice(id.as_ref()));
    println!(
        "  {} {}",
        color::muted("active:"),
        if is_active {
            color::aurora("yes")
        } else {
            color::muted("no")
        }
    );

    info!(
        event = "cli.projects.info_completed",
        identifier = identifier.as_str()
    );

    Ok(())
}

fn handle_project_default(matches: &ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    let identifier = matches
        .get_one::<String>("identifier")
        .ok_or("missing required argument: identifier")?;

    info!(
        event = "cli.projects.default_started",
        identifier = identifier.as_str()
    );

    let mut data = load_projects();
    check_load_error(&data)?;

    let resolved_path = match resolve_identifier(&data, identifier) {
        Some(path) => path,
        None => {
            let msg = format!("Project not found: {}", identifier);
            eprintln!("{}", color::error(&msg));
            return Err(msg.into());
        }
    };

    let project_name = data
        .projects
        .iter()
        .find(|p| p.path() == resolved_path.as_path())
        .map(|p| p.name().to_string())
        .unwrap_or_else(|| resolved_path.display().to_string());

    data.active = Some(resolved_path.clone());

    if let Err(e) = save_projects(&data) {
        error!(event = "cli.projects.default_failed", error = %e);
        eprintln!("{}", color::error(&e.to_string()));
        return Err(e.into());
    }

    println!(
        "{} {}",
        color::bold("Default project set to:"),
        color::aurora(&project_name)
    );
    info!(event = "cli.projects.default_completed", path = %resolved_path.display());

    Ok(())
}

/// Resolve a user-supplied identifier (path string or hex project ID) to a canonical PathBuf.
/// Returns None if not found in the projects list.
fn resolve_identifier(data: &ProjectsData, identifier: &str) -> Option<PathBuf> {
    // Try as a filesystem path first
    match Path::new(identifier).canonicalize() {
        Ok(canonical)
            if data
                .projects
                .iter()
                .any(|p| p.path() == canonical.as_path()) =>
        {
            return Some(canonical);
        }
        Ok(_) => {} // path exists but not registered — fall through to ID lookup
        Err(e) if e.kind() == io::ErrorKind::NotFound => {} // expected — fall through
        Err(e) => {
            // Unexpected IO error (e.g. permission denied) — log and still try ID lookup
            warn!(
                event = "cli.projects.resolve_canonicalize_failed",
                identifier = identifier,
                error = %e,
            );
        }
    }
    // Fall back to project ID (hex hash)
    data.projects
        .iter()
        .find(|p| generate_project_id(p.path()).as_ref() == identifier)
        .map(|p| p.path().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kild_core::projects::persistence::test_helpers::{
        PROJECTS_FILE_ENV_LOCK, ProjectsFileEnvGuard,
    };
    use tempfile::TempDir;

    fn make_git_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .expect("git init failed");
        dir
    }

    fn call_add(path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let app = crate::app::build_cli();
        let matches = app
            .try_get_matches_from(vec!["kild", "project", "add", path])
            .unwrap();
        let project_matches = matches.subcommand_matches("project").unwrap();
        let add_matches = project_matches.subcommand_matches("add").unwrap();
        handle_project_add(add_matches)
    }

    fn call_remove(identifier: &str) -> Result<(), Box<dyn std::error::Error>> {
        let app = crate::app::build_cli();
        let matches = app
            .try_get_matches_from(vec!["kild", "project", "remove", identifier])
            .unwrap();
        let project_matches = matches.subcommand_matches("project").unwrap();
        let remove_matches = project_matches.subcommand_matches("remove").unwrap();
        handle_project_remove(remove_matches)
    }

    // ---- resolve_identifier (pure function, no file I/O) ----

    #[test]
    fn test_resolve_identifier_by_hex_id() {
        let repo = make_git_repo();
        let project = Project::new(repo.path().to_path_buf(), None).unwrap();
        let id = generate_project_id(project.path()).to_string();
        let canonical = project.path().to_path_buf();
        let mut data = ProjectsData::default();
        data.projects.push(project);

        let resolved = resolve_identifier(&data, &id);
        assert_eq!(resolved, Some(canonical));
    }

    #[test]
    fn test_resolve_identifier_by_path() {
        let repo = make_git_repo();
        let project = Project::new(repo.path().to_path_buf(), None).unwrap();
        let canonical = project.path().to_path_buf();
        let path_str = canonical.to_str().unwrap().to_string();
        let mut data = ProjectsData::default();
        data.projects.push(project);

        let resolved = resolve_identifier(&data, &path_str);
        assert_eq!(resolved, Some(canonical));
    }

    #[test]
    fn test_resolve_identifier_unknown_returns_none() {
        let data = ProjectsData::default();
        assert!(resolve_identifier(&data, "deadbeef12345678").is_none());
        assert!(resolve_identifier(&data, "/nonexistent/path/xyz").is_none());
    }

    #[test]
    fn test_resolve_identifier_real_path_not_registered_returns_none() {
        let repo = make_git_repo();
        let canonical = repo.path().canonicalize().unwrap();
        let data = ProjectsData::default();
        let resolved = resolve_identifier(&data, canonical.to_str().unwrap());
        assert!(resolved.is_none());
    }

    // ---- handler tests ----

    #[test]
    fn test_handle_project_add_first_project_becomes_active() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let _guard = ProjectsFileEnvGuard::new(&temp_dir.path().join("projects.json"));

        let repo = make_git_repo();
        let canonical = repo.path().canonicalize().unwrap();

        call_add(repo.path().to_str().unwrap()).unwrap();

        let data = load_projects();
        assert_eq!(data.projects.len(), 1);
        assert_eq!(data.active, Some(canonical));
    }

    #[test]
    fn test_handle_project_add_second_project_does_not_change_active() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let _guard = ProjectsFileEnvGuard::new(&temp_dir.path().join("projects.json"));

        let repo1 = make_git_repo();
        let repo2 = make_git_repo();
        let canonical1 = repo1.path().canonicalize().unwrap();

        call_add(repo1.path().to_str().unwrap()).unwrap();
        call_add(repo2.path().to_str().unwrap()).unwrap();

        let data = load_projects();
        assert_eq!(data.projects.len(), 2);
        assert_eq!(data.active, Some(canonical1)); // first project remains active
    }

    #[test]
    fn test_handle_project_add_duplicate_rejected() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let _guard = ProjectsFileEnvGuard::new(&temp_dir.path().join("projects.json"));

        let repo = make_git_repo();

        call_add(repo.path().to_str().unwrap()).unwrap();
        let result = call_add(repo.path().to_str().unwrap());

        assert!(result.is_err());
        let data = load_projects();
        assert_eq!(data.projects.len(), 1);
    }

    #[test]
    fn test_handle_project_add_load_error_surfaces_error() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let corrupted_path = temp_dir.path().join("projects.json");
        std::fs::write(&corrupted_path, "not valid json").unwrap();
        let _guard = ProjectsFileEnvGuard::new(&corrupted_path);

        let repo = make_git_repo();
        let result = call_add(repo.path().to_str().unwrap());

        assert!(result.is_err());
        // Verify the corrupted file was not overwritten
        let content = std::fs::read_to_string(&corrupted_path).unwrap();
        assert_eq!(content, "not valid json");
    }

    #[test]
    fn test_handle_project_remove_active_reassigns_to_first_remaining() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let _guard = ProjectsFileEnvGuard::new(&temp_dir.path().join("projects.json"));

        let repo1 = make_git_repo();
        let repo2 = make_git_repo();
        let canonical1 = repo1.path().canonicalize().unwrap();
        let canonical2 = repo2.path().canonicalize().unwrap();

        call_add(repo1.path().to_str().unwrap()).unwrap();
        call_add(repo2.path().to_str().unwrap()).unwrap();

        let data = load_projects();
        assert_eq!(data.active, Some(canonical1.clone()));

        call_remove(canonical1.to_str().unwrap()).unwrap();

        let data = load_projects();
        assert_eq!(data.projects.len(), 1);
        assert_eq!(data.active, Some(canonical2));
    }

    #[test]
    fn test_handle_project_remove_last_project_clears_active() {
        let _lock = PROJECTS_FILE_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let _guard = ProjectsFileEnvGuard::new(&temp_dir.path().join("projects.json"));

        let repo = make_git_repo();
        let canonical = repo.path().canonicalize().unwrap();

        call_add(repo.path().to_str().unwrap()).unwrap();
        call_remove(canonical.to_str().unwrap()).unwrap();

        let data = load_projects();
        assert!(data.projects.is_empty());
        assert!(data.active.is_none());
    }
}
