//! Business logic handlers for shards-ui.
//!
//! This module contains functions that interact with shards-core
//! to perform operations like creating, destroying, relaunching, and listing shards.

use shards_core::{CreateSessionRequest, Session, ShardsConfig, session_ops};

use crate::state::ShardDisplay;

/// Create a new shard with the given branch name and agent.
///
/// Returns the created session on success, or an error message on failure.
pub fn create_shard(branch: &str, agent: &str) -> Result<Session, String> {
    tracing::info!(
        event = "ui.create_shard.started",
        branch = branch,
        agent = agent
    );

    if branch.trim().is_empty() {
        tracing::warn!(
            event = "ui.create_dialog.validation_failed",
            reason = "empty branch name"
        );
        return Err("Branch name cannot be empty".to_string());
    }

    let config = match ShardsConfig::load_hierarchy() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(
                event = "ui.create_shard.config_load_failed",
                error = %e
            );
            return Err(format!("Failed to load config: {e}"));
        }
    };

    let request = CreateSessionRequest::new(branch.to_string(), Some(agent.to_string()));

    match session_ops::create_session(request, &config) {
        Ok(session) => {
            tracing::info!(
                event = "ui.create_shard.completed",
                session_id = session.id,
                branch = session.branch
            );
            Ok(session)
        }
        Err(e) => {
            tracing::error!(
                event = "ui.create_shard.failed",
                branch = branch,
                agent = agent,
                error = %e
            );
            Err(e.to_string())
        }
    }
}

/// Refresh the list of sessions from disk.
///
/// Returns `(displays, error)` where `error` is `Some` if session loading failed.
pub fn refresh_sessions() -> (Vec<ShardDisplay>, Option<String>) {
    tracing::info!(event = "ui.refresh_sessions.started");

    match session_ops::list_sessions() {
        Ok(sessions) => {
            let displays = sessions
                .into_iter()
                .map(ShardDisplay::from_session)
                .collect();
            tracing::info!(event = "ui.refresh_sessions.completed");
            (displays, None)
        }
        Err(e) => {
            tracing::error!(event = "ui.refresh_sessions.failed", error = %e);
            (Vec::new(), Some(e.to_string()))
        }
    }
}

/// Destroy a shard by branch name.
///
/// Thin wrapper around shards-core's `destroy_session`, which handles
/// terminal cleanup, process termination, worktree removal, and session file deletion.
pub fn destroy_shard(branch: &str) -> Result<(), String> {
    tracing::info!(event = "ui.destroy_shard.started", branch = branch);

    match session_ops::destroy_session(branch, false) {
        Ok(()) => {
            tracing::info!(event = "ui.destroy_shard.completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            tracing::error!(event = "ui.destroy_shard.failed", branch = branch, error = %e);
            Err(e.to_string())
        }
    }
}

/// Open a new agent terminal in an existing shard (additive - doesn't close existing terminals).
///
/// Unlike relaunch, this does NOT close existing terminals - multiple agents can run in the same shard.
pub fn open_shard(branch: &str, agent: Option<String>) -> Result<Session, String> {
    tracing::info!(event = "ui.open_shard.started", branch = branch, agent = ?agent);

    match session_ops::open_session(branch, agent) {
        Ok(session) => {
            tracing::info!(
                event = "ui.open_shard.completed",
                branch = branch,
                process_id = session.process_id
            );
            Ok(session)
        }
        Err(e) => {
            tracing::error!(event = "ui.open_shard.failed", branch = branch, error = %e);
            Err(e.to_string())
        }
    }
}

/// Stop the agent process in a shard without destroying the shard.
///
/// The worktree and session file are preserved. The shard can be reopened with open_shard().
pub fn stop_shard(branch: &str) -> Result<(), String> {
    tracing::info!(event = "ui.stop_shard.started", branch = branch);

    match session_ops::stop_session(branch) {
        Ok(()) => {
            tracing::info!(event = "ui.stop_shard.completed", branch = branch);
            Ok(())
        }
        Err(e) => {
            tracing::error!(event = "ui.stop_shard.failed", branch = branch, error = %e);
            Err(e.to_string())
        }
    }
}
