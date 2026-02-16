use std::path::{Path, PathBuf};

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

pub fn generate_project_id(repo_path: &Path) -> kild_protocol::ProjectId {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    repo_path.hash(&mut hasher);
    kild_protocol::ProjectId::new(format!("{:x}", hasher.finish()))
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
    fn test_derive_project_name_from_remote_handles_bare_names() {
        assert_eq!(derive_project_name_from_remote("invalid-url"), "unknown");
    }

    #[test]
    fn test_generate_project_id_deterministic() {
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
    fn test_kild_branch_name_and_worktree_name_differ() {
        assert_eq!(KILD_BRANCH_PREFIX, "kild/");
    }
}
