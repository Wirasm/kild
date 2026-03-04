//! ACP (Agent Client Protocol) relay support.
//!
//! Spawns ACP agent processes with plain stdio pipes (no PTY) and relays
//! bytes between the daemon's IPC clients and the agent's stdin/stdout.

pub mod relay;
