use crate::forge::types::{CiStatus, PrInfo};
use crate::git::errors::GitError;
use crate::git::types::{
    BaseBranchDrift, BranchHealth, CleanKild, CommitActivity, CommitCounts, ConflictStatus,
    DiffStats, FileOverlap, GitStats, MergeReadiness, OverlapReport, UncommittedDetails,
    WorktreeStatus,
};
use git2::{Oid, Repository, Status, StatusOptions};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Sanitize a string for safe use in filesystem paths and git2 worktree names.
///
/// Replaces `/` with `-` to prevent nested directory creation. Git branch names
/// like `feature/foo` are valid, but git2's `repo.worktree()` treats the name
/// parameter as a directory name under `.git/worktrees/`, interpreting slashes
/// as path separators and attempting to create nested directories.
///
/// The `-` replacement matches the pattern in `process/pid_file.rs`.
pub fn sanitize_for_path(s: &str) -> String {
    s.replace('/', "-")
}

/// The git branch namespace prefix used by KILD for worktree branches.
pub const KILD_BRANCH_PREFIX: &str = "kild/";

/// Constructs the KILD branch name for a given user branch name.
///
/// Example: `"my-feature"` → `"kild/my-feature"`
pub fn kild_branch_name(branch: &str) -> String {
    format!("kild/{branch}")
}

/// Constructs the worktree admin name (flat, filesystem-safe) for a given user branch name.
///
/// The admin name is used for the `.git/worktrees/<name>` directory, which does not
/// support slashes. This is decoupled from the branch name via `WorktreeAddOptions::reference()`.
///
/// Examples:
/// - `"my-feature"` → `"kild-my-feature"`
/// - `"feature/auth"` → `"kild-feature-auth"`
pub fn kild_worktree_admin_name(branch: &str) -> String {
    format!("kild-{}", sanitize_for_path(branch))
}

pub fn calculate_worktree_path(base_dir: &Path, project_name: &str, branch: &str) -> PathBuf {
    let safe_branch = sanitize_for_path(branch);
    base_dir
        .join("worktrees")
        .join(project_name)
        .join(safe_branch)
}

pub fn derive_project_name_from_path(repo_path: &Path) -> String {
    repo_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string()
}

pub fn derive_project_name_from_remote(remote_url: &str) -> String {
    // Extract repo name from URLs like:
    // https://github.com/user/repo.git -> repo
    // git@github.com:user/repo.git -> repo

    let url = remote_url.trim_end_matches(".git");

    if let Some(last_slash) = url.rfind('/') {
        url[last_slash + 1..].to_string()
    } else if let Some(colon) = url.rfind(':') {
        if let Some(slash) = url[colon..].find('/') {
            url[colon + slash + 1..].to_string()
        } else {
            url[colon + 1..].to_string()
        }
    } else {
        "unknown".to_string()
    }
}

pub fn generate_project_id(repo_path: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    repo_path.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

pub fn validate_branch_name(branch: &str) -> Result<String, GitError> {
    let trimmed = branch.trim();

    if trimmed.is_empty() {
        return Err(GitError::OperationFailed {
            message: "Branch name cannot be empty".to_string(),
        });
    }

    // Git branch name validation rules
    if trimmed.contains("..")
        || trimmed.starts_with('-')
        || trimmed.contains(' ')
        || trimmed.contains('\t')
        || trimmed.contains('\n')
    {
        return Err(GitError::OperationFailed {
            message: format!("Invalid branch name: '{}'", trimmed),
        });
    }

    Ok(trimmed.to_string())
}

/// Validate a git argument to prevent injection.
///
/// Rejects values that start with `-` (option injection), contain control characters,
/// or contain `::` sequences (refspec injection).
pub fn validate_git_arg(value: &str, label: &str) -> Result<(), GitError> {
    if value.starts_with('-') {
        return Err(GitError::OperationFailed {
            message: format!("Invalid {label}: '{value}' (must not start with '-')"),
        });
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(GitError::OperationFailed {
            message: format!("Invalid {label}: contains control characters"),
        });
    }
    if value.contains("::") {
        return Err(GitError::OperationFailed {
            message: format!("Invalid {label}: '::' sequences are not allowed"),
        });
    }
    Ok(())
}

/// Gets the current branch name from the repository.
///
/// Returns `None` if the repository is in a detached HEAD state.
///
/// # Errors
/// Returns `GitError::Git2Error` if the repository HEAD cannot be accessed.
pub fn get_current_branch(repo: &git2::Repository) -> Result<Option<String>, GitError> {
    let head = repo.head().map_err(|e| GitError::Git2Error { source: e })?;

    if let Some(branch_name) = head.shorthand() {
        Ok(Some(branch_name.to_string()))
    } else {
        // Detached HEAD state - no current branch
        debug!("Repository is in detached HEAD state, no current branch available");
        Ok(None)
    }
}

/// Determines if the current branch should be used for the worktree.
///
/// Returns `true` if the current branch name exactly matches the requested branch name.
pub fn should_use_current_branch(current_branch: &str, requested_branch: &str) -> bool {
    current_branch == requested_branch
}

pub fn is_valid_git_directory(path: &Path) -> bool {
    path.join(".git").exists()
}

/// Get diff statistics for unstaged changes in a worktree.
///
/// Returns the number of insertions, deletions, and files changed
/// between the index (staging area) and the working directory.
/// This does not include staged changes.
///
/// # Errors
///
/// Returns `GitError::Git2Error` if the repository cannot be opened
/// or the diff cannot be computed.
pub fn get_diff_stats(worktree_path: &Path) -> Result<DiffStats, GitError> {
    let repo = Repository::open(worktree_path).map_err(|e| GitError::Git2Error { source: e })?;

    let diff = repo
        .diff_index_to_workdir(None, None)
        .map_err(|e| GitError::Git2Error { source: e })?;

    let stats = diff
        .stats()
        .map_err(|e| GitError::Git2Error { source: e })?;

    Ok(DiffStats {
        insertions: stats.insertions(),
        deletions: stats.deletions(),
        files_changed: stats.files_changed(),
    })
}

/// Get comprehensive worktree status for destroy safety checks.
///
/// Returns information about:
/// - Uncommitted changes (staged, modified, untracked files)
/// - Unpushed commits (commits ahead of remote tracking branch)
/// - Remote branch existence
///
/// # Conservative Fallback
///
/// If status checks fail, the function returns a conservative fallback that
/// assumes uncommitted changes exist. This prevents data loss by requiring
/// the user to verify manually. Check `status_check_failed` to detect this.
///
/// # Errors
///
/// Returns `GitError::Git2Error` if the repository cannot be opened.
pub fn get_worktree_status(worktree_path: &Path) -> Result<WorktreeStatus, GitError> {
    let repo = Repository::open(worktree_path).map_err(|e| GitError::Git2Error { source: e })?;

    // 1. Check for uncommitted changes using git2 status
    let (uncommitted_result, status_check_failed) = check_uncommitted_changes(&repo);

    // 2. Count unpushed/behind commits and check remote branch existence
    let commit_counts = count_unpushed_commits(&repo);

    // Determine if there are uncommitted changes
    // Conservative fallback: assume dirty if check failed
    let has_uncommitted = if let Some(details) = &uncommitted_result {
        !details.is_empty()
    } else {
        true
    };

    Ok(WorktreeStatus {
        has_uncommitted_changes: has_uncommitted,
        unpushed_commit_count: commit_counts.ahead,
        behind_commit_count: commit_counts.behind,
        has_remote_branch: commit_counts.has_remote,
        uncommitted_details: uncommitted_result,
        behind_count_failed: commit_counts.behind_count_failed,
        status_check_failed,
    })
}

/// Check for uncommitted changes in the repository.
///
/// Returns (Option<details>, status_check_failed).
/// - `Some(details)` with file counts when check succeeds
/// - `None` when check fails (status_check_failed will be true)
///
/// The caller should treat `None` as "assume uncommitted changes exist"
/// to be conservative and prevent data loss.
fn check_uncommitted_changes(repo: &Repository) -> (Option<UncommittedDetails>, bool) {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true);
    opts.include_ignored(false);

    let statuses = match repo.statuses(Some(&mut opts)) {
        Ok(s) => s,
        Err(e) => {
            warn!(
                event = "core.git.status_check_failed",
                error = %e,
                "Failed to get git status - assuming dirty to be safe"
            );
            // Return None to indicate check failed, true for status_check_failed
            return (None, true);
        }
    };

    let mut staged_files = 0;
    let mut modified_files = 0;
    let mut untracked_files = 0;

    for entry in statuses.iter() {
        let status = entry.status();

        // Check for staged changes (index changes)
        if status.intersects(
            Status::INDEX_NEW
                | Status::INDEX_MODIFIED
                | Status::INDEX_DELETED
                | Status::INDEX_RENAMED
                | Status::INDEX_TYPECHANGE,
        ) {
            staged_files += 1;
        }

        // Check for unstaged modifications to tracked files
        if status.intersects(
            Status::WT_MODIFIED | Status::WT_DELETED | Status::WT_RENAMED | Status::WT_TYPECHANGE,
        ) {
            modified_files += 1;
        }

        // Check for untracked files
        if status.contains(Status::WT_NEW) {
            untracked_files += 1;
        }
    }

    let details = UncommittedDetails {
        staged_files,
        modified_files,
        untracked_files,
    };

    // Return Some(details) even if empty - caller uses is_empty() to check
    (Some(details), false)
}

/// Count unpushed and behind commits and check if remote tracking branch exists.
fn count_unpushed_commits(repo: &Repository) -> CommitCounts {
    // Get current branch reference
    let head = match repo.head() {
        Ok(h) => h,
        Err(e) => {
            warn!(
                event = "core.git.head_read_failed",
                error = %e,
                "Failed to read HEAD - cannot count unpushed commits"
            );
            return CommitCounts::default();
        }
    };

    // Get the branch name
    let branch_name = match head.shorthand() {
        Some(name) => name,
        None => {
            // Detached HEAD is a normal state, not an error
            debug!(
                event = "core.git.detached_head",
                "Repository is in detached HEAD state"
            );
            return CommitCounts::default();
        }
    };

    // Find the local branch
    let local_branch = match repo.find_branch(branch_name, git2::BranchType::Local) {
        Ok(b) => b,
        Err(e) => {
            warn!(
                event = "core.git.local_branch_not_found",
                branch = branch_name,
                error = %e,
                "Could not find local branch"
            );
            return CommitCounts::default();
        }
    };

    // Check if there's an upstream (remote tracking) branch
    let upstream = match local_branch.upstream() {
        Ok(u) => u,
        Err(_) => {
            // No upstream configured - branch has never been pushed
            // This is expected for new branches, not an error
            debug!(
                event = "core.git.no_upstream",
                branch = branch_name,
                "Branch has no upstream - never pushed"
            );
            return CommitCounts::default();
        }
    };

    // Get the OIDs for local and remote
    let local_oid = match head.target() {
        Some(oid) => oid,
        None => {
            warn!(
                event = "core.git.head_target_missing",
                branch = branch_name,
                "HEAD has no target OID"
            );
            return CommitCounts {
                has_remote: true,
                ..Default::default()
            };
        }
    };

    let upstream_oid = match upstream.get().target() {
        Some(oid) => oid,
        None => {
            warn!(
                event = "core.git.upstream_target_missing",
                branch = branch_name,
                "Upstream branch has no target OID"
            );
            return CommitCounts {
                has_remote: true,
                ..Default::default()
            };
        }
    };

    // Count commits ahead (local has, upstream doesn't)
    let mut ahead_walk = match repo.revwalk() {
        Ok(rw) => rw,
        Err(e) => {
            warn!(
                event = "core.git.revwalk_init_failed",
                error = %e,
                "Failed to create revwalk - cannot count unpushed commits"
            );
            return CommitCounts {
                has_remote: true,
                ..Default::default()
            };
        }
    };

    if let Err(e) = ahead_walk.push(local_oid) {
        warn!(
            event = "core.git.revwalk_push_failed",
            error = %e,
            "Failed to push local commit to revwalk"
        );
        return CommitCounts {
            has_remote: true,
            ..Default::default()
        };
    }
    if let Err(e) = ahead_walk.hide(upstream_oid) {
        warn!(
            event = "core.git.revwalk_hide_failed",
            error = %e,
            "Failed to hide upstream commit - history may have diverged"
        );
        return CommitCounts {
            has_remote: true,
            ..Default::default()
        };
    }

    let unpushed_count = ahead_walk.count();

    // Count commits behind (upstream has, local doesn't)
    let mut behind_walk = match repo.revwalk() {
        Ok(rw) => rw,
        Err(e) => {
            warn!(
                event = "core.git.behind_revwalk_init_failed",
                error = %e,
                "Failed to create revwalk for behind count"
            );
            return CommitCounts {
                ahead: unpushed_count,
                has_remote: true,
                behind_count_failed: true,
                ..Default::default()
            };
        }
    };

    if let Err(e) = behind_walk.push(upstream_oid) {
        warn!(
            event = "core.git.behind_revwalk_push_failed",
            error = %e,
            "Failed to push upstream commit to behind revwalk"
        );
        return CommitCounts {
            ahead: unpushed_count,
            has_remote: true,
            behind_count_failed: true,
            ..Default::default()
        };
    }
    if let Err(e) = behind_walk.hide(local_oid) {
        warn!(
            event = "core.git.behind_revwalk_hide_failed",
            error = %e,
            "Failed to hide local commit in behind revwalk"
        );
        return CommitCounts {
            ahead: unpushed_count,
            has_remote: true,
            behind_count_failed: true,
            ..Default::default()
        };
    }

    let behind_count = behind_walk.count();

    CommitCounts {
        ahead: unpushed_count,
        behind: behind_count,
        has_remote: true,
        behind_count_failed: false,
    }
}

/// Collect aggregated git stats for a worktree.
///
/// Returns `None` if the worktree path doesn't exist.
/// Individual stat failures are logged as warnings and degraded to `None`
/// fields rather than failing the entire operation.
pub fn collect_git_stats(worktree_path: &Path, branch: &str) -> Option<GitStats> {
    if !worktree_path.exists() {
        return None;
    }

    let diff = match get_diff_stats(worktree_path) {
        Ok(d) => Some(d),
        Err(e) => {
            warn!(
                event = "core.git.stats.diff_failed",
                branch = branch,
                error = %e
            );
            None
        }
    };

    let status = match get_worktree_status(worktree_path) {
        Ok(s) => Some(s),
        Err(e) => {
            warn!(
                event = "core.git.stats.worktree_status_failed",
                branch = branch,
                error = %e
            );
            None
        }
    };

    Some(GitStats {
        diff_stats: diff,
        worktree_status: status,
    })
}

// --- Branch Health Operations ---

/// Find the merge base between two commits.
///
/// Returns `None` if no common ancestor exists (e.g., unrelated histories).
fn find_merge_base(repo: &Repository, oid_a: Oid, oid_b: Oid) -> Option<Oid> {
    match repo.merge_base(oid_a, oid_b) {
        Ok(oid) => Some(oid),
        Err(e) => {
            debug!(
                event = "core.git.stats.merge_base_not_found",
                error = %e
            );
            None
        }
    }
}

/// Count commits reachable from `branch_oid` but not from `base_oid`.
///
/// Returns 0 on revwalk failure (errors are logged as warnings).
/// Callers cannot distinguish between "no commits" and "check failed".
fn count_commits_since(repo: &Repository, branch_oid: Oid, base_oid: Oid) -> usize {
    let mut walk = match repo.revwalk() {
        Ok(rw) => rw,
        Err(e) => {
            warn!(event = "core.git.stats.revwalk_init_failed", error = %e);
            return 0;
        }
    };
    if let Err(e) = walk.push(branch_oid) {
        warn!(event = "core.git.stats.revwalk_push_failed", error = %e);
        return 0;
    }
    if let Err(e) = walk.hide(base_oid) {
        warn!(event = "core.git.stats.revwalk_hide_failed", error = %e);
        return 0;
    }
    walk.count()
}

/// Get the last commit time on HEAD as RFC3339.
///
/// Returns the commit time converted to UTC. Returns `None` if HEAD
/// cannot be resolved or the timestamp is invalid.
fn get_last_commit_time(repo: &Repository) -> Option<String> {
    let head = match repo.head() {
        Ok(h) => h,
        Err(e) => {
            debug!(event = "core.git.stats.head_read_failed", error = %e);
            return None;
        }
    };
    let commit = match head.peel_to_commit() {
        Ok(c) => c,
        Err(e) => {
            warn!(event = "core.git.stats.commit_peel_failed", error = %e);
            return None;
        }
    };
    let time = commit.time();
    let secs = time.seconds();
    let offset_mins = time.offset_minutes();
    let offset_secs = (offset_mins as i64) * 60;
    let utc_secs = secs + offset_secs;
    match chrono::DateTime::from_timestamp(utc_secs, 0) {
        Some(dt) => Some(dt.to_rfc3339()),
        None => {
            warn!(event = "core.git.stats.timestamp_invalid", secs = utc_secs);
            None
        }
    }
}

/// Compute diff stats between merge base tree and branch tip tree.
///
/// Shows the total changes introduced by the branch (how big the PR will be).
/// Returns `None` if commits cannot be resolved or diff computation fails (logged as warnings).
fn diff_against_base(repo: &Repository, branch_oid: Oid, merge_base_oid: Oid) -> Option<DiffStats> {
    let base_commit = match repo.find_commit(merge_base_oid) {
        Ok(c) => c,
        Err(e) => {
            warn!(event = "core.git.stats.base_commit_not_found", error = %e);
            return None;
        }
    };
    let branch_commit = match repo.find_commit(branch_oid) {
        Ok(c) => c,
        Err(e) => {
            warn!(event = "core.git.stats.branch_commit_not_found", error = %e);
            return None;
        }
    };
    let base_tree = match base_commit.tree() {
        Ok(t) => t,
        Err(e) => {
            warn!(event = "core.git.stats.base_tree_failed", error = %e);
            return None;
        }
    };
    let branch_tree = match branch_commit.tree() {
        Ok(t) => t,
        Err(e) => {
            warn!(event = "core.git.stats.branch_tree_failed", error = %e);
            return None;
        }
    };
    let diff = match repo.diff_tree_to_tree(Some(&base_tree), Some(&branch_tree), None) {
        Ok(d) => d,
        Err(e) => {
            warn!(event = "core.git.stats.diff_computation_failed", error = %e);
            return None;
        }
    };
    let stats = match diff.stats() {
        Ok(s) => s,
        Err(e) => {
            warn!(event = "core.git.stats.diff_stats_failed", error = %e);
            return None;
        }
    };
    Some(DiffStats {
        insertions: stats.insertions(),
        deletions: stats.deletions(),
        files_changed: stats.files_changed(),
    })
}

/// Get list of changed file paths between merge base and branch tip.
///
/// Returns the set of files modified, added, or deleted on the branch
/// relative to the merge base.
///
/// # Errors
///
/// Returns a descriptive error string if commits cannot be resolved,
/// trees cannot be retrieved, or diff computation fails.
fn get_changed_files(
    repo: &Repository,
    branch_oid: Oid,
    merge_base_oid: Oid,
) -> Result<Vec<PathBuf>, String> {
    let base_commit = repo.find_commit(merge_base_oid).map_err(|e| {
        warn!(event = "core.git.overlaps.base_commit_not_found", error = %e);
        format!("Base commit not found: {}", e)
    })?;
    let branch_commit = repo.find_commit(branch_oid).map_err(|e| {
        warn!(event = "core.git.overlaps.branch_commit_not_found", error = %e);
        format!("Branch commit not found: {}", e)
    })?;
    let base_tree = base_commit.tree().map_err(|e| {
        warn!(event = "core.git.overlaps.base_tree_failed", error = %e);
        format!("Failed to read base tree: {}", e)
    })?;
    let branch_tree = branch_commit.tree().map_err(|e| {
        warn!(event = "core.git.overlaps.branch_tree_failed", error = %e);
        format!("Failed to read branch tree: {}", e)
    })?;
    let diff = repo
        .diff_tree_to_tree(Some(&base_tree), Some(&branch_tree), None)
        .map_err(|e| {
            warn!(event = "core.git.overlaps.diff_computation_failed", error = %e);
            format!("Diff computation failed: {}", e)
        })?;

    let files: Vec<PathBuf> = diff
        .deltas()
        .filter_map(|delta| {
            delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_path_buf())
        })
        .collect();

    Ok(files)
}

/// Check for merge conflicts between branch tip and base tip (in-memory).
///
/// Performs an in-memory merge without modifying the working tree.
/// Returns `Unknown` if the merge cannot be performed (logged as warnings).
fn check_conflicts(repo: &Repository, branch_oid: Oid, base_oid: Oid) -> ConflictStatus {
    let branch_commit = match repo.find_commit(branch_oid) {
        Ok(c) => c,
        Err(e) => {
            warn!(event = "core.git.stats.conflict_check_branch_not_found", error = %e);
            return ConflictStatus::Unknown;
        }
    };
    let base_commit = match repo.find_commit(base_oid) {
        Ok(c) => c,
        Err(e) => {
            warn!(event = "core.git.stats.conflict_check_base_not_found", error = %e);
            return ConflictStatus::Unknown;
        }
    };

    let index = match repo.merge_commits(&branch_commit, &base_commit, None) {
        Ok(idx) => idx,
        Err(e) => {
            warn!(event = "core.git.stats.merge_check_failed", error = %e);
            return ConflictStatus::Unknown;
        }
    };

    if index.has_conflicts() {
        ConflictStatus::Conflicts
    } else {
        ConflictStatus::Clean
    }
}

/// Count commits ahead and behind between branch tip and base branch tip.
fn count_base_drift(
    repo: &Repository,
    branch_oid: Oid,
    base_oid: Oid,
    base_branch: &str,
) -> BaseBranchDrift {
    let ahead = count_commits_since(repo, branch_oid, base_oid);
    let behind = count_commits_since(repo, base_oid, branch_oid);
    BaseBranchDrift {
        ahead,
        behind,
        base_branch: base_branch.to_string(),
    }
}

/// Resolve a branch name to its OID, trying local first then remote.
fn resolve_branch_oid(repo: &Repository, branch_name: &str) -> Option<Oid> {
    // Try local branch first
    match repo.find_branch(branch_name, git2::BranchType::Local) {
        Ok(branch) => {
            if let Some(oid) = branch.get().target() {
                return Some(oid);
            }
            warn!(
                event = "core.git.stats.branch_no_target",
                branch = branch_name
            );
            return None;
        }
        Err(_) => {
            debug!(
                event = "core.git.stats.branch_not_found_local",
                branch = branch_name
            );
        }
    }
    // Try remote tracking branch
    let remote_ref = format!("origin/{}", branch_name);
    match repo.find_branch(&remote_ref, git2::BranchType::Remote) {
        Ok(branch) => {
            if let Some(oid) = branch.get().target() {
                return Some(oid);
            }
            warn!(
                event = "core.git.stats.remote_branch_no_target",
                branch = branch_name
            );
            return None;
        }
        Err(_) => {
            debug!(
                event = "core.git.stats.branch_not_found_remote",
                branch = branch_name
            );
        }
    }
    None
}

/// Check if repository has any remote configured.
fn repo_has_remote(repo: &Repository) -> bool {
    repo.remotes().is_ok_and(|remotes| !remotes.is_empty())
}

/// Collect comprehensive branch health metrics for a kild.
///
/// Returns pure git metrics only. Merge readiness (which depends on
/// forge/PR data) is computed separately by the caller.
///
/// - `branch`: User branch name (without `kild/` prefix).
/// - `base_branch`: Base branch for drift comparison (e.g., "main").
/// - `created_at`: Session creation timestamp (RFC3339), passed through to result.
///
/// Returns `Err` if the worktree cannot be opened or branch refs cannot be resolved.
pub fn collect_branch_health(
    worktree_path: &Path,
    branch: &str,
    base_branch: &str,
    created_at: &str,
) -> Result<BranchHealth, String> {
    let repo = match Repository::open(worktree_path) {
        Ok(r) => r,
        Err(e) => {
            warn!(event = "core.git.stats.repo_open_failed", branch = branch, error = %e);
            return Err(format!("Failed to open repository: {}", e));
        }
    };

    let has_remote = repo_has_remote(&repo);

    // Resolve kild branch OID
    let kild_branch = kild_branch_name(branch);
    let branch_oid = match resolve_branch_oid(&repo, &kild_branch) {
        Some(oid) => oid,
        None => {
            warn!(
                event = "core.git.stats.branch_not_found",
                branch = kild_branch
            );
            return Err(format!("Branch '{}' not found in repository", kild_branch));
        }
    };

    // Resolve base branch OID
    let base_oid = match resolve_branch_oid(&repo, base_branch) {
        Some(oid) => oid,
        None => {
            warn!(
                event = "core.git.stats.base_branch_not_found",
                base = base_branch
            );
            return Err(format!(
                "Base branch '{}' not found. Check your git.base_branch config.",
                base_branch
            ));
        }
    };

    // Find merge base
    let merge_base = find_merge_base(&repo, branch_oid, base_oid);

    // Commit activity
    let commits_since_base = merge_base.map_or(0, |mb| count_commits_since(&repo, branch_oid, mb));
    let last_commit_time = get_last_commit_time(&repo);

    // Diff vs base
    let diff_vs_base = merge_base.and_then(|mb| diff_against_base(&repo, branch_oid, mb));

    // Conflict detection (against base tip, not merge base)
    let conflict_status = check_conflicts(&repo, branch_oid, base_oid);

    // Base branch drift
    let drift = count_base_drift(&repo, branch_oid, base_oid, base_branch);

    Ok(BranchHealth {
        branch: branch.to_string(),
        created_at: created_at.to_string(),
        commit_activity: CommitActivity {
            commits_since_base,
            last_commit_time,
        },
        drift,
        diff_vs_base,
        conflict_status,
        has_remote,
    })
}

/// Collect file overlap information across multiple kilds.
///
/// For each session, computes the set of changed files relative to the merge base,
/// then identifies files modified by more than one kild.
///
/// Sessions that fail to provide changed files (e.g., repo can't be opened, branch
/// not found, merge base unavailable) are collected in the returned error vec
/// but do not prevent other sessions from being analyzed.
pub fn collect_file_overlaps(
    sessions: &[crate::Session],
    base_branch: &str,
) -> (OverlapReport, Vec<(String, String)>) {
    use std::collections::{HashMap, HashSet};

    info!(
        event = "core.git.overlaps.collect_started",
        session_count = sessions.len(),
        base_branch = base_branch
    );

    // Phase 1: Collect changed files per kild
    let mut files_by_branch: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut errors: Vec<(String, String)> = Vec::new();

    for session in sessions {
        let repo = match Repository::open(&session.worktree_path) {
            Ok(r) => r,
            Err(e) => {
                warn!(event = "core.git.overlaps.repo_open_failed", branch = &*session.branch, error = %e);
                errors.push((
                    session.branch.clone(),
                    format!(
                        "Failed to open repository at {}: {}",
                        session.worktree_path.display(),
                        e
                    ),
                ));
                continue;
            }
        };

        let kild_branch = kild_branch_name(&session.branch);
        let branch_oid = match resolve_branch_oid(&repo, &kild_branch) {
            Some(oid) => oid,
            None => {
                warn!(
                    event = "core.git.overlaps.branch_not_found",
                    branch = &*kild_branch
                );
                errors.push((
                    session.branch.clone(),
                    format!(
                        "Branch '{}' not found (checked local and origin remote)",
                        kild_branch
                    ),
                ));
                continue;
            }
        };

        let base_oid = match resolve_branch_oid(&repo, base_branch) {
            Some(oid) => oid,
            None => {
                warn!(
                    event = "core.git.overlaps.base_branch_not_found",
                    base = base_branch
                );
                errors.push((
                    session.branch.clone(),
                    format!(
                        "Base branch '{}' not found (checked local and origin remote)",
                        base_branch
                    ),
                ));
                continue;
            }
        };

        let merge_base = match find_merge_base(&repo, branch_oid, base_oid) {
            Some(mb) => mb,
            None => {
                warn!(
                    event = "core.git.overlaps.merge_base_not_found",
                    branch = &*session.branch
                );
                errors.push((
                    session.branch.clone(),
                    format!(
                        "No common ancestor with base branch '{}' (branch may be orphaned)",
                        base_branch
                    ),
                ));
                continue;
            }
        };

        match get_changed_files(&repo, branch_oid, merge_base) {
            Ok(files) => {
                files_by_branch.insert(session.branch.clone(), files);
            }
            Err(detail) => {
                errors.push((session.branch.clone(), detail));
            }
        }
    }

    // Phase 2: Build file → branches map
    let mut file_to_branches: HashMap<PathBuf, Vec<String>> = HashMap::new();
    for (branch, files) in &files_by_branch {
        for file in files {
            file_to_branches
                .entry(file.clone())
                .or_default()
                .push(branch.clone());
        }
    }

    // Phase 3: Extract overlaps (files in >1 branch) and clean kilds
    let mut overlapping_files: Vec<FileOverlap> = file_to_branches
        .into_iter()
        .filter(|(_, branches)| branches.len() > 1)
        .map(|(file, mut branches)| {
            branches.sort();
            FileOverlap { file, branches }
        })
        .collect();
    overlapping_files.sort_by(|a, b| {
        b.branches
            .len()
            .cmp(&a.branches.len())
            .then(a.file.cmp(&b.file))
    });

    // Determine which kilds have zero overlaps
    let overlapping_branches: HashSet<&str> = overlapping_files
        .iter()
        .flat_map(|o| o.branches.iter().map(|s| s.as_str()))
        .collect();

    let mut clean_kilds: Vec<CleanKild> = files_by_branch
        .iter()
        .filter(|(branch, _)| !overlapping_branches.contains(branch.as_str()))
        .map(|(branch, files)| CleanKild {
            branch: branch.clone(),
            changed_files: files.len(),
        })
        .collect();
    clean_kilds.sort_by(|a, b| a.branch.cmp(&b.branch));

    let report = OverlapReport {
        overlapping_files,
        clean_kilds,
    };

    info!(
        event = "core.git.overlaps.collect_completed",
        overlap_count = report.overlapping_files.len(),
        clean_count = report.clean_kilds.len(),
        error_count = errors.len()
    );

    (report, errors)
}

/// Compute merge readiness from git health metrics, worktree status, and optional PR info.
///
/// Priority order (highest severity first):
/// 1. HasConflicts / ConflictCheckFailed — blocks merge entirely
/// 2. NeedsRebase — behind base, conflicts likely if not rebased
/// 3. NeedsPush — local-only commits, PR can't be created/updated
/// 4. NeedsPr — pushed but no tracking PR exists
/// 5. CiFailing — PR exists but not passing checks
/// 6. Ready / ReadyLocal — all checks passed
pub fn compute_merge_readiness(
    health: &BranchHealth,
    worktree_status: &Option<WorktreeStatus>,
    pr_info: Option<&PrInfo>,
) -> MergeReadiness {
    match health.conflict_status {
        ConflictStatus::Conflicts => return MergeReadiness::HasConflicts,
        ConflictStatus::Unknown => return MergeReadiness::ConflictCheckFailed,
        ConflictStatus::Clean => {}
    }

    if health.drift.behind > 0 {
        return MergeReadiness::NeedsRebase;
    }

    if !health.has_remote {
        return MergeReadiness::ReadyLocal;
    }

    // Check if there are unpushed commits
    let has_unpushed = worktree_status
        .as_ref()
        .is_some_and(|ws| ws.unpushed_commit_count > 0 || !ws.has_remote_branch);

    if has_unpushed {
        return MergeReadiness::NeedsPush;
    }

    let Some(pr) = pr_info else {
        return MergeReadiness::NeedsPr;
    };

    if pr.ci_status == CiStatus::Failing {
        return MergeReadiness::CiFailing;
    }

    MergeReadiness::Ready
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_for_path() {
        assert_eq!(sanitize_for_path("feature/foo"), "feature-foo");
        assert_eq!(sanitize_for_path("bugfix/auth/login"), "bugfix-auth-login");
        assert_eq!(sanitize_for_path("simple-branch"), "simple-branch");
        assert_eq!(sanitize_for_path("no_slashes_here"), "no_slashes_here");
    }

    #[test]
    fn test_sanitize_for_path_edge_cases() {
        // Multiple consecutive slashes
        assert_eq!(sanitize_for_path("feature//auth"), "feature--auth");

        // Leading slash (invalid git branch, but document behavior)
        assert_eq!(sanitize_for_path("/feature"), "-feature");

        // Trailing slash (invalid git branch, but document behavior)
        assert_eq!(sanitize_for_path("feature/"), "feature-");

        // Mixed valid characters preserved
        assert_eq!(sanitize_for_path("feat/bug_fix-123"), "feat-bug_fix-123");
    }

    #[test]
    fn test_sanitize_collision_awareness() {
        // Document that different branches can sanitize to the same name.
        // Git2 will reject duplicate worktree names at creation time.
        let sanitized_with_slash = sanitize_for_path("feature/foo");
        let sanitized_with_hyphen = sanitize_for_path("feature-foo");

        // Both sanitize to the same filesystem-safe name
        assert_eq!(sanitized_with_slash, sanitized_with_hyphen);
        assert_eq!(sanitized_with_slash, "feature-foo");
    }

    #[test]
    fn test_calculate_worktree_path() {
        let base = Path::new("/home/user/.shards");
        let path = calculate_worktree_path(base, "my-project", "feature-branch");

        assert_eq!(
            path,
            PathBuf::from("/home/user/.shards/worktrees/my-project/feature-branch")
        );
    }

    #[test]
    fn test_calculate_worktree_path_with_slashes() {
        let base = Path::new("/home/user/.kild");

        // Branch with single slash
        let path = calculate_worktree_path(base, "my-project", "feature/auth");
        assert_eq!(
            path,
            PathBuf::from("/home/user/.kild/worktrees/my-project/feature-auth")
        );

        // Branch with multiple slashes
        let path = calculate_worktree_path(base, "my-project", "feature/auth/oauth");
        assert_eq!(
            path,
            PathBuf::from("/home/user/.kild/worktrees/my-project/feature-auth-oauth")
        );

        // Branch without slashes (unchanged behavior)
        let path = calculate_worktree_path(base, "my-project", "simple-branch");
        assert_eq!(
            path,
            PathBuf::from("/home/user/.kild/worktrees/my-project/simple-branch")
        );
    }

    #[test]
    fn test_derive_project_name_from_path() {
        let path = Path::new("/home/user/projects/my-awesome-project");
        let name = derive_project_name_from_path(path);
        assert_eq!(name, "my-awesome-project");

        let root_path = Path::new("/");
        let root_name = derive_project_name_from_path(root_path);
        assert_eq!(root_name, "unknown");
    }

    #[test]
    fn test_derive_project_name_from_remote() {
        assert_eq!(
            derive_project_name_from_remote("https://github.com/user/repo.git"),
            "repo"
        );

        assert_eq!(
            derive_project_name_from_remote("git@github.com:user/repo.git"),
            "repo"
        );

        assert_eq!(
            derive_project_name_from_remote("https://gitlab.com/group/subgroup/project.git"),
            "project"
        );

        assert_eq!(derive_project_name_from_remote("invalid-url"), "unknown");
    }

    #[test]
    fn test_generate_project_id() {
        let path1 = Path::new("/path/to/project");
        let path2 = Path::new("/different/path");

        let id1 = generate_project_id(path1);
        let id2 = generate_project_id(path2);

        assert_ne!(id1, id2);
        assert!(!id1.is_empty());
        assert!(!id2.is_empty());

        // Same path should generate same ID
        let id1_again = generate_project_id(path1);
        assert_eq!(id1, id1_again);
    }

    #[test]
    fn test_validate_branch_name() {
        assert!(validate_branch_name("feature-branch").is_ok());
        assert!(validate_branch_name("feat/auth").is_ok());
        assert!(validate_branch_name("v1.2.3").is_ok());

        assert!(validate_branch_name("").is_err());
        assert!(validate_branch_name("  ").is_err());
        assert!(validate_branch_name("branch..name").is_err());
        assert!(validate_branch_name("-branch").is_err());
        assert!(validate_branch_name("branch name").is_err());
        assert!(validate_branch_name("branch\tname").is_err());
        assert!(validate_branch_name("branch\nname").is_err());
    }

    #[test]
    fn test_is_valid_git_directory() {
        // This will fail in most test environments, but tests the logic
        let current_dir = std::env::current_dir().unwrap();
        let _is_git = is_valid_git_directory(&current_dir);

        let non_git_dir = Path::new("/tmp");
        assert!(!is_valid_git_directory(non_git_dir) || non_git_dir.join(".git").exists());
    }

    #[test]
    fn test_should_use_current_branch() {
        assert!(should_use_current_branch(
            "feature-branch",
            "feature-branch"
        ));
        assert!(!should_use_current_branch("main", "feature-branch"));
        assert!(!should_use_current_branch("feature-branch", "main"));
        assert!(should_use_current_branch("issue-33", "issue-33"));
    }

    #[test]
    fn test_kild_branch_name() {
        assert_eq!(kild_branch_name("my-feature"), "kild/my-feature");
        assert_eq!(kild_branch_name("feature/auth"), "kild/feature/auth");
        assert_eq!(kild_branch_name("simple"), "kild/simple");
    }

    #[test]
    fn test_kild_worktree_admin_name() {
        assert_eq!(kild_worktree_admin_name("my-feature"), "kild-my-feature");
        assert_eq!(
            kild_worktree_admin_name("feature/auth"),
            "kild-feature-auth"
        );
        assert_eq!(
            kild_worktree_admin_name("bugfix/auth/login"),
            "kild-bugfix-auth-login"
        );
    }

    #[test]
    fn test_kild_branch_prefix_constant() {
        assert_eq!(KILD_BRANCH_PREFIX, "kild/");
    }

    // --- get_diff_stats tests ---

    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_git_repo(dir: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .expect("Failed to init git repo");
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .expect("Failed to set git email");
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .expect("Failed to set git name");
    }

    #[test]
    fn test_get_diff_stats_clean_repo() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Create and commit a file
        fs::write(dir.path().join("test.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let stats = get_diff_stats(dir.path()).unwrap();
        assert_eq!(stats.insertions, 0);
        assert_eq!(stats.deletions, 0);
        assert_eq!(stats.files_changed, 0);
    }

    #[test]
    fn test_get_diff_stats_with_changes() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Create and commit a file
        fs::write(dir.path().join("test.txt"), "line1\nline2\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Make changes
        fs::write(dir.path().join("test.txt"), "line1\nmodified\nnew line\n").unwrap();

        let stats = get_diff_stats(dir.path()).unwrap();
        assert!(stats.insertions > 0 || stats.deletions > 0);
        assert_eq!(stats.files_changed, 1);
    }

    #[test]
    fn test_get_diff_stats_not_a_repo() {
        let dir = TempDir::new().unwrap();
        // Don't init git

        let result = get_diff_stats(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_get_diff_stats_staged_changes_not_included() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Initial commit
        fs::write(dir.path().join("test.txt"), "line1\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Stage a change (but don't commit)
        fs::write(dir.path().join("test.txt"), "line1\nstaged line\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Staged changes should NOT appear (diff_index_to_workdir only sees unstaged)
        let stats = get_diff_stats(dir.path()).unwrap();
        assert_eq!(
            stats.insertions, 0,
            "Staged changes should not appear in index-to-workdir diff"
        );
        assert_eq!(stats.files_changed, 0);
        assert!(!stats.has_changes());
    }

    #[test]
    fn test_get_diff_stats_binary_file() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Create binary file (PNG header bytes)
        let png_header: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        fs::write(dir.path().join("image.png"), png_header).unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Modify binary
        let modified: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0xFF, 0xFF, 0xFF, 0xFF];
        fs::write(dir.path().join("image.png"), modified).unwrap();

        let stats = get_diff_stats(dir.path()).unwrap();
        // Binary files are detected as changed
        assert_eq!(
            stats.files_changed, 1,
            "Binary file change should be detected"
        );
        // Note: git2 may report small line counts for binary files depending on content
        // The key assertion is that the file change is detected
    }

    #[test]
    fn test_get_diff_stats_untracked_files_not_included() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        fs::write(dir.path().join("committed.txt"), "initial").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Create untracked file (NOT staged)
        fs::write(dir.path().join("untracked.txt"), "new file content\n").unwrap();

        let stats = get_diff_stats(dir.path()).unwrap();
        // Untracked files don't appear in index-to-workdir diff
        assert_eq!(
            stats.files_changed, 0,
            "Untracked files should not be counted"
        );
        assert!(!stats.has_changes());
    }

    #[test]
    fn test_diff_stats_has_changes() {
        use crate::git::types::DiffStats;

        let no_changes = DiffStats::default();
        assert!(!no_changes.has_changes());

        let insertions_only = DiffStats {
            insertions: 5,
            deletions: 0,
            files_changed: 1,
        };
        assert!(insertions_only.has_changes());

        let deletions_only = DiffStats {
            insertions: 0,
            deletions: 3,
            files_changed: 1,
        };
        assert!(deletions_only.has_changes());

        let both = DiffStats {
            insertions: 10,
            deletions: 5,
            files_changed: 2,
        };
        assert!(both.has_changes());

        // Edge case: files_changed but no line counts
        // This can happen with binary files or certain edge cases
        let files_only = DiffStats {
            insertions: 0,
            deletions: 0,
            files_changed: 1,
        };
        // has_changes() only checks line counts, not files_changed
        assert!(
            !files_only.has_changes(),
            "has_changes() checks line counts only"
        );
    }

    // --- count_unpushed_commits / behind count tests ---

    #[test]
    fn test_count_unpushed_commits_no_remote_returns_zero_behind() {
        // A repo with no remote should return (0, 0, false)
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        fs::write(dir.path().join("test.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let repo = Repository::open(dir.path()).unwrap();
        let counts = count_unpushed_commits(&repo);
        assert_eq!(counts.ahead, 0);
        assert_eq!(counts.behind, 0);
        assert!(!counts.has_remote);
        assert!(!counts.behind_count_failed);
    }

    #[test]
    fn test_worktree_status_includes_behind_commit_count() {
        // A clean repo with no remote should have behind_commit_count = 0
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        fs::write(dir.path().join("test.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let status = get_worktree_status(dir.path()).unwrap();
        assert_eq!(status.behind_commit_count, 0);
        assert_eq!(status.unpushed_commit_count, 0);
        assert!(!status.has_remote_branch);
        assert!(!status.behind_count_failed);
    }

    /// Helper: git add all + commit with message
    fn git_add_commit(dir: &Path, msg: &str) {
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", msg])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    /// Helper: Create a bare git repository (for testing remote interactions)
    fn create_bare_repo(dir: &Path) {
        Command::new("git")
            .args(["init", "--bare"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    /// Helper: Configure git user identity in a repository
    fn configure_git_user(dir: &Path, email: &str, name: &str) {
        Command::new("git")
            .args(["config", "user.email", email])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", name])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    /// Helper: Get the current branch name
    fn get_current_branch(dir: &Path) -> String {
        let output = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Helper: Add remote and push with tracking
    fn add_remote_and_push(local_dir: &Path, remote_path: &Path) {
        let branch_name = get_current_branch(local_dir);
        Command::new("git")
            .args(["remote", "add", "origin", remote_path.to_str().unwrap()])
            .current_dir(local_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "-u", "origin", &branch_name])
            .current_dir(local_dir)
            .output()
            .unwrap();
    }

    #[test]
    fn test_count_unpushed_commits_behind_remote() {
        // Setup: local repo → bare origin → clone; push from clone, fetch in local
        let local_dir = TempDir::new().unwrap();
        init_git_repo(local_dir.path());

        // Initial commit in local
        fs::write(local_dir.path().join("file.txt"), "initial").unwrap();
        git_add_commit(local_dir.path(), "initial");

        // Create bare "origin" and push
        let bare_dir = TempDir::new().unwrap();
        create_bare_repo(bare_dir.path());
        add_remote_and_push(local_dir.path(), bare_dir.path());

        // Clone into another dir and push a new commit
        let other_dir = TempDir::new().unwrap();
        Command::new("git")
            .args([
                "clone",
                bare_dir.path().to_str().unwrap(),
                other_dir.path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        configure_git_user(other_dir.path(), "other@test.com", "Other");
        fs::write(other_dir.path().join("other.txt"), "remote change").unwrap();
        git_add_commit(other_dir.path(), "remote commit");
        Command::new("git")
            .args(["push"])
            .current_dir(other_dir.path())
            .output()
            .unwrap();

        // Fetch in local so it sees the remote commit
        Command::new("git")
            .args(["fetch", "origin"])
            .current_dir(local_dir.path())
            .output()
            .unwrap();

        let repo = Repository::open(local_dir.path()).unwrap();
        let counts = count_unpushed_commits(&repo);

        assert_eq!(counts.ahead, 0, "local should not be ahead");
        assert_eq!(counts.behind, 1, "local should be 1 commit behind");
        assert!(counts.has_remote, "should have remote tracking branch");
        assert!(!counts.behind_count_failed, "behind count should succeed");
    }

    #[test]
    fn test_count_unpushed_commits_ahead_and_behind() {
        // Local has 1 commit not on remote, remote has 1 commit not on local → diverged
        let local_dir = TempDir::new().unwrap();
        init_git_repo(local_dir.path());

        fs::write(local_dir.path().join("file.txt"), "initial").unwrap();
        git_add_commit(local_dir.path(), "initial");

        let bare_dir = TempDir::new().unwrap();
        create_bare_repo(bare_dir.path());
        add_remote_and_push(local_dir.path(), bare_dir.path());

        // Push a commit from a clone
        let other_dir = TempDir::new().unwrap();
        Command::new("git")
            .args([
                "clone",
                bare_dir.path().to_str().unwrap(),
                other_dir.path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        configure_git_user(other_dir.path(), "other@test.com", "Other");
        fs::write(other_dir.path().join("remote.txt"), "remote").unwrap();
        git_add_commit(other_dir.path(), "remote commit");
        Command::new("git")
            .args(["push"])
            .current_dir(other_dir.path())
            .output()
            .unwrap();

        // Make a local commit (diverging from remote)
        fs::write(local_dir.path().join("local.txt"), "local").unwrap();
        git_add_commit(local_dir.path(), "local commit");

        // Fetch so local knows about remote
        Command::new("git")
            .args(["fetch", "origin"])
            .current_dir(local_dir.path())
            .output()
            .unwrap();

        let repo = Repository::open(local_dir.path()).unwrap();
        let counts = count_unpushed_commits(&repo);

        assert_eq!(counts.ahead, 1, "local should be 1 ahead");
        assert_eq!(counts.behind, 1, "local should be 1 behind");
        assert!(counts.has_remote, "should have remote tracking branch");
        assert!(!counts.behind_count_failed, "behind count should succeed");
    }

    #[test]
    fn test_worktree_status_behind_with_remote() {
        // End-to-end: get_worktree_status should report behind_commit_count > 0
        let local_dir = TempDir::new().unwrap();
        init_git_repo(local_dir.path());

        fs::write(local_dir.path().join("file.txt"), "initial").unwrap();
        git_add_commit(local_dir.path(), "initial");

        let bare_dir = TempDir::new().unwrap();
        create_bare_repo(bare_dir.path());
        add_remote_and_push(local_dir.path(), bare_dir.path());

        // Push 2 commits from a clone
        let other_dir = TempDir::new().unwrap();
        Command::new("git")
            .args([
                "clone",
                bare_dir.path().to_str().unwrap(),
                other_dir.path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        configure_git_user(other_dir.path(), "other@test.com", "Other");
        fs::write(other_dir.path().join("a.txt"), "a").unwrap();
        git_add_commit(other_dir.path(), "remote commit 1");
        fs::write(other_dir.path().join("b.txt"), "b").unwrap();
        git_add_commit(other_dir.path(), "remote commit 2");
        Command::new("git")
            .args(["push"])
            .current_dir(other_dir.path())
            .output()
            .unwrap();

        // Fetch in local
        Command::new("git")
            .args(["fetch", "origin"])
            .current_dir(local_dir.path())
            .output()
            .unwrap();

        let status = get_worktree_status(local_dir.path()).unwrap();
        assert_eq!(status.behind_commit_count, 2);
        assert_eq!(status.unpushed_commit_count, 0);
        assert!(status.has_remote_branch);
        assert!(!status.behind_count_failed);
    }

    // --- collect_git_stats tests ---

    #[test]
    fn test_collect_git_stats_nonexistent_path() {
        let result = collect_git_stats(Path::new("/nonexistent/path"), "test-branch");
        assert!(result.is_none());
    }

    #[test]
    fn test_collect_git_stats_clean_repo() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        fs::write(dir.path().join("file.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let stats = collect_git_stats(dir.path(), "test-branch");
        assert!(stats.is_some());
        let stats = stats.unwrap();
        assert!(stats.diff_stats.is_some());
        assert!(stats.worktree_status.is_some());
        assert!(stats.has_data());
        assert!(!stats.is_empty());
    }

    #[test]
    fn test_collect_git_stats_with_modifications() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());
        fs::write(dir.path().join("file.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Modify tracked file to create diff stats
        fs::write(dir.path().join("file.txt"), "modified").unwrap();

        let stats = collect_git_stats(dir.path(), "test-branch");
        assert!(stats.is_some());
        let stats = stats.unwrap();
        assert!(stats.has_data());
        assert!(stats.diff_stats.is_some());
        let diff = stats.diff_stats.unwrap();
        assert!(diff.insertions > 0 || diff.deletions > 0);
    }

    // --- Branch health tests ---

    #[test]
    fn test_collect_branch_health_nonexistent_path() {
        let result = collect_branch_health(
            Path::new("/nonexistent/path"),
            "test",
            "main",
            "2026-02-09T10:00:00Z",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_collect_branch_health_with_kild_branch() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Create initial commit on main
        fs::write(dir.path().join("file.txt"), "initial").unwrap();
        git_add_commit(dir.path(), "initial on main");

        // Rename default branch to main (in case it's 'master')
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Create kild branch with a commit
        Command::new("git")
            .args(["checkout", "-b", "kild/test-feature"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("feature.txt"), "feature code").unwrap();
        git_add_commit(dir.path(), "feature commit");

        let health =
            collect_branch_health(dir.path(), "test-feature", "main", "2026-02-09T10:00:00Z");
        assert!(health.is_ok());
        let health = health.unwrap();
        assert_eq!(health.branch, "test-feature");
        assert_eq!(health.commit_activity.commits_since_base, 1);
        assert!(!health.has_remote);
        assert_eq!(health.drift.ahead, 1);
        assert_eq!(health.drift.behind, 0);
        assert_eq!(health.conflict_status, ConflictStatus::Clean);
        assert!(health.diff_vs_base.is_some());
    }

    #[test]
    fn test_collect_branch_health_behind_base() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Create initial commit on main
        fs::write(dir.path().join("file.txt"), "initial").unwrap();
        git_add_commit(dir.path(), "initial on main");

        // Rename default branch to main
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Create kild branch
        Command::new("git")
            .args(["checkout", "-b", "kild/test-behind"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("feature.txt"), "feature").unwrap();
        git_add_commit(dir.path(), "feature commit");

        // Go back to main and add commits
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("main-update.txt"), "update 1").unwrap();
        git_add_commit(dir.path(), "main update 1");
        fs::write(dir.path().join("main-update2.txt"), "update 2").unwrap();
        git_add_commit(dir.path(), "main update 2");

        // Go back to kild branch to set HEAD
        Command::new("git")
            .args(["checkout", "kild/test-behind"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let health =
            collect_branch_health(dir.path(), "test-behind", "main", "2026-02-09T10:00:00Z");
        assert!(health.is_ok());
        let health = health.unwrap();
        assert_eq!(health.drift.ahead, 1);
        assert_eq!(health.drift.behind, 2);
    }

    #[test]
    fn test_collect_branch_health_with_conflicts() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Create initial commit on main
        fs::write(dir.path().join("shared.txt"), "original content").unwrap();
        git_add_commit(dir.path(), "initial on main");

        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Create kild branch that modifies the same file
        Command::new("git")
            .args(["checkout", "-b", "kild/test-conflicts"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("shared.txt"), "branch version of content").unwrap();
        git_add_commit(dir.path(), "branch change to shared file");

        // Go back to main and modify the same file differently
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("shared.txt"), "main version of content").unwrap();
        git_add_commit(dir.path(), "main change to shared file");

        // Switch back to kild branch for HEAD
        Command::new("git")
            .args(["checkout", "kild/test-conflicts"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let health =
            collect_branch_health(dir.path(), "test-conflicts", "main", "2026-02-09T10:00:00Z");
        assert!(health.is_ok());
        let health = health.unwrap();
        assert_eq!(
            health.conflict_status,
            ConflictStatus::Conflicts,
            "Branches modifying the same file should detect conflicts"
        );
    }

    #[test]
    fn test_collect_branch_health_invalid_base_branch() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        fs::write(dir.path().join("file.txt"), "initial").unwrap();
        git_add_commit(dir.path(), "initial on main");

        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["checkout", "-b", "kild/test"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("feature.txt"), "feature").unwrap();
        git_add_commit(dir.path(), "feature commit");

        let result = collect_branch_health(
            dir.path(),
            "test",
            "nonexistent-base",
            "2026-02-09T10:00:00Z",
        );
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("nonexistent-base"),
            "Error should mention the missing base branch: {}",
            msg
        );
    }

    // --- get_changed_files tests ---

    #[test]
    fn test_get_changed_files_with_changes() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        // Create initial commit on main
        fs::write(dir.path().join("file.txt"), "initial").unwrap();
        git_add_commit(dir.path(), "initial");

        // Rename to main
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Create a branch with changes
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("new_file.rs"), "code").unwrap();
        fs::write(dir.path().join("file.txt"), "modified").unwrap();
        git_add_commit(dir.path(), "feature changes");

        let repo = Repository::open(dir.path()).unwrap();
        let branch_oid = resolve_branch_oid(&repo, "feature").unwrap();
        let base_oid = resolve_branch_oid(&repo, "main").unwrap();
        let merge_base = find_merge_base(&repo, branch_oid, base_oid).unwrap();

        let files = get_changed_files(&repo, branch_oid, merge_base);
        assert!(files.is_ok());
        let files = files.unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.contains(&PathBuf::from("new_file.rs")));
        assert!(files.contains(&PathBuf::from("file.txt")));
    }

    #[test]
    fn test_get_changed_files_no_changes() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        fs::write(dir.path().join("file.txt"), "initial").unwrap();
        git_add_commit(dir.path(), "initial");

        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Create branch but don't change anything
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let repo = Repository::open(dir.path()).unwrap();
        let branch_oid = resolve_branch_oid(&repo, "feature").unwrap();
        let base_oid = resolve_branch_oid(&repo, "main").unwrap();
        let merge_base = find_merge_base(&repo, branch_oid, base_oid).unwrap();

        let files = get_changed_files(&repo, branch_oid, merge_base);
        assert!(files.is_ok());
        assert!(files.unwrap().is_empty());
    }

    #[test]
    fn test_get_changed_files_with_deleted_file() {
        let dir = TempDir::new().unwrap();
        init_git_repo(dir.path());

        fs::write(dir.path().join("to_delete.txt"), "content").unwrap();
        fs::write(dir.path().join("keep.txt"), "content").unwrap();
        git_add_commit(dir.path(), "initial");

        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::remove_file(dir.path().join("to_delete.txt")).unwrap();
        git_add_commit(dir.path(), "delete file");

        let repo = Repository::open(dir.path()).unwrap();
        let branch_oid = resolve_branch_oid(&repo, "feature").unwrap();
        let base_oid = resolve_branch_oid(&repo, "main").unwrap();
        let merge_base = find_merge_base(&repo, branch_oid, base_oid).unwrap();

        let files = get_changed_files(&repo, branch_oid, merge_base);
        assert!(files.is_ok());
        let files = files.unwrap();
        assert_eq!(files.len(), 1);
        assert!(files.contains(&PathBuf::from("to_delete.txt")));
    }

    // --- collect_file_overlaps tests ---

    fn make_test_session(branch: &str, worktree_path: PathBuf) -> crate::Session {
        crate::Session::new_for_test(branch.to_string(), worktree_path)
    }

    /// Helper: set up an independent git repo with a main branch, an initial file set,
    /// and a kild branch that modifies specified files. Each repo is independent (no clone)
    /// to avoid cross-platform issues with `git clone` into existing TempDir directories.
    fn setup_kild_repo(dir: &Path, branch: &str, initial_files: &[&str], modify_files: &[&str]) {
        init_git_repo(dir);
        for file in initial_files {
            fs::write(dir.join(file), "original").unwrap();
        }
        git_add_commit(dir, "initial");
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir)
            .output()
            .unwrap();
        let kild_branch = format!("kild/{}", branch);
        Command::new("git")
            .args(["checkout", "-b", &kild_branch])
            .current_dir(dir)
            .output()
            .unwrap();
        for file in modify_files {
            fs::write(dir.join(file), format!("modified by {}", branch)).unwrap();
        }
        git_add_commit(dir, &format!("{} changes", branch));
    }

    #[test]
    fn test_collect_file_overlaps_with_overlapping_files() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();

        let shared_files = &["shared.rs", "only_a.rs"];
        setup_kild_repo(
            dir1.path(),
            "branch-a",
            shared_files,
            &["shared.rs", "only_a.rs"],
        );
        setup_kild_repo(dir2.path(), "branch-b", shared_files, &["shared.rs"]);

        let sessions = vec![
            make_test_session("branch-a", dir1.path().to_path_buf()),
            make_test_session("branch-b", dir2.path().to_path_buf()),
        ];

        let (report, errors) = collect_file_overlaps(&sessions, "main");
        assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
        assert_eq!(report.overlapping_files.len(), 1);
        assert_eq!(report.overlapping_files[0].file, PathBuf::from("shared.rs"));
        assert_eq!(report.overlapping_files[0].branches.len(), 2);
        assert!(
            report.overlapping_files[0]
                .branches
                .contains(&"branch-a".to_string())
        );
        assert!(
            report.overlapping_files[0]
                .branches
                .contains(&"branch-b".to_string())
        );
    }

    #[test]
    fn test_collect_file_overlaps_no_overlaps() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();

        let all_files = &["file_a.rs", "file_b.rs"];
        setup_kild_repo(dir1.path(), "no-overlap-a", all_files, &["file_a.rs"]);
        setup_kild_repo(dir2.path(), "no-overlap-b", all_files, &["file_b.rs"]);

        let sessions = vec![
            make_test_session("no-overlap-a", dir1.path().to_path_buf()),
            make_test_session("no-overlap-b", dir2.path().to_path_buf()),
        ];

        let (report, errors) = collect_file_overlaps(&sessions, "main");
        assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
        assert!(report.overlapping_files.is_empty());
        assert_eq!(report.clean_kilds.len(), 2);
    }

    #[test]
    fn test_collect_file_overlaps_single_session() {
        let dir = TempDir::new().unwrap();
        setup_kild_repo(dir.path(), "solo", &["file.rs"], &["file.rs"]);

        let sessions = vec![make_test_session("solo", dir.path().to_path_buf())];
        let (report, errors) = collect_file_overlaps(&sessions, "main");
        assert!(errors.is_empty());
        assert!(report.overlapping_files.is_empty());
        assert_eq!(report.clean_kilds.len(), 1);
        assert_eq!(report.clean_kilds[0].branch, "solo");
        assert_eq!(report.clean_kilds[0].changed_files, 1);
    }

    #[test]
    fn test_collect_file_overlaps_session_with_bad_branch() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();

        init_git_repo(dir1.path());
        fs::write(dir1.path().join("file.rs"), "original").unwrap();
        git_add_commit(dir1.path(), "initial");
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir1.path())
            .output()
            .unwrap();

        // Good session
        Command::new("git")
            .args(["checkout", "-b", "kild/good"])
            .current_dir(dir1.path())
            .output()
            .unwrap();
        fs::write(dir1.path().join("file.rs"), "changed").unwrap();
        git_add_commit(dir1.path(), "good change");

        // Bad session: dir2 has no kild/bad branch
        init_git_repo(dir2.path());
        fs::write(dir2.path().join("dummy.rs"), "dummy").unwrap();
        git_add_commit(dir2.path(), "dummy");
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir2.path())
            .output()
            .unwrap();

        let sessions = vec![
            make_test_session("good", dir1.path().to_path_buf()),
            make_test_session("bad", dir2.path().to_path_buf()),
        ];

        let (report, errors) = collect_file_overlaps(&sessions, "main");
        // One session should fail (branch not found), one should succeed
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, "bad");
        assert!(report.overlapping_files.is_empty());
        assert_eq!(report.clean_kilds.len(), 1);
        assert_eq!(report.clean_kilds[0].branch, "good");
    }

    #[test]
    fn test_collect_file_overlaps_three_way_overlap() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();
        let dir3 = TempDir::new().unwrap();

        let all_files = &["core.rs", "utils.rs", "only_c.rs"];
        // All three modify core.rs, A and B modify utils.rs
        setup_kild_repo(dir1.path(), "branch-a", all_files, &["core.rs", "utils.rs"]);
        setup_kild_repo(dir2.path(), "branch-b", all_files, &["core.rs", "utils.rs"]);
        setup_kild_repo(
            dir3.path(),
            "branch-c",
            all_files,
            &["core.rs", "only_c.rs"],
        );

        let sessions = vec![
            make_test_session("branch-a", dir1.path().to_path_buf()),
            make_test_session("branch-b", dir2.path().to_path_buf()),
            make_test_session("branch-c", dir3.path().to_path_buf()),
        ];

        let (report, errors) = collect_file_overlaps(&sessions, "main");
        assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);

        // core.rs is modified by 3 branches, utils.rs by 2 — sorted by count desc
        assert_eq!(report.overlapping_files.len(), 2);
        assert_eq!(
            report.overlapping_files[0].file,
            PathBuf::from("core.rs"),
            "3-way overlap should sort first"
        );
        assert_eq!(report.overlapping_files[0].branches.len(), 3);
        assert_eq!(report.overlapping_files[1].file, PathBuf::from("utils.rs"));
        assert_eq!(report.overlapping_files[1].branches.len(), 2);

        // No clean kilds — all three are involved in at least one overlap
        assert!(
            report.clean_kilds.is_empty(),
            "All kilds have overlaps: {:?}",
            report.clean_kilds
        );
    }

    // --- Merge readiness tests ---

    fn make_health(
        conflict_status: ConflictStatus,
        behind: usize,
        has_remote: bool,
    ) -> BranchHealth {
        BranchHealth {
            branch: "test".to_string(),
            created_at: "2026-02-09T10:00:00Z".to_string(),
            commit_activity: CommitActivity {
                commits_since_base: 3,
                last_commit_time: None,
            },
            drift: BaseBranchDrift {
                ahead: 3,
                behind,
                base_branch: "main".to_string(),
            },
            diff_vs_base: Some(DiffStats {
                insertions: 10,
                deletions: 2,
                files_changed: 1,
            }),
            conflict_status,
            has_remote,
        }
    }

    #[test]
    fn test_readiness_has_conflicts() {
        let h = make_health(ConflictStatus::Conflicts, 0, true);
        assert_eq!(
            compute_merge_readiness(&h, &None, None),
            MergeReadiness::HasConflicts
        );
    }

    #[test]
    fn test_readiness_conflict_check_failed() {
        let h = make_health(ConflictStatus::Unknown, 0, true);
        assert_eq!(
            compute_merge_readiness(&h, &None, None),
            MergeReadiness::ConflictCheckFailed
        );
    }

    #[test]
    fn test_readiness_needs_rebase() {
        let h = make_health(ConflictStatus::Clean, 5, true);
        assert_eq!(
            compute_merge_readiness(&h, &None, None),
            MergeReadiness::NeedsRebase
        );
    }

    #[test]
    fn test_readiness_ready_local() {
        let h = make_health(ConflictStatus::Clean, 0, false);
        assert_eq!(
            compute_merge_readiness(&h, &None, None),
            MergeReadiness::ReadyLocal
        );
    }

    #[test]
    fn test_readiness_needs_push() {
        let h = make_health(ConflictStatus::Clean, 0, true);
        let ws = WorktreeStatus {
            unpushed_commit_count: 3,
            has_remote_branch: true,
            ..Default::default()
        };
        assert_eq!(
            compute_merge_readiness(&h, &Some(ws), None),
            MergeReadiness::NeedsPush
        );
    }

    #[test]
    fn test_readiness_needs_push_never_pushed() {
        let h = make_health(ConflictStatus::Clean, 0, true);
        let ws = WorktreeStatus {
            unpushed_commit_count: 0,
            has_remote_branch: false,
            ..Default::default()
        };
        assert_eq!(
            compute_merge_readiness(&h, &Some(ws), None),
            MergeReadiness::NeedsPush
        );
    }

    #[test]
    fn test_readiness_needs_pr() {
        let h = make_health(ConflictStatus::Clean, 0, true);
        let ws = WorktreeStatus {
            unpushed_commit_count: 0,
            has_remote_branch: true,
            ..Default::default()
        };
        assert_eq!(
            compute_merge_readiness(&h, &Some(ws), None),
            MergeReadiness::NeedsPr
        );
    }

    #[test]
    fn test_readiness_ci_failing() {
        use crate::forge::types::{PrState, ReviewStatus};
        let h = make_health(ConflictStatus::Clean, 0, true);
        let ws = WorktreeStatus {
            unpushed_commit_count: 0,
            has_remote_branch: true,
            ..Default::default()
        };
        let pr = PrInfo {
            number: 1,
            url: "https://example.com/pull/1".to_string(),
            state: PrState::Open,
            ci_status: CiStatus::Failing,
            ci_summary: None,
            review_status: ReviewStatus::Unknown,
            review_summary: None,
            updated_at: "2026-02-09T12:00:00Z".to_string(),
        };
        assert_eq!(
            compute_merge_readiness(&h, &Some(ws), Some(&pr)),
            MergeReadiness::CiFailing
        );
    }

    #[test]
    fn test_readiness_ready() {
        use crate::forge::types::{PrState, ReviewStatus};
        let h = make_health(ConflictStatus::Clean, 0, true);
        let ws = WorktreeStatus {
            unpushed_commit_count: 0,
            has_remote_branch: true,
            ..Default::default()
        };
        let pr = PrInfo {
            number: 1,
            url: "https://example.com/pull/1".to_string(),
            state: PrState::Open,
            ci_status: CiStatus::Passing,
            ci_summary: None,
            review_status: ReviewStatus::Approved,
            review_summary: None,
            updated_at: "2026-02-09T12:00:00Z".to_string(),
        };
        assert_eq!(
            compute_merge_readiness(&h, &Some(ws), Some(&pr)),
            MergeReadiness::Ready
        );
    }

    #[test]
    fn test_readiness_display() {
        assert_eq!(MergeReadiness::Ready.to_string(), "Ready");
        assert_eq!(MergeReadiness::NeedsPush.to_string(), "Needs push");
        assert_eq!(MergeReadiness::NeedsRebase.to_string(), "Needs rebase");
        assert_eq!(MergeReadiness::HasConflicts.to_string(), "Has conflicts");
        assert_eq!(
            MergeReadiness::ConflictCheckFailed.to_string(),
            "Conflict check failed"
        );
        assert_eq!(MergeReadiness::NeedsPr.to_string(), "Needs PR");
        assert_eq!(MergeReadiness::CiFailing.to_string(), "CI failing");
        assert_eq!(MergeReadiness::ReadyLocal.to_string(), "Ready (local)");
    }

    #[test]
    fn test_readiness_serde() {
        let json = serde_json::to_string(&MergeReadiness::NeedsRebase).unwrap();
        assert_eq!(json, "\"needs_rebase\"");

        let json = serde_json::to_string(&MergeReadiness::HasConflicts).unwrap();
        assert_eq!(json, "\"has_conflicts\"");

        let json = serde_json::to_string(&MergeReadiness::ConflictCheckFailed).unwrap();
        assert_eq!(json, "\"conflict_check_failed\"");

        let json = serde_json::to_string(&MergeReadiness::ReadyLocal).unwrap();
        assert_eq!(json, "\"ready_local\"");
    }
}
