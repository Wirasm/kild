pub mod cli;
pub mod errors;
pub mod handler;
pub mod health;
pub mod naming;
pub mod overlaps;
pub mod query;
pub mod remote;
pub mod removal;
pub mod status;
pub mod types;
pub mod validation;

#[cfg(test)]
pub mod test_support;

// Re-export commonly used types and functions
pub use errors::GitError;
pub use handler::{create_worktree, detect_project, detect_project_at};
pub use health::collect_branch_health;
pub use naming::{
    KILD_BRANCH_PREFIX, calculate_worktree_path, derive_project_name_from_path,
    derive_project_name_from_remote, generate_project_id, kild_branch_name,
    kild_worktree_admin_name, sanitize_for_path,
};
pub use overlaps::collect_file_overlaps;
pub use query::{
    WorktreeEntry, delete_local_branch, ensure_in_repo, get_origin_url, has_any_remote,
    has_uncommitted_changes, head_branch_name, is_git_repo, is_worktree_valid,
    list_local_branch_names, list_worktree_entries, worktree_active_branches,
};
pub use remote::{fetch_remote, rebase_worktree};
pub use removal::{
    delete_branch_if_exists, find_main_repo_root, remove_worktree, remove_worktree_by_path,
    remove_worktree_force,
};
pub use status::{collect_git_stats, get_diff_stats, get_worktree_status};
pub use types::{
    BaseBranchDrift, BranchHealth, CleanKild, CommitActivity, ConflictStatus, DiffStats,
    FileOverlap, GitStats, OverlapReport, UncommittedDetails, WorktreeStatus,
};
pub use validation::{
    get_current_branch, is_valid_git_directory, should_use_current_branch, validate_branch_name,
    validate_git_arg,
};
