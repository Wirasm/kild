//! `agent` slice — a reusable role: a name + system prompt you call upon.
//!
//! Agents are **read** from convention directories — a project's `.kild/agents/`,
//! `.claude/agents/`, `.pi/agents/`, plus global `~/.config/kild/agents/` and
//! `~/.claude/agents/`. A `<name>.md` file is an agent named `<name>`; its body
//! (frontmatter stripped) is the system prompt. This interops with Claude Code
//! and pi's own agent tooling — author an agent once, use it everywhere.
//!
//! A session spawns *from* an agent: a non-default agent's prompt is passed to pi
//! via `--append-system-prompt`, layering on pi's default prompt and the project's
//! auto-loaded context. The built-in `default` agent has no prompt.
//!
//! Authoring is just dropping a file (or, later, an agent doing it via a skill /
//! extension). kild only reads — it deliberately owns no creation API yet.

mod agent_errors;
mod agent_store;
mod agent_types;

pub use agent_errors::AgentError;
pub use agent_store::{list_agents, resolve_prompt, DEFAULT_NAME};
pub use agent_types::Agent;
