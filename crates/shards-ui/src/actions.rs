//! Business logic handlers for shards-ui.
//!
//! This module contains functions that interact with shards-core
//! to perform operations like creating and listing shards.

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
