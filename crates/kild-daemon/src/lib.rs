pub mod client;
pub mod errors;
pub mod pane_backend;
pub mod pid;
pub mod protocol;
pub mod pty;
pub mod server;
pub mod session;
pub mod types;

// Primary re-exports
pub use client::DaemonClient;
pub use errors::DaemonError;
pub use protocol::messages::{ClientMessage, DaemonMessage};
pub use server::run_server;
pub use types::{DaemonConfig, DaemonStatus, SessionInfo, load_daemon_config};
