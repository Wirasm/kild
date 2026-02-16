//! Environment variable cleanup for spawned agent sessions.
//!
//! When kild is invoked from inside an existing agent session (e.g. Claude Code),
//! the parent's nesting-detection env vars leak into spawned terminals and daemon
//! PTYs, causing agents to refuse to start. This module defines the vars to strip.
//!
//! Lives in kild-protocol so both kild-core (terminal mode) and kild-daemon (PTY
//! mode) share a single source of truth.

/// Environment variables to remove when spawning agent sessions.
///
/// These are nesting-detection vars set by AI agents to prevent accidental
/// recursive launches. KILD intentionally spawns isolated sessions, so these
/// must be stripped.
///
/// Entries must be valid POSIX environment variable names: non-empty, containing
/// only ASCII alphanumerics and underscores. This is enforced by tests.
pub const ENV_VARS_TO_STRIP: &[&str] = &[
    // Claude Code sets this to detect nested sessions
    "CLAUDECODE",
];
