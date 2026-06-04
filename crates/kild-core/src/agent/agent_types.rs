use serde::{Deserialize, Serialize};

/// A reusable agent role: a name and a system prompt.
///
/// Stored as an editable `<kild_home>/agents/<name>.md` file whose body *is* the
/// prompt. The built-in `default` agent has no file and an empty prompt — it runs
/// pi's own default behavior. A non-empty prompt is layered on pi's default via
/// `--append-system-prompt` (see [`crate::rpc::SpawnOptions`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Unique, user-facing name (also the `.md` filename).
    pub name: String,
    /// The role prompt. Empty for the built-in `default` agent.
    pub system_prompt: String,
}
