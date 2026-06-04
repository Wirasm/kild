//! kild-core — orchestration library for running `pi` coding agents in isolated
//! git worktrees.
//!
//! Organized as vertical slices: each slice owns its types, logic, and (only where
//! there is a real swap axis) a trait. Everything pi-specific lives behind the
//! [`rpc`] slice — no other slice may know the agent backend is pi, so the backbone
//! stays swappable.
//!
//! Slices:
//! - [`agent`] — a reusable role: a name + system prompt you call upon.
//! - [`paths`] — centralized filesystem paths for kild's own state (`~/.config/kild`).
//! - [`project`] — a project is a directory an agent works in (a session's cwd).
//! - [`rpc`] — the sole boundary to `pi`; drives `pi --mode rpc` over JSONL.

pub mod agent;
pub mod paths;
pub mod project;
pub mod rpc;
