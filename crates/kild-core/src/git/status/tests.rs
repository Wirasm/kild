use super::*;
use std::fs;
use std::path::Path;
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

    let repo = git2::Repository::open(dir.path()).unwrap();
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

    let repo = git2::Repository::open(local_dir.path()).unwrap();
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

    let repo = git2::Repository::open(local_dir.path()).unwrap();
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

// --- compute_base_metrics tests ---

#[test]
fn test_base_metrics_happy_path() {
    let dir = TempDir::new().unwrap();
    init_git_repo(dir.path());

    // Initial commit on main
    fs::write(dir.path().join("base.txt"), "base").unwrap();
    git_add_commit(dir.path(), "base commit");
    Command::new("git")
        .args(["branch", "-M", "main"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Create kild/feature branch with 2 commits
    Command::new("git")
        .args(["checkout", "-b", "kild/feature"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    fs::write(dir.path().join("feat1.txt"), "feature work").unwrap();
    git_add_commit(dir.path(), "feature commit 1");
    fs::write(dir.path().join("feat2.txt"), "more work").unwrap();
    git_add_commit(dir.path(), "feature commit 2");

    let stats = collect_git_stats(dir.path(), "feature", "main").unwrap();

    let drift = stats.drift.expect("drift should be Some");
    assert_eq!(drift.ahead, 2);
    assert_eq!(drift.behind, 0);
    assert_eq!(drift.base_branch, "main");

    let dvb = stats.diff_vs_base.expect("diff_vs_base should be Some");
    assert_eq!(dvb.files_changed, 2);
    assert!(dvb.insertions > 0);
}

#[test]
fn test_base_metrics_behind_base() {
    let dir = TempDir::new().unwrap();
    init_git_repo(dir.path());

    // Initial commit on main
    fs::write(dir.path().join("base.txt"), "base").unwrap();
    git_add_commit(dir.path(), "base commit");
    Command::new("git")
        .args(["branch", "-M", "main"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Branch off for kild
    Command::new("git")
        .args(["checkout", "-b", "kild/old-feature"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    fs::write(dir.path().join("old.txt"), "old work").unwrap();
    git_add_commit(dir.path(), "old feature work");

    // Main gets 2 new commits
    Command::new("git")
        .args(["checkout", "main"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    fs::write(dir.path().join("new1.txt"), "new on main").unwrap();
    git_add_commit(dir.path(), "main commit 1");
    fs::write(dir.path().join("new2.txt"), "more main").unwrap();
    git_add_commit(dir.path(), "main commit 2");

    // Switch back to kild branch for stats collection
    Command::new("git")
        .args(["checkout", "kild/old-feature"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stats = collect_git_stats(dir.path(), "old-feature", "main").unwrap();

    let drift = stats.drift.expect("drift should be Some");
    assert_eq!(drift.ahead, 1);
    assert_eq!(drift.behind, 2);
}

#[test]
fn test_base_metrics_missing_base_branch() {
    let dir = TempDir::new().unwrap();
    init_git_repo(dir.path());

    fs::write(dir.path().join("file.txt"), "content").unwrap();
    git_add_commit(dir.path(), "commit");
    Command::new("git")
        .args(["checkout", "-b", "kild/test"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let stats = collect_git_stats(dir.path(), "test", "nonexistent-base").unwrap();

    assert!(
        stats.drift.is_none(),
        "drift should be None when base branch missing"
    );
    assert!(
        stats.diff_vs_base.is_none(),
        "diff_vs_base should be None when base branch missing"
    );
    // Uncommitted metrics should still work independently
    assert!(stats.uncommitted_diff.is_some());
}

#[test]
fn test_base_metrics_missing_kild_branch() {
    let dir = TempDir::new().unwrap();
    init_git_repo(dir.path());

    fs::write(dir.path().join("file.txt"), "content").unwrap();
    git_add_commit(dir.path(), "commit");
    Command::new("git")
        .args(["branch", "-M", "main"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Request stats for branch that doesn't exist (no kild/nonexistent)
    let stats = collect_git_stats(dir.path(), "nonexistent", "main").unwrap();

    assert!(
        stats.drift.is_none(),
        "drift should be None when kild branch missing"
    );
    assert!(
        stats.diff_vs_base.is_none(),
        "diff_vs_base should be None when kild branch missing"
    );
}

#[test]
fn test_base_metrics_independent_from_uncommitted() {
    let dir = TempDir::new().unwrap();
    init_git_repo(dir.path());

    // Base commit on main
    fs::write(dir.path().join("base.txt"), "base content").unwrap();
    git_add_commit(dir.path(), "base commit");
    Command::new("git")
        .args(["branch", "-M", "main"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Create feature branch with committed work
    Command::new("git")
        .args(["checkout", "-b", "kild/test"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    fs::write(dir.path().join("feature.txt"), "committed feature work").unwrap();
    git_add_commit(dir.path(), "feature commit");

    // Add uncommitted changes (different from committed work)
    fs::write(dir.path().join("wip.txt"), "work in progress").unwrap();

    let stats = collect_git_stats(dir.path(), "test", "main").unwrap();

    // diff_vs_base reflects committed work only (1 file: feature.txt)
    let dvb = stats.diff_vs_base.expect("diff_vs_base should be Some");
    assert_eq!(
        dvb.files_changed, 1,
        "Only feature.txt is committed vs base"
    );

    // uncommitted_diff reflects local WIP changes (untracked wip.txt won't show
    // in index-to-workdir diff, but the test validates independence)
    assert!(stats.uncommitted_diff.is_some());
}

// --- collect_git_stats tests ---

#[test]
fn test_collect_git_stats_nonexistent_path() {
    let result = collect_git_stats(Path::new("/nonexistent/path"), "test-branch", "main");
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

    let stats = collect_git_stats(dir.path(), "test-branch", "main");
    assert!(stats.is_some());
    let stats = stats.unwrap();
    assert!(stats.uncommitted_diff.is_some());
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

    let stats = collect_git_stats(dir.path(), "test-branch", "main");
    assert!(stats.is_some());
    let stats = stats.unwrap();
    assert!(stats.has_data());
    assert!(stats.uncommitted_diff.is_some());
    let diff = stats.uncommitted_diff.unwrap();
    assert!(diff.insertions > 0 || diff.deletions > 0);
}
