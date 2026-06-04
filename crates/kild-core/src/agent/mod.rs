//! `agent` slice — a reusable role: a name + system prompt you call upon.
//!
//! Stored as editable `<kild_home>/agents/<name>.md` files (the body *is* the
//! prompt). A session spawns *from* an agent: a non-default agent's prompt is
//! passed to pi via `--append-system-prompt`, layering on pi's default prompt and
//! the project's auto-loaded context. The built-in `default` agent has no prompt
//! (pi's own behavior) and always exists.

mod agent_errors;
mod agent_store;
mod agent_types;

pub use agent_errors::AgentError;
pub use agent_store::{add_agent, list_agents, prompt_file, DEFAULT_NAME};
pub use agent_types::Agent;
