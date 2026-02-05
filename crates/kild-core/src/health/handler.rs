use crate::health::{errors::HealthError, operations, types::*};
use crate::process;
use crate::process::types::ProcessMetrics;
use crate::sessions;
use tracing::{info, warn};

/// Get health status for all sessions in current project
pub fn get_health_all_sessions() -> Result<HealthOutput, HealthError> {
    // Load config and apply thresholds (warn on errors, use defaults)
    match crate::config::KildConfig::load_hierarchy() {
        Ok(config) => {
            operations::set_idle_threshold_minutes(config.health.idle_threshold_minutes());
        }
        Err(e) => {
            warn!(
                event = "core.config.load_failed",
                error = %e,
                "Config load failed during health check, using default idle threshold"
            );
        }
    }

    info!(event = "core.health.get_all_started");

    let sessions = sessions::handler::list_sessions()?;
    let mut kild_healths = Vec::new();

    for session in sessions {
        let kild_health = enrich_session_with_metrics(&session);
        kild_healths.push(kild_health);
    }

    let output = operations::aggregate_health_stats(&kild_healths);

    info!(
        event = "core.health.get_all_completed",
        total = output.total_count,
        working = output.working_count,
        idle = output.idle_count,
        stuck = output.stuck_count,
        crashed = output.crashed_count
    );

    Ok(output)
}

/// Get health status for a specific session
pub fn get_health_single_session(branch: &str) -> Result<KildHealth, HealthError> {
    info!(event = "core.health.get_single_started", branch = branch);

    let session = sessions::handler::get_session(branch)?;
    let kild_health = enrich_session_with_metrics(&session);

    info!(
        event = "core.health.get_single_completed",
        branch = branch,
        status = ?kild_health.metrics.status
    );

    Ok(kild_health)
}

/// Helper to enrich session with process metrics
fn enrich_session_with_metrics(session: &sessions::types::Session) -> KildHealth {
    // Find first running agent for metrics (multi-agent path)
    let running_pid = session
        .agents()
        .iter()
        .filter_map(|a| a.process_id())
        .find(|&pid| matches!(process::is_process_running(pid), Ok(true)));

    let (process_metrics, process_running) = if let Some(pid) = running_pid {
        (get_metrics_for_pid(pid, &session.branch), true)
    } else {
        (None, false)
    };

    operations::enrich_session_with_health(session, process_metrics, process_running)
}

fn get_metrics_for_pid(pid: u32, branch: &str) -> Option<ProcessMetrics> {
    match process::get_process_metrics(pid) {
        Ok(metrics) => Some(metrics),
        Err(e) => {
            warn!(
                event = "core.health.process_metrics_failed",
                pid = pid,
                session_branch = branch,
                error = %e
            );
            None
        }
    }
}
