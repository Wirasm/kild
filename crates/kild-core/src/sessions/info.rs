//! Session enrichment types and detection logic.
//!
//! Provides `SessionInfo`, which combines a `Session` with computed
//! process status, git status, and diff statistics. This is the enriched
//! view of a session used by UI and CLI consumers.

use std::path::Path;

use crate::git::operations::get_diff_stats;
use crate::git::types::DiffStats;
use crate::process::is_process_running;
use crate::sessions::types::{GitStatus, ProcessStatus, Session};
use crate::terminal::is_terminal_window_open;

/// Enriched session data combining a `Session` with computed status fields.
///
/// Created via `SessionInfo::from_session()`, which runs process detection,
/// git status checks, and diff stat computation.
///
/// Status fields reflect state at construction time and become stale as
/// processes start/stop and files change. Refresh via `from_session()` or
/// targeted field updates as needed.
///
/// Invariant: `diff_stats` is `Some` only when `git_status` is `Dirty`.
#[derive(Clone)]
pub struct SessionInfo {
    pub session: Session,
    pub process_status: ProcessStatus,
    pub git_status: GitStatus,
    pub diff_stats: Option<DiffStats>,
}

impl SessionInfo {
    /// Create a `SessionInfo` by enriching a `Session` with computed status.
    ///
    /// Runs process detection, git status check, and diff stat computation.
    pub fn from_session(session: Session) -> Self {
        let process_status = determine_process_status(&session);

        let git_status = if session.worktree_path.exists() {
            check_git_status(&session.worktree_path)
        } else {
            GitStatus::Unknown
        };

        let diff_stats = if git_status == GitStatus::Dirty {
            match get_diff_stats(&session.worktree_path) {
                Ok(stats) => Some(stats),
                Err(e) => {
                    tracing::warn!(
                        event = "core.session.diff_stats_failed",
                        path = %session.worktree_path.display(),
                        error = %e,
                        "Failed to compute diff stats"
                    );
                    None
                }
            }
        } else {
            None
        };

        Self {
            session,
            process_status,
            git_status,
            diff_stats,
        }
    }
}

/// Determine process status from session data.
///
/// Uses PID-based detection as primary method, falling back to window-based
/// detection for terminals like Ghostty where PID is unavailable.
///
/// Detection failures are logged as warnings and return:
/// - `ProcessStatus::Unknown` when PID check errors
/// - `ProcessStatus::Stopped` when window check errors or no detection method available
pub fn determine_process_status(session: &Session) -> ProcessStatus {
    if let Some(pid) = session.process_id {
        match is_process_running(pid) {
            Ok(true) => ProcessStatus::Running,
            Ok(false) => ProcessStatus::Stopped,
            Err(e) => {
                tracing::warn!(
                    event = "core.session.process_check_failed",
                    pid = pid,
                    branch = session.branch,
                    error = %e
                );
                ProcessStatus::Unknown
            }
        }
    } else if let (Some(terminal_type), Some(window_id)) =
        (&session.terminal_type, &session.terminal_window_id)
    {
        match is_terminal_window_open(terminal_type, window_id) {
            Ok(Some(true)) => ProcessStatus::Running,
            Ok(Some(false) | None) => ProcessStatus::Stopped,
            Err(e) => {
                tracing::warn!(
                    event = "core.session.window_check_failed",
                    terminal_type = ?terminal_type,
                    window_id = %window_id,
                    branch = session.branch,
                    error = %e
                );
                ProcessStatus::Stopped
            }
        }
    } else {
        ProcessStatus::Stopped
    }
}

/// Check if a worktree has uncommitted changes using `git status --porcelain`.
///
/// Returns `GitStatus::Dirty` if there are uncommitted changes,
/// `GitStatus::Clean` if the worktree is clean, or `GitStatus::Unknown`
/// if the git status check failed.
fn check_git_status(worktree_path: &Path) -> GitStatus {
    match std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output()
    {
        Ok(output) if output.status.success() => {
            if output.stdout.is_empty() {
                GitStatus::Clean
            } else {
                GitStatus::Dirty
            }
        }
        Ok(output) => {
            tracing::warn!(
                event = "core.session.git_status_failed",
                path = %worktree_path.display(),
                exit_code = ?output.status.code(),
                stderr = %String::from_utf8_lossy(&output.stderr),
                "Git status command failed"
            );
            GitStatus::Unknown
        }
        Err(e) => {
            tracing::warn!(
                event = "core.session.git_status_error",
                path = %worktree_path.display(),
                error = %e,
                "Failed to execute git status"
            );
            GitStatus::Unknown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::types::SessionStatus;
    use std::path::PathBuf;

    fn make_session(worktree_path: PathBuf) -> Session {
        Session {
            id: "test-id".to_string(),
            branch: "test-branch".to_string(),
            worktree_path,
            agent: "claude".to_string(),
            project_id: "test-project".to_string(),
            status: SessionStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            port_range_start: 0,
            port_range_end: 0,
            port_count: 0,
            process_id: None,
            process_name: None,
            process_start_time: None,
            terminal_type: None,
            terminal_window_id: None,
            command: String::new(),
            last_activity: None,
            note: None,
        }
    }

    #[test]
    fn test_determine_process_status_no_pid() {
        let session = make_session(PathBuf::from("/tmp/nonexistent"));
        assert_eq!(determine_process_status(&session), ProcessStatus::Stopped);
    }

    #[test]
    fn test_determine_process_status_dead_pid() {
        let mut session = make_session(PathBuf::from("/tmp/nonexistent"));
        session.process_id = Some(999999); // Non-existent PID
        assert_eq!(determine_process_status(&session), ProcessStatus::Stopped);
    }

    #[test]
    fn test_determine_process_status_live_pid() {
        let mut session = make_session(PathBuf::from("/tmp/nonexistent"));
        session.process_id = Some(std::process::id()); // Current process
        assert_eq!(determine_process_status(&session), ProcessStatus::Running);
    }

    #[test]
    fn test_from_session_nonexistent_path() {
        let session = make_session(PathBuf::from("/tmp/nonexistent-test-path"));
        let info = SessionInfo::from_session(session);
        assert_eq!(info.process_status, ProcessStatus::Stopped);
        assert_eq!(info.git_status, GitStatus::Unknown);
    }

    #[test]
    fn test_check_git_status_clean_repo() {
        use std::process::Command;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(path)
            .output()
            .unwrap();
        std::fs::write(path.join("test.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(path)
            .output()
            .unwrap();

        assert_eq!(check_git_status(path), GitStatus::Clean);
    }

    #[test]
    fn test_check_git_status_dirty_repo() {
        use std::process::Command;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();
        std::fs::write(path.join("test.txt"), "hello").unwrap();

        assert_eq!(check_git_status(path), GitStatus::Dirty);
    }

    #[test]
    fn test_check_git_status_non_git_directory() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        assert_eq!(check_git_status(temp_dir.path()), GitStatus::Unknown);
    }

    #[test]
    fn test_check_git_status_nonexistent_directory() {
        let path = Path::new("/nonexistent/path/that/does/not/exist");
        assert_eq!(check_git_status(path), GitStatus::Unknown);
    }

    #[test]
    fn test_from_session_dirty_repo_has_diff_stats() {
        use std::process::Command;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .output()
            .unwrap();
        std::fs::write(path.join("test.txt"), "line1\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(path)
            .output()
            .unwrap();

        // Make it dirty
        std::fs::write(path.join("test.txt"), "line1\nline2\nline3\n").unwrap();

        let session = make_session(path.to_path_buf());
        let info = SessionInfo::from_session(session);

        assert_eq!(info.git_status, GitStatus::Dirty);
        assert!(info.diff_stats.is_some());
        let stats = info.diff_stats.unwrap();
        assert_eq!(stats.insertions, 2);
        assert_eq!(stats.files_changed, 1);
    }
}
