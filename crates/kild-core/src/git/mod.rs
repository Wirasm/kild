pub mod cli;
pub mod errors;
pub mod handler;
pub mod operations;
pub mod remote;
pub mod removal;
pub mod types;

// Re-export commonly used types and functions
pub use errors::GitError;
pub use handler::{create_worktree, detect_project, detect_project_at};
pub use remote::{fetch_remote, rebase_worktree};
pub use removal::{remove_worktree, remove_worktree_by_path, remove_worktree_force};
pub use types::{DiffStats, GitStats, UncommittedDetails, WorktreeStatus};
