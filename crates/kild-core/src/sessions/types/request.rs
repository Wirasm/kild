use kild_protocol::BranchName;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ValidatedRequest {
    pub name: BranchName,
    pub command: String,
    pub agent: String,
}

#[derive(Debug, Clone)]
pub struct CreateSessionRequest {
    pub branch: BranchName,
    /// What agent to launch (default from config, specific override, or bare shell).
    pub agent_mode: crate::state::types::AgentMode,
    pub note: Option<String>,
    /// Optional project path for UI context. When provided, this path is used
    /// instead of current working directory for project detection.
    ///
    /// See [`crate::sessions::handler::create_session`] for the branching logic.
    pub project_path: Option<PathBuf>,
    /// Override base branch for this create (CLI --base flag).
    pub base_branch: Option<String>,
    /// Skip fetching before create (CLI --no-fetch flag).
    pub no_fetch: bool,
    /// Whether to launch in an external terminal or daemon-owned PTY.
    pub runtime_mode: crate::state::types::RuntimeMode,
}

impl CreateSessionRequest {
    pub fn new(
        branch: impl Into<BranchName>,
        agent_mode: crate::state::types::AgentMode,
        note: Option<String>,
    ) -> Self {
        Self {
            branch: branch.into(),
            agent_mode,
            note,
            project_path: None,
            base_branch: None,
            no_fetch: false,
            runtime_mode: crate::state::types::RuntimeMode::Terminal,
        }
    }

    /// Create a request with explicit project path (for UI usage)
    pub fn with_project_path(
        branch: impl Into<BranchName>,
        agent_mode: crate::state::types::AgentMode,
        note: Option<String>,
        project_path: PathBuf,
    ) -> Self {
        Self {
            branch: branch.into(),
            agent_mode,
            note,
            project_path: Some(project_path),
            base_branch: None,
            no_fetch: false,
            runtime_mode: crate::state::types::RuntimeMode::Terminal,
        }
    }

    pub fn with_base_branch(mut self, base_branch: Option<String>) -> Self {
        self.base_branch = base_branch;
        self
    }

    pub fn with_no_fetch(mut self, no_fetch: bool) -> Self {
        self.no_fetch = no_fetch;
        self
    }

    pub fn with_runtime_mode(mut self, mode: crate::state::types::RuntimeMode) -> Self {
        self.runtime_mode = mode;
        self
    }
}
