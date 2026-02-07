use serde::Serialize;

/// Enriched session data for JSON output (used by list and status commands).
#[derive(Serialize)]
pub struct EnrichedSession {
    #[serde(flatten)]
    pub session: kild_core::Session,
    pub git_stats: Option<kild_core::GitStats>,
    pub agent_status: Option<String>,
    pub agent_status_updated_at: Option<String>,
    pub terminal_window_title: Option<String>,
    pub pr_info: Option<kild_core::PrInfo>,
}
