use std::path::Path;

use git2::{Repository, Status, StatusOptions};
use tracing::{debug, warn};

use crate::git::errors::GitError;
use crate::git::types::{CommitCounts, DiffStats, GitStats, UncommittedDetails, WorktreeStatus};

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

#[cfg(test)]
mod tests {
    use super::*;
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

    // --- get_diff_stats tests ---

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
}
