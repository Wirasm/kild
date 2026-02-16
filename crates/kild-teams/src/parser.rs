//! Parsers for Claude Code team config and shim pane registry.
//!
//! Raw serde types with `#[serde(default)]` for forward compatibility
//! with unknown/added fields.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::errors::TeamsError;
use crate::types::{TeamColor, TeamMember, TeamState};

// =============================================================================
// Claude Code team config: ~/.claude/teams/<team>/config.json
// =============================================================================

/// Raw Claude Code team config (serde).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RawTeamConfig {
    pub members: Vec<RawTeamMember>,
    #[serde(rename = "hiddenPaneIds")]
    pub hidden_pane_ids: Vec<String>,
}

/// Raw Claude Code team member entry (serde).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RawTeamMember {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub name: String,
    #[serde(rename = "agentType")]
    pub agent_type: String,
    pub model: String,
    pub color: String,
    #[serde(rename = "planModeRequired")]
    pub plan_mode_required: bool,
    #[serde(rename = "joinedAt")]
    pub joined_at: u64,
    #[serde(rename = "tmuxPaneId")]
    pub tmux_pane_id: String,
    pub cwd: String,
    #[serde(rename = "backendType")]
    pub backend_type: String,
    #[serde(rename = "isActive")]
    pub is_active: bool,
}

// =============================================================================
// Shim pane registry: ~/.kild/shim/<session_id>/panes.json
// (Duplicates minimal fields from kild-tmux-shim to avoid dependency)
// =============================================================================

/// Minimal shim pane registry for cross-referencing.
#[derive(Debug, Deserialize)]
pub struct ShimPaneRegistry {
    pub panes: HashMap<String, ShimPaneEntry>,
}

/// Minimal pane entry — only fields we need for mapping.
#[derive(Debug, Deserialize)]
pub struct ShimPaneEntry {
    pub daemon_session_id: String,
    pub title: String,
    #[serde(default)]
    pub border_style: String,
    #[serde(default)]
    pub hidden: bool,
}

// =============================================================================
// Parse functions
// =============================================================================

/// Parse a Claude Code team config file.
///
/// Returns `Ok(None)` for missing file, `Err` for malformed JSON.
pub fn parse_team_config(path: &Path) -> Result<Option<TeamState>, TeamsError> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let raw: RawTeamConfig = serde_json::from_str(&content)?;

    // Derive team name from directory name
    let team_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let members = raw
        .members
        .into_iter()
        .map(|m| {
            let is_leader = m.tmux_pane_id.is_empty() || m.tmux_pane_id == "%0";
            TeamMember {
                name: m.name,
                agent_id: m.agent_id,
                agent_type: m.agent_type,
                color: TeamColor::parse(&m.color),
                pane_id: m.tmux_pane_id,
                daemon_session_id: None,
                is_active: m.is_active,
                is_leader,
            }
        })
        .collect();

    Ok(Some(TeamState {
        team_name,
        kild_session_id: None,
        members,
    }))
}

/// Parse a shim pane registry file.
///
/// Returns `Ok(None)` for missing file, `Err` for malformed JSON.
pub fn parse_shim_registry(path: &Path) -> Result<Option<ShimPaneRegistry>, TeamsError> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let registry: ShimPaneRegistry = serde_json::from_str(&content)?;
    Ok(Some(registry))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_parse_team_config_valid() {
        let dir = tempfile::TempDir::new().unwrap();
        let team_dir = dir.path().join("my-team");
        fs::create_dir_all(&team_dir).unwrap();
        let config_path = team_dir.join("config.json");

        fs::write(
            &config_path,
            r#"{
                "members": [
                    {
                        "agentId": "researcher@my-team",
                        "name": "researcher",
                        "agentType": "general-purpose",
                        "model": "claude-sonnet-4-5-20250929",
                        "color": "blue",
                        "planModeRequired": false,
                        "joinedAt": 1707500000000,
                        "tmuxPaneId": "%1",
                        "cwd": "/project",
                        "backendType": "tmux",
                        "isActive": true
                    }
                ],
                "hiddenPaneIds": []
            }"#,
        )
        .unwrap();

        let result = parse_team_config(&config_path).unwrap().unwrap();
        assert_eq!(result.team_name, "my-team");
        assert_eq!(result.members.len(), 1);
        assert_eq!(result.members[0].name, "researcher");
        assert_eq!(result.members[0].color, TeamColor::Blue);
        assert_eq!(result.members[0].pane_id, "%1");
        assert!(result.members[0].is_active);
        assert!(!result.members[0].is_leader);
    }

    #[test]
    fn test_parse_team_config_leader_detection() {
        let dir = tempfile::TempDir::new().unwrap();
        let team_dir = dir.path().join("team");
        fs::create_dir_all(&team_dir).unwrap();
        let config_path = team_dir.join("config.json");

        // Leader has empty tmuxPaneId
        fs::write(
            &config_path,
            r#"{
                "members": [
                    { "name": "leader", "tmuxPaneId": "" },
                    { "name": "worker", "tmuxPaneId": "%1" }
                ]
            }"#,
        )
        .unwrap();

        let result = parse_team_config(&config_path).unwrap().unwrap();
        assert!(result.members[0].is_leader);
        assert!(!result.members[1].is_leader);
    }

    #[test]
    fn test_parse_team_config_leader_pane_zero() {
        let dir = tempfile::TempDir::new().unwrap();
        let team_dir = dir.path().join("team");
        fs::create_dir_all(&team_dir).unwrap();
        let config_path = team_dir.join("config.json");

        fs::write(
            &config_path,
            r#"{ "members": [{ "name": "leader", "tmuxPaneId": "%0" }] }"#,
        )
        .unwrap();

        let result = parse_team_config(&config_path).unwrap().unwrap();
        assert!(result.members[0].is_leader);
    }

    #[test]
    fn test_parse_team_config_minimal_fields() {
        let dir = tempfile::TempDir::new().unwrap();
        let team_dir = dir.path().join("team");
        fs::create_dir_all(&team_dir).unwrap();
        let config_path = team_dir.join("config.json");

        // Only required structure, all fields default
        fs::write(&config_path, r#"{ "members": [{}] }"#).unwrap();

        let result = parse_team_config(&config_path).unwrap().unwrap();
        assert_eq!(result.members.len(), 1);
        assert_eq!(result.members[0].name, "");
        assert_eq!(result.members[0].color, TeamColor::Unknown);
        assert!(result.members[0].is_leader); // empty pane_id = leader
    }

    #[test]
    fn test_parse_team_config_extra_unknown_fields() {
        let dir = tempfile::TempDir::new().unwrap();
        let team_dir = dir.path().join("team");
        fs::create_dir_all(&team_dir).unwrap();
        let config_path = team_dir.join("config.json");

        fs::write(
            &config_path,
            r#"{
                "members": [{ "name": "a", "futureField": 42 }],
                "hiddenPaneIds": [],
                "anotherNewField": "hello"
            }"#,
        )
        .unwrap();

        let result = parse_team_config(&config_path).unwrap().unwrap();
        assert_eq!(result.members.len(), 1);
    }

    #[test]
    fn test_parse_team_config_missing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.json");
        let result = parse_team_config(&path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_team_config_malformed_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "not valid json{{{").unwrap();
        assert!(parse_team_config(&path).is_err());
    }

    #[test]
    fn test_parse_team_config_empty_members() {
        let dir = tempfile::TempDir::new().unwrap();
        let team_dir = dir.path().join("team");
        fs::create_dir_all(&team_dir).unwrap();
        let config_path = team_dir.join("config.json");

        fs::write(&config_path, r#"{ "members": [] }"#).unwrap();

        let result = parse_team_config(&config_path).unwrap().unwrap();
        assert!(result.members.is_empty());
    }

    #[test]
    fn test_parse_shim_registry_valid() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("panes.json");

        fs::write(
            &path,
            r#"{
                "next_pane_id": 2,
                "session_name": "kild_0",
                "panes": {
                    "%0": {
                        "daemon_session_id": "d-1",
                        "title": "",
                        "border_style": "",
                        "window_id": "0",
                        "hidden": false
                    },
                    "%1": {
                        "daemon_session_id": "d-2",
                        "title": "researcher",
                        "border_style": "fg=blue",
                        "window_id": "0",
                        "hidden": false
                    }
                },
                "windows": { "0": { "name": "main", "pane_ids": ["%0", "%1"] } },
                "sessions": {}
            }"#,
        )
        .unwrap();

        let result = parse_shim_registry(&path).unwrap().unwrap();
        assert_eq!(result.panes.len(), 2);
        assert_eq!(result.panes["%0"].daemon_session_id, "d-1");
        assert_eq!(result.panes["%1"].title, "researcher");
    }

    #[test]
    fn test_parse_shim_registry_missing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("missing.json");
        assert!(parse_shim_registry(&path).unwrap().is_none());
    }

    #[test]
    fn test_parse_shim_registry_malformed() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("panes.json");
        fs::write(&path, "garbage").unwrap();
        assert!(parse_shim_registry(&path).is_err());
    }
}
