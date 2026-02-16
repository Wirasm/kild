//! Team state manager for the UI.
//!
//! Owns the team watcher and cached team state. Provides queries
//! for the sidebar and main view to discover teammates.

use std::collections::HashMap;

use kild_teams::{TeamMember, TeamState, TeamWatcher};

/// Manages team state for the UI, providing cached team data
/// and file-watching for live updates.
pub struct TeamManager {
    /// Cached team states keyed by kild session_id.
    team_states: HashMap<String, TeamState>,
    /// File watcher for team config and shim registry changes.
    watcher: Option<TeamWatcher>,
    /// Mapping from team_name → kild session_id (for cross-referencing).
    team_to_session: HashMap<String, String>,
}

impl TeamManager {
    pub fn new() -> Self {
        let watcher = TeamWatcher::new_default();
        if watcher.is_some() {
            tracing::info!(event = "ui.teams.watcher_created");
        } else {
            tracing::debug!(event = "ui.teams.watcher_unavailable");
        }

        Self {
            team_states: HashMap::new(),
            watcher,
            team_to_session: HashMap::new(),
        }
    }

    /// Re-scan teams and cross-reference with shim registries.
    ///
    /// Called when the watcher detects changes or periodically.
    /// Takes a set of known daemon kild session IDs and their branch names
    /// to cross-reference teams with sessions.
    pub fn refresh(&mut self, session_ids: &[(&str, &str)]) {
        let teams = kild_teams::scanner::scan_teams_default();

        self.team_states.clear();
        self.team_to_session.clear();

        for (team_name, team_state) in teams {
            // Try to match this team to a kild session by scanning shim registries.
            // For each known session, check if the shim has teammates.
            let mut matched = false;

            for &(session_id, _branch) in session_ids {
                match kild_teams::mapper::resolve_team(team_state.clone(), session_id) {
                    Ok(resolved) if resolved.kild_session_id.is_some() => {
                        // Check if any member actually resolved a daemon_session_id
                        let has_resolved_members = resolved
                            .members
                            .iter()
                            .any(|m| m.daemon_session_id.is_some());
                        if has_resolved_members {
                            tracing::debug!(
                                event = "ui.teams.team_matched",
                                team = team_name,
                                session_id = session_id,
                                members = resolved.members.len()
                            );
                            self.team_to_session
                                .insert(team_name.clone(), session_id.to_string());
                            self.team_states.insert(session_id.to_string(), resolved);
                            matched = true;
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(
                            event = "ui.teams.resolve_failed",
                            team = team_name,
                            session_id = session_id,
                            error = %e
                        );
                    }
                }
            }

            if !matched {
                tracing::debug!(
                    event = "ui.teams.team_unmatched",
                    team = team_name,
                    "No kild session matched for team"
                );
            }
        }

        // Fallback: for sessions without a team config match, try shim discovery
        for &(session_id, _branch) in session_ids {
            if self.team_states.contains_key(session_id) {
                continue;
            }
            match kild_teams::discovery::discover_teammates(session_id) {
                Ok(Some(members)) => {
                    tracing::debug!(
                        event = "ui.teams.fallback_discovery",
                        session_id = session_id,
                        members = members.len()
                    );
                    self.team_states.insert(
                        session_id.to_string(),
                        TeamState {
                            team_name: format!("shim-{}", session_id),
                            kild_session_id: Some(session_id.to_string()),
                            members,
                        },
                    );
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::debug!(
                        event = "ui.teams.fallback_discovery_failed",
                        session_id = session_id,
                        error = %e
                    );
                }
            }
        }
    }

    /// Get team state for a kild session.
    #[allow(dead_code)]
    pub fn team_for_session(&self, session_id: &str) -> Option<&TeamState> {
        self.team_states.get(session_id)
    }

    /// Get non-leader teammates for a kild session.
    pub fn teammates_for_session(&self, session_id: &str) -> Vec<&TeamMember> {
        self.team_states
            .get(session_id)
            .map(|t| t.teammates().collect())
            .unwrap_or_default()
    }

    /// Check if the watcher has pending events.
    pub fn has_pending_events(&self) -> bool {
        self.watcher
            .as_ref()
            .is_some_and(|w| w.has_pending_events())
    }
}
