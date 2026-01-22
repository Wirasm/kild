//! shards-core: Core library for parallel AI agent worktree management
//!
//! This library provides the business logic for managing shards (isolated
//! git worktrees with AI agents). It is used by both the CLI and UI.
//!
//! # Main Entry Points
//!
//! - [`sessions`] - Create, list, destroy, restart sessions
//! - [`health`] - Monitor shard health and metrics
//! - [`cleanup`] - Clean up orphaned resources
//! - [`config`] - Configuration management
//! - [`agents`] - Agent backend management

pub mod agents;
pub mod cleanup;
pub mod config;
pub mod errors;
pub mod events;
pub mod files;
pub mod git;
pub mod health;
pub mod logging;
pub mod process;
pub mod sessions;
pub mod terminal;

// Re-export commonly used types at crate root for convenience
pub use config::ShardsConfig;
pub use sessions::types::{CreateSessionRequest, Session, SessionStatus};

// Re-export handler modules as the primary API
pub use cleanup::handler as cleanup_ops;
pub use health::handler as health_ops;
pub use sessions::handler as session_ops;
pub use terminal::handler as terminal_ops;

// Re-export logging initialization
pub use logging::init_logging;
