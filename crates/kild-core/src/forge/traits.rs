//! Forge backend trait definition.

use std::path::Path;

use crate::forge::errors::ForgeError;
use crate::forge::types::{MergeStrategy, PrCheckResult, PrInfo};

/// Trait defining the interface for forge (code hosting) backends.
///
/// Each supported forge (GitHub, GitLab, etc.) implements this trait
/// to provide platform-specific PR/MR operations.
///
/// Callers obtain backends via `get_forge_backend()`, which guarantees
/// `is_available()` is true before returning a backend reference.
pub trait ForgeBackend: Send + Sync {
    /// The canonical name of this forge (e.g., "github", "gitlab").
    fn name(&self) -> &'static str;

    /// The user-facing display name (e.g., "GitHub", "GitLab").
    fn display_name(&self) -> &'static str;

    /// Whether this forge's CLI tooling is available on the system.
    fn is_available(&self) -> bool;

    /// Check if a merged PR/MR exists for the given branch.
    fn is_pr_merged(&self, worktree_path: &Path, branch: &str) -> Result<bool, ForgeError>;

    /// Check if any PR/MR exists for the given branch.
    fn check_pr_exists(&self, worktree_path: &Path, branch: &str) -> PrCheckResult;

    /// Fetch rich PR/MR info (number, URL, state, CI, reviews).
    ///
    /// Returns `Ok(Some(info))` if PR found, `Ok(None)` if no PR exists,
    /// or `Err(ForgeError)` if the fetch itself failed.
    fn fetch_pr_info(
        &self,
        worktree_path: &Path,
        branch: &str,
    ) -> Result<Option<PrInfo>, ForgeError>;

    /// Merge a PR using the specified strategy.
    ///
    /// Calls the forge CLI to merge the PR. The `--delete-branch` flag is NOT
    /// passed because KILD manages remote branch cleanup separately (the worktree
    /// blocks `gh`'s local branch deletion).
    ///
    /// Returns `Ok(())` on successful merge, or `Err(ForgeError)` on failure.
    fn merge_pr(
        &self,
        worktree_path: &Path,
        branch: &str,
        strategy: MergeStrategy,
    ) -> Result<(), ForgeError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockForge;

    impl ForgeBackend for MockForge {
        fn name(&self) -> &'static str {
            "mock"
        }

        fn display_name(&self) -> &'static str {
            "Mock Forge"
        }

        fn is_available(&self) -> bool {
            true
        }

        fn is_pr_merged(&self, _worktree_path: &Path, _branch: &str) -> Result<bool, ForgeError> {
            Ok(false)
        }

        fn check_pr_exists(&self, _worktree_path: &Path, _branch: &str) -> PrCheckResult {
            PrCheckResult::Unavailable
        }

        fn fetch_pr_info(
            &self,
            _worktree_path: &Path,
            _branch: &str,
        ) -> Result<Option<PrInfo>, ForgeError> {
            Ok(None)
        }

        fn merge_pr(
            &self,
            _worktree_path: &Path,
            _branch: &str,
            _strategy: MergeStrategy,
        ) -> Result<(), ForgeError> {
            Ok(())
        }
    }

    #[test]
    fn test_forge_backend_basic_methods() {
        let backend = MockForge;
        assert_eq!(backend.name(), "mock");
        assert_eq!(backend.display_name(), "Mock Forge");
        assert!(backend.is_available());
    }

    #[test]
    fn test_forge_backend_pr_methods() {
        let backend = MockForge;
        let path = Path::new("/tmp");
        assert!(!backend.is_pr_merged(path, "test").unwrap());
        assert!(backend.check_pr_exists(path, "test").is_unavailable());
        assert!(backend.fetch_pr_info(path, "test").unwrap().is_none());
    }
}
