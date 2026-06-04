use std::fs;
use std::path::{Path, PathBuf};

use super::agent_errors::AgentError;
use super::agent_types::Agent;

/// The built-in agent that runs pi's own default prompt.
pub const DEFAULT_NAME: &str = "default";

/// Convention directories scanned for agent definitions, highest priority first:
/// the project's own dirs, then global. A `<name>.md` file is an agent named
/// `<name>` (its body — frontmatter stripped — is the system prompt).
fn agent_dirs(project_root: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(root) = project_root {
        dirs.push(root.join(".kild/agents"));
        dirs.push(root.join(".claude/agents"));
        dirs.push(root.join(".pi/agents"));
    }
    if let Some(dir) = crate::paths::agents_dir() {
        dirs.push(dir); // ~/.config/kild/agents
    }
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".claude/agents"));
    }
    dirs
}

/// List agents: the built-in `default` (pi's own prompt) followed by every
/// `<name>.md` found across the convention dirs, deduplicated by name (first
/// directory wins, so a project agent shadows a global one of the same name).
pub fn list_agents(project_root: Option<&Path>) -> Result<Vec<Agent>, AgentError> {
    let mut agents = vec![Agent {
        name: DEFAULT_NAME.to_string(),
        system_prompt: String::new(),
    }];
    let mut seen: Vec<String> = vec![DEFAULT_NAME.to_string()];

    for dir in agent_dirs(project_root) {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(AgentError::Io(e)),
        };
        let mut found: Vec<Agent> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "md") {
                    let name = path.file_stem()?.to_string_lossy().into_owned();
                    if seen.contains(&name) {
                        return None;
                    }
                    let system_prompt = strip_frontmatter(&fs::read_to_string(&path).ok()?);
                    Some(Agent {
                        name,
                        system_prompt,
                    })
                } else {
                    None
                }
            })
            .collect();
        found.sort_by(|a, b| a.name.cmp(&b.name));
        for agent in found {
            seen.push(agent.name.clone());
            agents.push(agent);
        }
    }
    Ok(agents)
}

/// Resolve a selected agent to a prompt file ready for `--append-system-prompt`.
///
/// Finds `<name>.md` across the convention dirs, strips any frontmatter, and
/// writes the body to `<kild_home>/prompts/<name>.md` (so the YAML never leaks
/// into the prompt). Returns `None` for `default` or an unknown agent — pi then
/// runs its own prompt.
pub fn resolve_prompt(
    name: &str,
    project_root: Option<&Path>,
) -> Result<Option<PathBuf>, AgentError> {
    if name == DEFAULT_NAME {
        return Ok(None);
    }
    let Some(src) = find_agent_file(name, project_root) else {
        return Ok(None);
    };
    let body = strip_frontmatter(&fs::read_to_string(&src)?);

    let dir = crate::paths::prompts_dir().ok_or(AgentError::NoHome)?;
    fs::create_dir_all(&dir)?;
    let out = dir.join(format!("{name}.md"));
    fs::write(&out, body)?;
    Ok(Some(out))
}

fn find_agent_file(name: &str, project_root: Option<&Path>) -> Option<PathBuf> {
    agent_dirs(project_root)
        .into_iter()
        .map(|dir| dir.join(format!("{name}.md")))
        .find(|path| path.is_file())
}

/// Return the markdown body with a leading YAML frontmatter block (if any)
/// removed. Files without frontmatter are returned unchanged.
fn strip_frontmatter(content: &str) -> String {
    let normalized = content.replace("\r\n", "\n");
    if let Some(rest) = normalized.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            return rest[end + 5..].trim().to_string();
        }
        if let Some(end) = rest.find("\n---") {
            return rest[end + 4..].trim().to_string();
        }
    }
    content.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_frontmatter_keeps_body() {
        let body = strip_frontmatter("---\nname: planner\ndescription: plans\n---\nYou are a planner.\n");
        assert_eq!(body, "You are a planner.");
    }

    #[test]
    fn passes_through_when_no_frontmatter() {
        assert_eq!(strip_frontmatter("You are a planner."), "You are a planner.");
    }

    #[test]
    fn resolve_is_none_for_default() {
        assert!(resolve_prompt(DEFAULT_NAME, None).unwrap().is_none());
    }
}
