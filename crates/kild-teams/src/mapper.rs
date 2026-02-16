//! Cross-reference team config with shim pane registry.
//!
//! Enriches `TeamMember` entries with `daemon_session_id` by reading
//! the shim pane registry for a given kild session.

use std::path::PathBuf;

use crate::errors::TeamsError;
use crate::parser;
use crate::types::TeamState;

/// Default shim state directory: `~/.kild/shim/`.
fn shim_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".kild").join("shim"))
}

/// Resolve a shim pane registry path for a session.
fn shim_registry_path(session_id: &str) -> Option<PathBuf> {
    shim_dir().map(|d| d.join(session_id).join("panes.json"))
}

/// Enrich team state with daemon session IDs from the shim pane registry.
///
/// For each member, looks up their `pane_id` in the registry and copies
/// the `daemon_session_id`. Members with no matching pane keep `None`.
pub fn resolve_team(mut team_state: TeamState, session_id: &str) -> Result<TeamState, TeamsError> {
    let Some(registry_path) = shim_registry_path(session_id) else {
        tracing::debug!(
            event = "teams.mapper.home_dir_unavailable",
            session_id = session_id
        );
        return Ok(team_state);
    };

    let registry = match parser::parse_shim_registry(&registry_path)? {
        Some(r) => r,
        None => {
            tracing::debug!(
                event = "teams.mapper.no_shim_registry",
                session_id = session_id,
                path = %registry_path.display()
            );
            return Ok(team_state);
        }
    };

    team_state.kild_session_id = Some(session_id.to_string());

    for member in &mut team_state.members {
        if let Some(pane) = registry.panes.get(&member.pane_id) {
            member.daemon_session_id = Some(pane.daemon_session_id.clone());
            tracing::debug!(
                event = "teams.mapper.pane_resolved",
                member = member.name,
                pane_id = member.pane_id,
                daemon_session_id = pane.daemon_session_id
            );
        } else {
            tracing::debug!(
                event = "teams.mapper.pane_not_found",
                member = member.name,
                pane_id = member.pane_id,
                session_id = session_id
            );
        }
    }

    Ok(team_state)
}

/// Resolve team state from a registry at a custom path (for testing).
pub fn resolve_team_with_registry(
    mut team_state: TeamState,
    registry_path: &std::path::Path,
    session_id: &str,
) -> Result<TeamState, TeamsError> {
    let registry = match parser::parse_shim_registry(registry_path)? {
        Some(r) => r,
        None => return Ok(team_state),
    };

    team_state.kild_session_id = Some(session_id.to_string());

    for member in &mut team_state.members {
        if let Some(pane) = registry.panes.get(&member.pane_id) {
            member.daemon_session_id = Some(pane.daemon_session_id.clone());
        }
    }

    Ok(team_state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TeamColor, TeamMember};
    use std::fs;

    fn make_team(members: Vec<TeamMember>) -> TeamState {
        TeamState {
            team_name: "test-team".to_string(),
            kild_session_id: None,
            members,
        }
    }

    fn make_member(name: &str, pane_id: &str, is_leader: bool) -> TeamMember {
        TeamMember {
            name: name.to_string(),
            agent_id: format!("{}@test-team", name),
            agent_type: "general-purpose".to_string(),
            color: TeamColor::Blue,
            pane_id: pane_id.to_string(),
            daemon_session_id: None,
            is_active: true,
            is_leader,
        }
    }

    #[test]
    fn test_resolve_all_mapped() {
        let dir = tempfile::TempDir::new().unwrap();
        let registry_path = dir.path().join("panes.json");

        fs::write(
            &registry_path,
            r#"{
                "panes": {
                    "%0": { "daemon_session_id": "d-leader", "title": "", "border_style": "" },
                    "%1": { "daemon_session_id": "d-worker1", "title": "worker1", "border_style": "fg=blue" },
                    "%2": { "daemon_session_id": "d-worker2", "title": "worker2", "border_style": "fg=red" }
                }
            }"#,
        )
        .unwrap();

        let team = make_team(vec![
            make_member("leader", "%0", true),
            make_member("worker1", "%1", false),
            make_member("worker2", "%2", false),
        ]);

        let resolved = resolve_team_with_registry(team, &registry_path, "sess-123").unwrap();

        assert_eq!(resolved.kild_session_id.as_deref(), Some("sess-123"));
        assert_eq!(
            resolved.members[0].daemon_session_id.as_deref(),
            Some("d-leader")
        );
        assert_eq!(
            resolved.members[1].daemon_session_id.as_deref(),
            Some("d-worker1")
        );
        assert_eq!(
            resolved.members[2].daemon_session_id.as_deref(),
            Some("d-worker2")
        );
    }

    #[test]
    fn test_resolve_missing_pane() {
        let dir = tempfile::TempDir::new().unwrap();
        let registry_path = dir.path().join("panes.json");

        fs::write(
            &registry_path,
            r#"{ "panes": { "%0": { "daemon_session_id": "d-1", "title": "" } } }"#,
        )
        .unwrap();

        let team = make_team(vec![
            make_member("leader", "%0", true),
            make_member("ghost", "%99", false),
        ]);

        let resolved = resolve_team_with_registry(team, &registry_path, "sess").unwrap();

        assert_eq!(
            resolved.members[0].daemon_session_id.as_deref(),
            Some("d-1")
        );
        assert!(resolved.members[1].daemon_session_id.is_none());
    }

    #[test]
    fn test_resolve_empty_members() {
        let dir = tempfile::TempDir::new().unwrap();
        let registry_path = dir.path().join("panes.json");

        fs::write(&registry_path, r#"{ "panes": {} }"#).unwrap();

        let team = make_team(vec![]);
        let resolved = resolve_team_with_registry(team, &registry_path, "sess").unwrap();

        assert!(resolved.members.is_empty());
        assert_eq!(resolved.kild_session_id.as_deref(), Some("sess"));
    }

    #[test]
    fn test_resolve_missing_registry() {
        let dir = tempfile::TempDir::new().unwrap();
        let registry_path = dir.path().join("nonexistent.json");

        let team = make_team(vec![make_member("a", "%0", true)]);
        let resolved = resolve_team_with_registry(team, &registry_path, "sess").unwrap();

        // No daemon session IDs resolved, but no error
        assert!(resolved.members[0].daemon_session_id.is_none());
        assert!(resolved.kild_session_id.is_none());
    }

    #[test]
    fn test_resolve_leader_no_pane_id() {
        let dir = tempfile::TempDir::new().unwrap();
        let registry_path = dir.path().join("panes.json");

        fs::write(
            &registry_path,
            r#"{ "panes": { "%0": { "daemon_session_id": "d-1", "title": "" } } }"#,
        )
        .unwrap();

        // Leader with empty pane_id won't match any registry entry
        let team = make_team(vec![make_member("leader", "", true)]);
        let resolved = resolve_team_with_registry(team, &registry_path, "sess").unwrap();

        assert!(resolved.members[0].daemon_session_id.is_none());
    }
}
