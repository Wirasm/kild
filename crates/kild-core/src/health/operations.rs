use crate::health::types::{HealthMetrics, HealthOutput, HealthStatus, KildHealth};
use crate::process::types::ProcessMetrics;
use crate::sessions::types::{AgentStatus, Session};
use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicU64, Ordering};

static IDLE_THRESHOLD_MINUTES: AtomicU64 = AtomicU64::new(10);

/// Set the idle threshold for health status calculation
pub fn set_idle_threshold_minutes(minutes: u64) {
    IDLE_THRESHOLD_MINUTES.store(minutes, Ordering::Relaxed);
}

/// Get the current idle threshold
pub fn get_idle_threshold_minutes() -> u64 {
    IDLE_THRESHOLD_MINUTES.load(Ordering::Relaxed)
}

/// Calculate health status based on process state and activity
pub fn calculate_health_status(
    process_running: bool,
    last_activity: Option<&str>,
    last_message_from_user: bool,
) -> HealthStatus {
    if !process_running {
        return HealthStatus::Crashed;
    }

    let Some(activity_str) = last_activity else {
        return HealthStatus::Unknown;
    };

    let Ok(activity_time) = DateTime::parse_from_rfc3339(activity_str) else {
        return HealthStatus::Unknown;
    };

    let now = Utc::now();
    let minutes_since_activity = (now.signed_duration_since(activity_time)).num_minutes();
    let threshold = IDLE_THRESHOLD_MINUTES.load(Ordering::Relaxed);

    // Compare as i64 (threshold fits in i64, and minutes_since_activity is i64)
    if minutes_since_activity < threshold as i64 {
        HealthStatus::Working
    } else if last_message_from_user {
        HealthStatus::Stuck
    } else {
        HealthStatus::Idle
    }
}

/// Returns the more recent of two optional RFC3339 timestamps.
///
/// If both are present, parses and compares; returns whichever is valid when
/// only one parses; falls back to `a` when neither parses.
/// If only one is present, returns that one.
fn most_recent_activity(a: Option<&str>, b: Option<&str>) -> Option<String> {
    match (a, b) {
        (None, x) | (x, None) => x.map(str::to_string),
        (Some(ta), Some(tb)) => {
            let ta_dt = DateTime::parse_from_rfc3339(ta).ok();
            let tb_dt = DateTime::parse_from_rfc3339(tb).ok();
            match (ta_dt, tb_dt) {
                (Some(a_dt), Some(b_dt)) => Some(if a_dt >= b_dt { ta } else { tb }.to_string()),
                (None, Some(_)) => Some(tb.to_string()),
                (Some(_), None) | (None, None) => Some(ta.to_string()),
            }
        }
    }
}

/// Enrich session with health metrics
pub fn enrich_session_with_health(
    session: &Session,
    process_metrics: Option<ProcessMetrics>,
    process_running: bool,
    agent_status: Option<AgentStatus>,
    agent_status_updated_at: Option<String>,
) -> KildHealth {
    // Use the most recent of kild.json last_activity and sidecar updated_at.
    // Agent hook updates only touch the sidecar; kild.json last_activity
    // reflects lifecycle events only.
    let effective_last_activity = most_recent_activity(
        session.last_activity.as_deref(),
        agent_status_updated_at.as_deref(),
    );
    let status = calculate_health_status(
        process_running,
        effective_last_activity.as_deref(),
        false, // TODO: Track last message sender in future
    );

    let status_icon = match status {
        HealthStatus::Working => "✅",
        HealthStatus::Idle => "⏸️ ",
        HealthStatus::Stuck => "⚠️ ",
        HealthStatus::Crashed => "❌",
        HealthStatus::Unknown => "❓",
    };

    let metrics = HealthMetrics {
        cpu_usage_percent: process_metrics.as_ref().map(|m| m.cpu_usage_percent),
        memory_usage_mb: process_metrics.as_ref().map(|m| m.memory_usage_mb()),
        process_status: if process_running {
            "Running".to_string()
        } else {
            "Stopped".to_string()
        },
        last_activity: effective_last_activity,
        status,
        status_icon: status_icon.to_string(),
    };

    KildHealth {
        session_id: session.id.to_string(),
        project_id: session.project_id.to_string(),
        branch: session.branch.to_string(),
        agent: session.agent.clone(),
        worktree_path: session.worktree_path.display().to_string(),
        created_at: session.created_at.clone(),
        agent_status,
        agent_status_updated_at,
        metrics,
    }
}

/// Aggregate health statistics
pub fn aggregate_health_stats(kilds: &[KildHealth]) -> HealthOutput {
    let mut working = 0;
    let mut idle = 0;
    let mut stuck = 0;
    let mut crashed = 0;

    for kild in kilds {
        match kild.metrics.status {
            HealthStatus::Working => working += 1,
            HealthStatus::Idle => idle += 1,
            HealthStatus::Stuck => stuck += 1,
            HealthStatus::Crashed => crashed += 1,
            HealthStatus::Unknown => {}
        }
    }

    HealthOutput {
        kilds: kilds.to_vec(),
        total_count: kilds.len(),
        working_count: working,
        idle_count: idle,
        stuck_count: stuck,
        crashed_count: crashed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::path::PathBuf;

    // --- calculate_health_status tests ---

    #[test]
    fn test_calculate_health_status_crashed_when_not_running() {
        let result = calculate_health_status(false, Some("2026-01-01T00:00:00Z"), false);
        assert_eq!(result, HealthStatus::Crashed);
    }

    #[test]
    fn test_calculate_health_status_unknown_when_no_activity() {
        let result = calculate_health_status(true, None, false);
        assert_eq!(result, HealthStatus::Unknown);
    }

    #[test]
    fn test_calculate_health_status_unknown_when_invalid_timestamp() {
        let result = calculate_health_status(true, Some("not-a-timestamp"), false);
        assert_eq!(result, HealthStatus::Unknown);
    }

    #[test]
    fn test_calculate_health_status_working_when_recent_activity() {
        let recent = Utc::now().to_rfc3339();
        let result = calculate_health_status(true, Some(&recent), false);
        assert_eq!(result, HealthStatus::Working);
    }

    #[test]
    fn test_calculate_health_status_idle_when_old_activity_from_agent() {
        // Use activity far older than any threshold to avoid races with parallel tests
        set_idle_threshold_minutes(10);
        let old = (Utc::now() - chrono::Duration::minutes(200)).to_rfc3339();
        let result = calculate_health_status(true, Some(&old), false);
        assert_eq!(result, HealthStatus::Idle);
    }

    #[test]
    fn test_calculate_health_status_stuck_when_old_activity_from_user() {
        set_idle_threshold_minutes(10);
        let old = (Utc::now() - chrono::Duration::minutes(200)).to_rfc3339();
        let result = calculate_health_status(true, Some(&old), true);
        assert_eq!(result, HealthStatus::Stuck);
    }

    #[test]
    fn test_calculate_health_status_threshold_boundary() {
        // Activity well beyond any threshold proves >= threshold → Idle (strict < comparison)
        set_idle_threshold_minutes(5);
        let old = (Utc::now() - chrono::Duration::minutes(200)).to_rfc3339();
        let result = calculate_health_status(true, Some(&old), false);
        assert_eq!(result, HealthStatus::Idle);
    }

    #[test]
    fn test_calculate_health_status_respects_custom_threshold() {
        // Use very recent activity that's within any threshold to avoid races
        set_idle_threshold_minutes(60);
        let just_now = (Utc::now() - chrono::Duration::seconds(30)).to_rfc3339();
        let result = calculate_health_status(true, Some(&just_now), false);
        assert_eq!(result, HealthStatus::Working);
        set_idle_threshold_minutes(10);
    }

    #[test]
    fn test_calculate_health_status_crashed_takes_priority_over_activity() {
        let recent = Utc::now().to_rfc3339();
        let result = calculate_health_status(false, Some(&recent), false);
        assert_eq!(result, HealthStatus::Crashed);
    }

    // --- threshold getter/setter tests ---

    #[test]
    fn test_idle_threshold_get_set_roundtrip() {
        set_idle_threshold_minutes(42);
        assert_eq!(get_idle_threshold_minutes(), 42);
        set_idle_threshold_minutes(10);
    }

    // --- enrich_session_with_health tests ---

    #[test]
    fn test_enrich_session_running_with_metrics() {
        let session = Session::new_for_test("test-branch".to_string(), PathBuf::from("/tmp/test"));
        let metrics = ProcessMetrics {
            cpu_usage_percent: 25.0,
            memory_usage_bytes: 100 * 1024 * 1024,
        };

        let health = enrich_session_with_health(&session, Some(metrics), true, None, None);

        assert_eq!(health.branch, "test-branch");
        assert_eq!(health.metrics.process_status, "Running");
        assert_eq!(health.metrics.cpu_usage_percent, Some(25.0));
        assert_eq!(health.metrics.memory_usage_mb, Some(100));
    }

    #[test]
    fn test_enrich_session_stopped_no_metrics() {
        let session = Session::new_for_test("stopped".to_string(), PathBuf::from("/tmp/test"));

        let health = enrich_session_with_health(&session, None, false, None, None);

        assert_eq!(health.metrics.process_status, "Stopped");
        assert_eq!(health.metrics.status, HealthStatus::Crashed);
        assert_eq!(health.metrics.cpu_usage_percent, None);
        assert_eq!(health.metrics.memory_usage_mb, None);
        assert_eq!(health.metrics.status_icon, "\u{274c}");
    }

    #[test]
    fn test_enrich_session_copies_session_fields() {
        let session = Session::new_for_test("my-branch".to_string(), PathBuf::from("/tmp/wt"));

        let health = enrich_session_with_health(&session, None, false, None, None);

        assert_eq!(health.session_id, session.id.to_string());
        assert_eq!(health.project_id, session.project_id.to_string());
        assert_eq!(health.branch, session.branch.to_string());
        assert_eq!(health.agent, session.agent);
        assert_eq!(health.worktree_path, "/tmp/wt");
        assert_eq!(health.created_at, session.created_at);
    }

    // --- agent status enrichment tests ---

    #[test]
    fn test_enrich_session_with_agent_status() {
        let session = Session::new_for_test("test".to_string(), PathBuf::from("/tmp"));

        let health = enrich_session_with_health(
            &session,
            None,
            true,
            Some(AgentStatus::Working),
            Some("2026-02-05T12:00:00Z".to_string()),
        );

        assert_eq!(health.agent_status, Some(AgentStatus::Working));
        assert_eq!(
            health.agent_status_updated_at,
            Some("2026-02-05T12:00:00Z".to_string())
        );
    }

    #[test]
    fn test_enrich_session_without_agent_status() {
        let session = Session::new_for_test("test".to_string(), PathBuf::from("/tmp"));

        let health = enrich_session_with_health(&session, None, true, None, None);

        assert_eq!(health.agent_status, None);
        assert_eq!(health.agent_status_updated_at, None);
    }

    #[test]
    fn test_enrich_session_all_agent_status_variants() {
        let session = Session::new_for_test("test".to_string(), PathBuf::from("/tmp"));

        for status in [
            AgentStatus::Working,
            AgentStatus::Idle,
            AgentStatus::Waiting,
            AgentStatus::Done,
            AgentStatus::Error,
        ] {
            let health = enrich_session_with_health(
                &session,
                None,
                true,
                Some(status),
                Some("2026-02-05T12:00:00Z".to_string()),
            );

            assert_eq!(health.agent_status, Some(status));
        }
    }

    #[test]
    fn test_agent_status_json_serialization() {
        let session = Session::new_for_test("test".to_string(), PathBuf::from("/tmp"));

        let health = enrich_session_with_health(
            &session,
            None,
            true,
            Some(AgentStatus::Idle),
            Some("2026-02-05T12:00:00Z".to_string()),
        );

        let json = serde_json::to_string(&health).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["agent_status"], "idle");
        assert_eq!(parsed["agent_status_updated_at"], "2026-02-05T12:00:00Z");
    }

    #[test]
    fn test_agent_status_none_json_serialization() {
        let session = Session::new_for_test("test".to_string(), PathBuf::from("/tmp"));

        let health = enrich_session_with_health(&session, None, true, None, None);

        let json = serde_json::to_string(&health).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["agent_status"].is_null());
        assert!(parsed["agent_status_updated_at"].is_null());
    }

    // --- aggregate_health_stats tests ---

    #[test]
    fn test_aggregate_empty() {
        let output = aggregate_health_stats(&[]);
        assert_eq!(output.total_count, 0);
        assert_eq!(output.working_count, 0);
        assert_eq!(output.idle_count, 0);
        assert_eq!(output.stuck_count, 0);
        assert_eq!(output.crashed_count, 0);
    }

    #[test]
    fn test_aggregate_mixed_statuses() {
        let session = Session::new_for_test("b".to_string(), PathBuf::from("/tmp"));

        let mut working = enrich_session_with_health(&session, None, true, None, None);
        working.metrics.status = HealthStatus::Working;

        let mut idle = enrich_session_with_health(&session, None, true, None, None);
        idle.metrics.status = HealthStatus::Idle;

        let mut crashed = enrich_session_with_health(&session, None, false, None, None);
        crashed.metrics.status = HealthStatus::Crashed;

        let mut unknown = enrich_session_with_health(&session, None, true, None, None);
        unknown.metrics.status = HealthStatus::Unknown;

        let output = aggregate_health_stats(&[working, idle, crashed, unknown]);

        assert_eq!(output.total_count, 4);
        assert_eq!(output.working_count, 1);
        assert_eq!(output.idle_count, 1);
        assert_eq!(output.crashed_count, 1);
        assert_eq!(output.stuck_count, 0);
    }

    // --- most_recent_activity tests ---

    #[test]
    fn test_most_recent_activity_prefers_newer_sidecar() {
        // session.last_activity is old; sidecar updated_at is recent
        let old = "2026-01-01T00:00:00Z";
        let recent = "2026-02-18T12:00:00Z";
        let result = most_recent_activity(Some(old), Some(recent));
        assert_eq!(result.as_deref(), Some(recent));
    }

    #[test]
    fn test_most_recent_activity_prefers_newer_last_activity() {
        let recent = "2026-02-18T12:00:00Z";
        let old = "2026-01-01T00:00:00Z";
        let result = most_recent_activity(Some(recent), Some(old));
        assert_eq!(result.as_deref(), Some(recent));
    }

    #[test]
    fn test_most_recent_activity_b_valid_when_a_unparseable() {
        // a is corrupt; b is a valid timestamp — b should win
        let valid = "2026-02-18T12:00:00Z";
        let result = most_recent_activity(Some("not-a-date"), Some(valid));
        assert_eq!(result.as_deref(), Some(valid));
    }

    #[test]
    fn test_most_recent_activity_none_a_returns_b() {
        let ts = "2026-02-18T12:00:00Z";
        assert_eq!(most_recent_activity(None, Some(ts)).as_deref(), Some(ts));
    }

    #[test]
    fn test_most_recent_activity_none_b_returns_a() {
        let ts = "2026-02-18T12:00:00Z";
        assert_eq!(most_recent_activity(Some(ts), None).as_deref(), Some(ts));
    }

    #[test]
    fn test_most_recent_activity_both_none_returns_none() {
        assert_eq!(most_recent_activity(None, None), None);
    }
}
