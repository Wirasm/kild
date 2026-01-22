use crate::health::{errors::HealthError, operations, types::*};
use crate::process;
use crate::sessions;
use tracing::{info, warn};

/// Get health status for all sessions in current project
pub fn get_health_all_sessions() -> Result<HealthOutput, HealthError> {
    // Load config and apply thresholds
    if let Ok(config) = crate::config::ShardsConfig::load_hierarchy() {
        operations::set_idle_threshold_minutes(config.health.idle_threshold_minutes());
    }

    info!(event = "health.get_all_started");

    let sessions = sessions::handler::list_sessions()?;
    let mut shard_healths = Vec::new();

    for session in sessions {
        let shard_health = enrich_session_with_metrics(&session);
        shard_healths.push(shard_health);
    }

    let output = operations::aggregate_health_stats(&shard_healths);

    info!(
        event = "health.get_all_completed",
        total = output.total_count,
        working = output.working_count,
        idle = output.idle_count,
        stuck = output.stuck_count,
        crashed = output.crashed_count
    );

    Ok(output)
}

/// Get health status for a specific session
pub fn get_health_single_session(branch: &str) -> Result<ShardHealth, HealthError> {
    info!(event = "health.get_single_started", branch = branch);

    let session = sessions::handler::get_session(branch)?;
    let shard_health = enrich_session_with_metrics(&session);

    info!(
        event = "health.get_single_completed",
        branch = branch,
        status = ?shard_health.metrics.status
    );

    Ok(shard_health)
}

/// Helper to enrich session with process metrics
fn enrich_session_with_metrics(session: &sessions::types::Session) -> ShardHealth {
    let (process_metrics, process_running) = if let Some(pid) = session.process_id {
        match process::is_process_running(pid) {
            Ok(true) => {
                let metrics = match process::get_process_metrics(pid) {
                    Ok(metrics) => Some(metrics),
                    Err(e) => {
                        warn!(
                            event = "health.process_metrics_failed",
                            pid = pid,
                            session_branch = &session.branch,
                            error = %e
                        );
                        None
                    }
                };
                (metrics, true)
            }
            Ok(false) => (None, false),
            Err(e) => {
                warn!(
                    event = "health.process_check_failed",
                    pid = pid,
                    session_branch = &session.branch,
                    error = %e
                );
                (None, false)
            }
        }
    } else {
        (None, false)
    };

    operations::enrich_session_with_health(session, process_metrics, process_running)
}
