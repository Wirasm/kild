//! kild-core: Core library for parallel AI agent worktree management
//!
//! This library provides the business logic for managing kilds (isolated
//! git worktrees with AI agents). It is used by both the CLI and UI.
//!
//! # Main Entry Points
//!
//! - [`sessions`] - Create, list, destroy sessions
//! - [`health`] - Monitor kild health and metrics
//! - [`cleanup`] - Clean up orphaned resources
//! - [`config`] - Configuration management
//! - [`agents`] - Agent backend management

pub mod agents;
pub mod cleanup;
pub mod daemon;
pub mod editor;
pub mod errors;
pub mod escape;
pub mod events;
pub mod files;
pub mod forge;
pub mod git;
pub mod health;
pub mod logging;
pub mod notify;
pub mod process;
pub mod projects;
pub mod sessions;
pub mod state;
pub mod terminal;

// Re-export newtypes and shared domain enums from kild-protocol
pub use kild_protocol::{
    AgentMode, AgentStatus, BranchName, OpenMode, ProjectId, RuntimeMode, SessionId,
};

// Re-export config types from kild-config
pub use editor::{EditorBackend, EditorError, EditorType};
pub use forge::types::{
    CiStatus, MergeReadiness, MergeStrategy, PrCheckResult, PrState, PullRequest, ReviewStatus,
};
pub use forge::{ForgeBackend, ForgeError, ForgeType};
pub use git::types::{
    BaseBranchDrift, BranchHealth, CleanKild, CommitActivity, ConflictStatus, DiffStats,
    FileOverlap, GitStats, OverlapReport, UncommittedDetails, WorktreeStatus,
};
pub use kild_config::ConfigError;
pub use kild_config::{
    AgentConfig, AgentSettings, Config, DaemonRuntimeConfig, EditorConfig, GitConfig, HealthConfig,
    Keybindings, KildConfig, TerminalConfig, UiConfig, VALID_TERMINALS,
};
pub use kild_config::{CopyOptions, IncludeConfig, PatternRule};
pub use projects::{Project, ProjectError, ProjectManager, ProjectsData};
pub use sessions::agent_status::AgentStatusResult;
pub use sessions::info::SessionSnapshot;
pub use sessions::types::{
    AgentProcess, AgentStatusRecord, CompleteRequest, CompleteResult, CreateSessionRequest,
    DestroySafety, GitStatus, ProcessStatus, Session, SessionStatus,
};
pub use state::{Command, CoreStore, DispatchError, Event, Store};

// Re-export handler modules as the primary API
pub use cleanup::handler as cleanup_ops;
pub use health::handler as health_ops;
pub use sessions::handler as session_ops;
pub use terminal::handler as terminal_ops;

// Re-export logging initialization
pub use logging::init_logging;
