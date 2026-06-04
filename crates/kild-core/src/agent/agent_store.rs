use std::fs;
use std::path::PathBuf;

use super::agent_errors::AgentError;
use super::agent_types::Agent;

/// The built-in agent that runs pi's own default prompt.
pub const DEFAULT_NAME: &str = "default";

fn agents_dir() -> Result<PathBuf, AgentError> {
    crate::paths::agents_dir().ok_or(AgentError::NoHome)
}

/// List agents: the built-in `default` (pi's own prompt) followed by any authored
/// `<kild_home>/agents/*.md` files, sorted by name.
pub fn list_agents() -> Result<Vec<Agent>, AgentError> {
    let mut agents = vec![Agent {
        name: DEFAULT_NAME.to_string(),
        system_prompt: String::new(),
    }];

    match fs::read_dir(agents_dir()?) {
        Ok(entries) => {
            let mut authored: Vec<Agent> = entries
                .flatten()
                .filter_map(|entry| {
                    let path = entry.path();
                    if path.extension().is_some_and(|ext| ext == "md") {
                        Some(Agent {
                            name: path.file_stem()?.to_string_lossy().into_owned(),
                            system_prompt: fs::read_to_string(&path).unwrap_or_default(),
                        })
                    } else {
                        None
                    }
                })
                .collect();
            authored.sort_by(|a, b| a.name.cmp(&b.name));
            agents.extend(authored);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(AgentError::Io(e)),
    }
    Ok(agents)
}

/// The prompt file for a named agent, if it exists and isn't the default. Used to
/// pass `--append-system-prompt` at spawn; `None` runs pi's default prompt.
pub fn prompt_file(name: &str) -> Option<PathBuf> {
    if name == DEFAULT_NAME {
        return None;
    }
    let path = crate::paths::agents_dir()?.join(format!("{name}.md"));
    path.is_file().then_some(path)
}

/// Author a new agent (a name + system prompt). Names are unique, non-empty, not
/// `default`, and free of path separators / leading dots.
pub fn add_agent(name: String, system_prompt: String) -> Result<Agent, AgentError> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AgentError::EmptyName);
    }
    if name == DEFAULT_NAME {
        return Err(AgentError::ReservedName);
    }
    if name.contains('/') || name.contains('\\') || name.starts_with('.') {
        return Err(AgentError::InvalidName(name));
    }

    let dir = agents_dir()?;
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{name}.md"));
    if path.exists() {
        return Err(AgentError::DuplicateName(name));
    }
    fs::write(&path, &system_prompt)?;
    Ok(Agent {
        name,
        system_prompt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // These all fail validation before any filesystem/env access.
    #[test]
    fn add_agent_rejects_bad_names() {
        assert!(matches!(
            add_agent(String::new(), "p".into()),
            Err(AgentError::EmptyName)
        ));
        assert!(matches!(
            add_agent("   ".into(), "p".into()),
            Err(AgentError::EmptyName)
        ));
        assert!(matches!(
            add_agent("default".into(), "p".into()),
            Err(AgentError::ReservedName)
        ));
        assert!(matches!(
            add_agent("a/b".into(), "p".into()),
            Err(AgentError::InvalidName(_))
        ));
        assert!(matches!(
            add_agent(".hidden".into(), "p".into()),
            Err(AgentError::InvalidName(_))
        ));
    }

    #[test]
    fn prompt_file_is_none_for_default() {
        assert!(prompt_file(DEFAULT_NAME).is_none());
    }
}
