//! `project` slice — a project is a directory an agent works in.
//!
//! A session binds to a project's `path` as its cwd. Services (multiple
//! repos/folders under the project) are a later, additive concern for git and
//! worktree management; the primitive here is just `{ name, path }`, persisted
//! to `~/.config/kild/projects.json`.

mod project_errors;
mod project_store;
mod project_types;

pub use project_errors::ProjectError;
pub use project_store::{add_project, load_projects, remove_project};
pub use project_types::Project;
