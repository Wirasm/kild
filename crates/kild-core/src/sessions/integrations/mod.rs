pub mod claude;
pub mod codex;
pub mod opencode;

// Re-export setup orchestrators used by create.rs and open.rs
pub(crate) use claude::setup_claude_integration;
pub(crate) use codex::setup_codex_integration;
pub(crate) use opencode::setup_opencode_integration;

// Re-export public functions used by CLI (init-hooks command)
pub use claude::{ensure_claude_settings, ensure_claude_status_hook};
pub use opencode::{
    ensure_opencode_config, ensure_opencode_package_json, ensure_opencode_plugin_in_worktree,
};
