pub mod agent_status;
pub mod complete;
pub mod destroy;
pub mod errors;
pub mod handler;
pub mod info;
pub mod persistence;
pub mod ports;
pub mod store;
pub mod types;
pub mod validation;

// Re-export commonly used types and functions
pub use agent_status::{find_session_by_worktree_path, read_agent_status, update_agent_status};
pub use complete::complete_session;
pub use destroy::{destroy_session, get_destroy_safety_info};
pub use errors::SessionError;
pub use handler::{create_session, get_session, list_sessions, open_session, stop_session};
pub use info::SessionInfo;
pub use types::{
    AgentProcess, AgentStatus, AgentStatusInfo, CompleteResult, CreateSessionRequest,
    DestroySafetyInfo, GitStatus, ProcessStatus, Session, SessionStatus,
};
