use clap::ArgMatches;
use tracing::{error, info};

use kild_core::sessions::dropbox;

use super::helpers;

/// Handle `kild check-queue --self`.
///
/// Checks for queued work. If a task is available:
/// - Dequeues and writes it to the dropbox (task.md + inbox)
/// - Prints feedback to stderr
/// - Exits with code 2 (blocks TeammateIdle, teammate continues working)
///
/// If no work is queued: exits with code 0 (teammate goes idle normally).
pub(crate) fn handle_check_queue_command(
    matches: &ArgMatches,
) -> Result<(), Box<dyn std::error::Error>> {
    let _self_flag = matches.get_flag("self");

    let branch = std::env::var("KILD_SESSION_BRANCH").map_err(
        |_| "KILD_SESSION_BRANCH not set — --self requires running inside a kild session",
    )?;

    info!(event = "cli.check_queue_started", branch = %branch);

    let session = helpers::require_session(&branch, "cli.check_queue_failed")?;

    // Peek at queue first
    let queued = dropbox::peek_queue(&session.project_id, &session.branch).map_err(|e| {
        error!(event = "cli.check_queue_failed", branch = %branch, error = %e);
        Box::<dyn std::error::Error>::from(e)
    })?;

    if queued.is_none() {
        info!(event = "cli.check_queue_empty", branch = %branch);
        return Ok(());
    }

    // Dequeue and deliver
    let task_text = dropbox::dequeue_task(&session.project_id, &session.branch)
        .map_err(|e| {
            error!(event = "cli.check_queue_failed", branch = %branch, error = %e);
            Box::<dyn std::error::Error>::from(e)
        })?
        .ok_or("Queue was emptied between peek and dequeue")?;

    // Write task to dropbox (same as kild inject)
    use kild_core::sessions::dropbox::DeliveryMethod;
    let delivery_methods = vec![DeliveryMethod::Dropbox];
    let _ = dropbox::write_task(
        &session.project_id,
        &session.branch,
        &task_text,
        &delivery_methods,
    )
    .map_err(|e| {
        error!(event = "cli.check_queue_delivery_failed", branch = %branch, error = %e);
    });

    // For claude sessions, also write to inbox
    if session.agent == "claude" {
        let safe_name = kild_core::sessions::fleet::fleet_safe_name(&branch);
        let _ = kild_core::sessions::fleet::write_to_inbox(
            kild_core::sessions::fleet::BRAIN_BRANCH,
            &safe_name,
            &task_text,
        )
        .map_err(|e| {
            error!(event = "cli.check_queue_inbox_failed", branch = %branch, error = %e);
        });
    }

    eprintln!("Queued task delivered to '{}'", branch);
    info!(event = "cli.check_queue_delivered", branch = %branch);

    // Exit 2 = block the TeammateIdle event (teammate continues working)
    std::process::exit(2);
}
