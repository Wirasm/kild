//! Fallback teammate discovery from shim pane registry.
//!
//! When no Claude Code team config exists, discovers teammates directly
//! from the shim `panes.json`. Less rich than team config (no agent_type,
//! no agent_id) but sufficient for basic UI integration.

use std::path::PathBuf;

use crate::errors::TeamsError;
use crate::parser;
use crate::types::{TeamColor, TeamMember};

/// Default shim state directory: `~/.kild/shim/`.
fn shim_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".kild").join("shim"))
}

/// Discover teammates from the shim pane registry for a session.
///
/// Returns `None` if the registry doesn't exist or has only the leader pane.
/// `%0` is treated as the leader; all others are teammates.
pub fn discover_teammates(session_id: &str) -> Result<Option<Vec<TeamMember>>, TeamsError> {
    let Some(base) = shim_dir() else {
        return Ok(None);
    };
    let registry_path = base.join(session_id).join("panes.json");
    discover_teammates_from_path(&registry_path)
}

/// Discover teammates from a specific registry path (for testing).
pub fn discover_teammates_from_path(
    registry_path: &std::path::Path,
) -> Result<Option<Vec<TeamMember>>, TeamsError> {
    let registry = match parser::parse_shim_registry(registry_path)? {
        Some(r) => r,
        None => return Ok(None),
    };

    // Only leader pane — no teammates
    if registry.panes.len() <= 1 {
        return Ok(None);
    }

    let mut members: Vec<TeamMember> = registry
        .panes
        .iter()
        .filter(|(_, pane)| !pane.hidden)
        .map(|(pane_id, pane)| {
            let is_leader = pane_id == "%0";
            let name = if pane.title.is_empty() {
                if is_leader {
                    "leader".to_string()
                } else {
                    format!("teammate-{}", pane_id.trim_start_matches('%'))
                }
            } else {
                pane.title.clone()
            };

            let color = TeamColor::from_border_style(&pane.border_style);

            TeamMember {
                name,
                agent_id: String::new(),
                agent_type: String::new(),
                color,
                pane_id: pane_id.clone(),
                daemon_session_id: Some(pane.daemon_session_id.clone()),
                is_active: true,
                is_leader,
            }
        })
        .collect();

    // Sort by pane_id for stable ordering (%0, %1, %2, ...)
    members.sort_by(|a, b| a.pane_id.cmp(&b.pane_id));

    Ok(Some(members))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_discover_basic() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("panes.json");

        fs::write(
            &path,
            r#"{
                "panes": {
                    "%0": { "daemon_session_id": "d-0", "title": "", "border_style": "" },
                    "%1": { "daemon_session_id": "d-1", "title": "researcher", "border_style": "fg=blue" }
                }
            }"#,
        )
        .unwrap();

        let members = discover_teammates_from_path(&path).unwrap().unwrap();
        assert_eq!(members.len(), 2);

        // Leader
        assert_eq!(members[0].pane_id, "%0");
        assert!(members[0].is_leader);
        assert_eq!(members[0].name, "leader");
        assert_eq!(members[0].daemon_session_id.as_deref(), Some("d-0"));

        // Teammate
        assert_eq!(members[1].pane_id, "%1");
        assert!(!members[1].is_leader);
        assert_eq!(members[1].name, "researcher");
        assert_eq!(members[1].color, TeamColor::Blue);
        assert_eq!(members[1].daemon_session_id.as_deref(), Some("d-1"));
    }

    #[test]
    fn test_discover_leader_only() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("panes.json");

        fs::write(
            &path,
            r#"{ "panes": { "%0": { "daemon_session_id": "d-0", "title": "" } } }"#,
        )
        .unwrap();

        // Only leader — no team
        assert!(discover_teammates_from_path(&path).unwrap().is_none());
    }

    #[test]
    fn test_discover_title_as_name() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("panes.json");

        fs::write(
            &path,
            r#"{
                "panes": {
                    "%0": { "daemon_session_id": "d-0", "title": "main-agent" },
                    "%1": { "daemon_session_id": "d-1", "title": "" }
                }
            }"#,
        )
        .unwrap();

        let members = discover_teammates_from_path(&path).unwrap().unwrap();
        assert_eq!(members[0].name, "main-agent"); // title used
        assert_eq!(members[1].name, "teammate-1"); // fallback
    }

    #[test]
    fn test_discover_color_parsing() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("panes.json");

        fs::write(
            &path,
            r#"{
                "panes": {
                    "%0": { "daemon_session_id": "d-0", "title": "", "border_style": "" },
                    "%1": { "daemon_session_id": "d-1", "title": "a", "border_style": "fg=red" },
                    "%2": { "daemon_session_id": "d-2", "title": "b", "border_style": "fg=cyan" }
                }
            }"#,
        )
        .unwrap();

        let members = discover_teammates_from_path(&path).unwrap().unwrap();
        assert_eq!(members[0].color, TeamColor::Unknown); // leader, no style
        assert_eq!(members[1].color, TeamColor::Red);
        assert_eq!(members[2].color, TeamColor::Cyan);
    }

    #[test]
    fn test_discover_hidden_panes_skipped() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("panes.json");

        fs::write(
            &path,
            r#"{
                "panes": {
                    "%0": { "daemon_session_id": "d-0", "title": "", "hidden": false },
                    "%1": { "daemon_session_id": "d-1", "title": "visible", "hidden": false },
                    "%2": { "daemon_session_id": "d-2", "title": "hidden", "hidden": true }
                }
            }"#,
        )
        .unwrap();

        let members = discover_teammates_from_path(&path).unwrap().unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.iter().all(|m| m.name != "hidden"));
    }

    #[test]
    fn test_discover_missing_registry() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.json");
        assert!(discover_teammates_from_path(&path).unwrap().is_none());
    }
}
