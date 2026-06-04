//! kild-core — orchestration library for running `pi` coding agents in isolated
//! git worktrees.
//!
//! Organized as vertical slices: each slice owns its types, logic, and (only where
//! there is a real swap axis) a trait. Everything pi-specific lives behind the
//! [`rpc`] slice — no other slice may know the agent backend is pi, so the backbone
//! stays swappable.
//!
//! Slices:
//! - [`rpc`] — the sole boundary to `pi`; drives `pi --mode rpc` over JSONL.

pub mod rpc;
