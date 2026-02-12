use std::collections::HashSet;

use serde::Serialize;

use kild_core::sessions::types::SessionStatus;

/// Fleet-level summary metrics for list output.
#[derive(Serialize)]
pub struct FleetSummary {
    pub total: usize,
    pub active: usize,
    pub stopped: usize,
    pub conflicts: usize,
    pub needs_push: usize,
}

impl FleetSummary {
    /// Derive fleet summary from enriched sessions and overlap analysis.
    ///
    /// All counts are computed from the session list, ensuring consistency.
    pub fn from_enriched(sessions: &[EnrichedSession], conflict_count: usize) -> Self {
        Self {
            total: sessions.len(),
            active: sessions
                .iter()
                .filter(|e| e.session.status == SessionStatus::Active)
                .count(),
            stopped: sessions
                .iter()
                .filter(|e| e.session.status == SessionStatus::Stopped)
                .count(),
            conflicts: conflict_count,
            needs_push: sessions
                .iter()
                .filter(|e| {
                    e.git_stats
                        .as_ref()
                        .and_then(|gs| gs.worktree_status.as_ref())
                        .is_some_and(|ws| ws.unpushed_commit_count > 0 || !ws.has_remote_branch)
                })
                .count(),
        }
    }

    /// Derive fleet summary from raw sessions with pre-collected git stats.
    pub fn from_sessions(
        sessions: &[kild_core::Session],
        git_stats: &[Option<kild_core::GitStats>],
        conflict_count: usize,
    ) -> Self {
        Self {
            total: sessions.len(),
            active: sessions
                .iter()
                .filter(|s| s.status == SessionStatus::Active)
                .count(),
            stopped: sessions
                .iter()
                .filter(|s| s.status == SessionStatus::Stopped)
                .count(),
            conflicts: conflict_count,
            needs_push: git_stats
                .iter()
                .filter(|gs| {
                    gs.as_ref()
                        .and_then(|g| g.worktree_status.as_ref())
                        .is_some_and(|ws| ws.unpushed_commit_count > 0 || !ws.has_remote_branch)
                })
                .count(),
        }
    }
}

/// Top-level list output with fleet summary (JSON only).
#[derive(Serialize)]
pub struct ListOutput {
    pub sessions: Vec<EnrichedSession>,
    pub fleet_summary: FleetSummary,
}

impl ListOutput {
    /// Construct list output with fleet summary derived from the sessions.
    pub fn new(sessions: Vec<EnrichedSession>, kilds_with_conflicts: &HashSet<&str>) -> Self {
        let fleet_summary = FleetSummary::from_enriched(&sessions, kilds_with_conflicts.len());
        Self {
            sessions,
            fleet_summary,
        }
    }
}

/// JSON error response for --json commands.
#[derive(Serialize)]
pub struct JsonError {
    pub error: String,
    pub code: String,
}

/// JSON response for agent-status command confirmation.
#[derive(Serialize)]
pub struct AgentStatusResponse {
    pub branch: String,
    pub status: String,
    pub updated_at: String,
}

/// Enriched session data for JSON output (used by list and status commands).
#[derive(Serialize)]
pub struct EnrichedSession {
    #[serde(flatten)]
    pub session: kild_core::Session,
    pub process_status: kild_core::ProcessStatus,
    pub git_stats: Option<kild_core::GitStats>,
    pub branch_health: Option<kild_core::BranchHealth>,
    pub merge_readiness: Option<kild_core::MergeReadiness>,
    pub agent_status: Option<String>,
    pub agent_status_updated_at: Option<String>,
    pub terminal_window_title: Option<String>,
    pub terminal_type: Option<String>,
    pub pr_info: Option<kild_core::PrInfo>,
    pub overlapping_files: Option<Vec<String>>,
}
