use kild_protocol::{AgentMode, BranchName, RuntimeMode};
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
    pub agent_mode: AgentMode,
    pub note: Option<String>,
    /// Optional GitHub issue number linked to this kild.
    pub issue: Option<u32>,
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
    pub runtime_mode: RuntimeMode,
    /// Use the main project root as working directory instead of creating a worktree.
    /// Intended for the Honryū brain session and other supervisory agents that don't write code.
    pub use_main_worktree: bool,
    /// Optional prompt written to PTY stdin after the session is saved and the TUI settles.
    ///
    /// Only effective for daemon sessions (no `daemon_session_id` available for terminal sessions).
    /// Best-effort: session creation succeeds even if prompt delivery fails.
    /// May block up to 20s waiting for the agent's TUI to stabilize before injecting.
    pub initial_prompt: Option<String>,
    /// Override initial PTY rows (daemon sessions only).
    /// Takes precedence over config `[daemon] default_rows` and terminal ioctl.
    pub rows: Option<u16>,
    /// Override initial PTY columns (daemon sessions only).
    /// Takes precedence over config `[daemon] default_cols` and terminal ioctl.
    pub cols: Option<u16>,
}

impl CreateSessionRequest {
    pub fn new(branch: impl Into<BranchName>, agent_mode: AgentMode, note: Option<String>) -> Self {
        Self {
            branch: branch.into(),
            agent_mode,
            note,
            issue: None,
            project_path: None,
            base_branch: None,
            no_fetch: false,
            runtime_mode: RuntimeMode::Terminal,
            use_main_worktree: false,
            initial_prompt: None,
            rows: None,
            cols: None,
        }
    }

    /// Create a request with explicit project path (for UI usage)
    pub fn with_project_path(
        branch: impl Into<BranchName>,
        agent_mode: AgentMode,
        note: Option<String>,
        project_path: PathBuf,
    ) -> Self {
        Self {
            branch: branch.into(),
            agent_mode,
            note,
            issue: None,
            project_path: Some(project_path),
            base_branch: None,
            no_fetch: false,
            runtime_mode: RuntimeMode::Terminal,
            use_main_worktree: false,
            initial_prompt: None,
            rows: None,
            cols: None,
        }
    }

    pub fn with_issue(mut self, issue: Option<u32>) -> Self {
        self.issue = issue;
        self
    }

    pub fn with_main_worktree(mut self, use_main: bool) -> Self {
        self.use_main_worktree = use_main;
        self
    }

    pub fn with_base_branch(mut self, base_branch: Option<String>) -> Self {
        self.base_branch = base_branch;
        self
    }

    pub fn with_no_fetch(mut self, no_fetch: bool) -> Self {
        self.no_fetch = no_fetch;
        self
    }

    pub fn with_runtime_mode(mut self, mode: RuntimeMode) -> Self {
        self.runtime_mode = mode;
        self
    }

    pub fn with_initial_prompt(mut self, prompt: Option<String>) -> Self {
        self.initial_prompt = prompt;
        self
    }

    pub fn with_pty_size(mut self, rows: Option<u16>, cols: Option<u16>) -> Self {
        self.rows = rows;
        self.cols = cols;
        self
    }
}

/// Parameters for opening an agent in an existing kild session.
#[derive(Debug, Clone)]
pub struct OpenSessionRequest {
    pub name: String,
    pub mode: kild_protocol::OpenMode,
    /// Runtime mode explicitly requested via CLI flags.
    /// `None` = no flag passed; auto-detect from session's stored mode, then config.
    pub runtime_mode: Option<RuntimeMode>,
    /// Resume the previous agent conversation instead of starting fresh.
    pub resume: bool,
    /// Enable full autonomy mode (skip all permission prompts).
    pub yolo: bool,
    /// Don't open an attach window for daemon sessions.
    pub no_attach: bool,
    /// Optional prompt written to PTY stdin after the agent's TUI settles.
    pub initial_prompt: Option<String>,
    /// Override initial PTY rows (daemon sessions only).
    pub rows: Option<u16>,
    /// Override initial PTY columns (daemon sessions only).
    pub cols: Option<u16>,
}

impl OpenSessionRequest {
    pub fn new(name: impl Into<String>, mode: kild_protocol::OpenMode) -> Self {
        Self {
            name: name.into(),
            mode,
            runtime_mode: None,
            resume: false,
            yolo: false,
            no_attach: false,
            initial_prompt: None,
            rows: None,
            cols: None,
        }
    }

    pub fn with_runtime_mode(mut self, mode: Option<RuntimeMode>) -> Self {
        self.runtime_mode = mode;
        self
    }

    pub fn with_resume(mut self, resume: bool) -> Self {
        self.resume = resume;
        self
    }

    pub fn with_yolo(mut self, yolo: bool) -> Self {
        self.yolo = yolo;
        self
    }

    pub fn with_no_attach(mut self, no_attach: bool) -> Self {
        self.no_attach = no_attach;
        self
    }

    pub fn with_initial_prompt(mut self, prompt: Option<String>) -> Self {
        self.initial_prompt = prompt;
        self
    }

    pub fn with_pty_size(mut self, rows: Option<u16>, cols: Option<u16>) -> Self {
        self.rows = rows;
        self.cols = cols;
        self
    }
}
