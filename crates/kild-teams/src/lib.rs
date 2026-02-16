//! Agent team discovery and state management for KILD UI.
//!
//! Standalone library that understands Claude Code agent teams.
//! Reads team configs from `~/.claude/teams/` and cross-references
//! with shim pane registries at `~/.kild/shim/` to map teammates
//! to daemon PTY sessions.

pub mod discovery;
pub mod errors;
pub mod mapper;
pub mod parser;
pub mod scanner;
pub mod types;
pub mod watcher;

pub use errors::TeamsError;
pub use types::*;
pub use watcher::TeamWatcher;
