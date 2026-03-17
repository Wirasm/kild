/// Default HTTP hooks port for Claude Code `type: "http"` hooks.
/// Used by kild-daemon (bind), kild-config (config default), and kild-core (settings patching).
pub const DEFAULT_HOOKS_PORT: u16 = 19222;

#[cfg(unix)]
pub mod async_client;
#[cfg(unix)]
pub mod client;
pub mod env_cleanup;
mod messages;
#[cfg(unix)]
pub mod pool;
mod types;

#[cfg(unix)]
pub use async_client::AsyncIpcClient;
#[cfg(unix)]
pub use client::{IpcConnection, IpcError};
pub use messages::{ClientMessage, DaemonMessage, ErrorCode};
pub use types::{
    AgentMode, AgentStatus, BranchName, DaemonSessionStatus, ForgeType, OpenMode, ProjectId,
    RuntimeMode, SessionId, SessionStatus,
};
