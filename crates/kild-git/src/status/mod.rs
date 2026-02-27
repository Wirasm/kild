mod commits;
mod stats;
mod worktree;

pub use stats::collect_git_stats;
pub use worktree::{get_diff_stats, get_worktree_status};

// Re-export for internal use by sibling submodules
use commits::count_unpushed_commits;

#[cfg(test)]
mod tests;
