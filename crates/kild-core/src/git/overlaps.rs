use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use git2::{Oid, Repository};
use tracing::{info, warn};

use kild_git::health::{find_merge_base, resolve_branch_oid};
use kild_git::naming::kild_branch_name;
use kild_git::types::{CleanKild, FileOverlap, OverlapReport};

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
                    session.branch.to_string(),
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
                    session.branch.to_string(),
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
                    session.branch.to_string(),
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
                    session.branch.to_string(),
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
                files_by_branch.insert(session.branch.to_string(), files);
            }
            Err(detail) => {
                errors.push((session.branch.to_string(), detail));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_git_repo(dir: &std::path::Path) {
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
    fn git_add_commit(dir: &std::path::Path, msg: &str) {
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

    fn make_test_session(branch: &str, worktree_path: PathBuf) -> crate::Session {
        crate::Session::new_for_test(branch.to_string(), worktree_path)
    }

    /// Helper: set up an independent git repo with a main branch, an initial file set,
    /// and a kild branch that modifies specified files. Each repo is independent (no clone)
    /// to avoid cross-platform issues with `git clone` into existing TempDir directories.
    fn setup_kild_repo(
        dir: &std::path::Path,
        branch: &str,
        initial_files: &[&str],
        modify_files: &[&str],
    ) {
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
}
