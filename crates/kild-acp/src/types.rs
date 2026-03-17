use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration for spawning an ACP agent process.
#[derive(Debug, Clone)]
pub struct AcpSpawnConfig {
    /// The binary to execute (e.g., "claude-code-acp", "opencode").
    pub binary: String,
    /// Arguments to pass to the binary (e.g., ["acp"]).
    pub args: Vec<String>,
    /// Working directory for the agent process.
    pub working_directory: PathBuf,
    /// Environment variables to set for the agent process.
    pub env_vars: HashMap<String, String>,
}

/// Commands sent from the external caller to the ACP runtime thread.
#[derive(Debug)]
pub(crate) enum AcpCommand {
    /// Shut down the ACP connection and agent process.
    Shutdown,
}

/// Events emitted by the ACP runtime thread to the external caller.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AcpEvent {
    /// The ACP agent process has exited.
    ProcessExited { exit_code: Option<i32> },

    /// A session notification was received from the agent.
    SessionNotification { payload: String },

    /// A permission request was received from the agent.
    PermissionRequest { payload: String },

    /// The ACP connection encountered an error.
    Error { message: String },
}
