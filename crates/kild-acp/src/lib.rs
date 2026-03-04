//! KILD ACP client library.
//!
//! Wraps the `agent-client-protocol` SDK behind a thread-safe (`Send + Sync`)
//! API. The SDK produces `!Send` futures, so this crate spawns a dedicated
//! single-threaded tokio runtime on its own OS thread. External callers
//! communicate via mpsc channels through the [`AcpHandle`].
//!
//! # Architecture
//!
//! ```text
//! Caller (any thread)          AcpHandle          ACP Runtime Thread
//!   spawn(config) ──────────→  cmd_tx/event_rx    LocalSet + tokio current_thread
//!   handle.recv() ←────────── event_rx ←───────── KildAcpClient::session_notification
//!   handle.shutdown() ──────→ cmd_tx ────────────→ child.kill()
//! ```
//!
//! # Usage
//!
//! ```ignore
//! let config = AcpSpawnConfig {
//!     binary: "claude-code-acp".to_string(),
//!     args: vec![],
//!     working_directory: "/path/to/project".into(),
//!     env_vars: Default::default(),
//! };
//! let mut handle = kild_acp::spawn(config)?;
//! while let Some(event) = handle.recv().await {
//!     match event {
//!         AcpEvent::SessionNotification { payload } => { /* ... */ }
//!         AcpEvent::ProcessExited { exit_code } => break,
//!         _ => {}
//!     }
//! }
//! ```

mod client_impl;
mod connection;
pub mod errors;
pub mod runner;
pub mod types;

pub use errors::AcpError;
pub use runner::{AcpHandle, spawn};
pub use types::{AcpEvent, AcpSpawnConfig};
