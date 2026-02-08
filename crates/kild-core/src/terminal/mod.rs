pub mod backends;
pub mod common;
pub mod errors;
pub mod handler;
pub mod operations;
pub mod registry;
pub mod traits;
pub mod types;

// Re-export commonly used types and functions
pub use errors::TerminalError;
pub use operations::{execute_spawn_script, is_terminal_window_open};
pub use registry::{detect_terminal, get_backend};
pub use traits::TerminalBackend;
pub use types::{SpawnConfig, TerminalType};
