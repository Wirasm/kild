//! `rpc` slice — the only part of kild that knows `pi` exists.
//!
//! Drives a `pi --mode rpc` subprocess over its JSONL stdin/stdout protocol:
//! commands in ([`RpcCommand`]), structured events out ([`PiOutput`]). One
//! subprocess per worktree/agent.
//!
//! Keep this boundary narrow. Other slices must translate [`PiOutput`] into kild
//! domain types rather than passing pi shapes around — that is what keeps the
//! agent backbone swappable.

mod rpc_client;
mod rpc_errors;
mod rpc_types;

pub use rpc_client::{PiRpcSession, SpawnOptions};
pub use rpc_errors::RpcError;
pub use rpc_types::{AssistantDelta, PiOutput, RpcCommand};
