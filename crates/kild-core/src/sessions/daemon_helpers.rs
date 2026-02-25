//! Re-export facade for daemon session helpers.
//!
//! The actual implementations live in focused per-concern modules:
//! - `attach` — terminal attach window spawning
//! - `daemon_request` — daemon PTY create request building
//! - `integrations/` — per-agent hook and config patching (claude, codex, opencode)
//! - `shim_setup` — tmux shim binary installation

// Generic daemon utilities
pub(super) use super::daemon_request::{
    build_daemon_create_request, compute_spawn_id, deliver_initial_prompt,
};

// Attach window
pub use super::attach::spawn_and_save_attach_window;

// Shim setup
pub(crate) use super::shim_setup::ensure_shim_binary;

// Agent integrations — setup orchestrators
pub(crate) use super::integrations::{
    setup_claude_integration, setup_codex_integration, setup_opencode_integration,
};

// Agent integrations — public ensure functions (used by CLI init-hooks)
pub use super::integrations::{
    ensure_claude_settings, ensure_claude_status_hook, ensure_opencode_config,
    ensure_opencode_package_json, ensure_opencode_plugin_in_worktree,
};
