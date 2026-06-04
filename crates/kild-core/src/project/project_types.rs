use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A project: a directory an agent works in.
///
/// A session binds to the project's `path` as its cwd, so one agent sees every
/// service (repo/folder) under it. Those services are a later, additive concern
/// for git/worktree management — the primitive here is just `{ name, path }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Unique, user-facing name (the identifier).
    pub name: String,
    /// The project directory — a session's cwd.
    pub path: PathBuf,
}
