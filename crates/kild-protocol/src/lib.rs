#[cfg(unix)]
pub mod async_client;
#[cfg(unix)]
pub mod client;
pub mod env_cleanup;
mod messages;
mod types;

#[cfg(unix)]
pub use async_client::AsyncIpcClient;
#[cfg(unix)]
pub use client::{IpcConnection, IpcError};
pub use messages::{ClientMessage, DaemonMessage, ErrorCode};
pub use types::{
    AgentMode, AgentStatus, BranchName, ForgeType, OpenMode, ProjectId, RuntimeMode, SessionId,
    SessionInfo, SessionStatus,
};
