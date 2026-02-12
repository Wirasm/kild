//! Re-export facade for session operations.
//!
//! All session operations are implemented in focused modules. This file
//! re-exports them to preserve the `session_ops::*` public API used by
//! lib.rs, dispatch.rs, and health/handler.rs.

// Operations
pub use super::create::create_session;
pub use super::list::{get_session, list_sessions, sync_daemon_session_status};
pub use super::open::open_session;
pub use super::stop::stop_session;

// Re-export from previously extracted modules
pub use super::agent_status::{
    AgentStatusResult, find_session_by_worktree_path, read_agent_status, update_agent_status,
};
pub use super::complete::{complete_session, fetch_pr_info, read_pr_info};
pub use super::destroy::{destroy_session, get_destroy_safety_info, has_remote_configured};
